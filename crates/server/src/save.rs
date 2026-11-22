//! Sauvegarde de la partie dans `DATA_DIR/world.sav` : modifications du monde, heure,
//! et état des joueurs (par pseudo). Sauvegarde automatique toutes les minutes et à l'arrêt.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use shared::block::Block;
use shared::config::WorldConfig;
use shared::daytime::GameTime;
use shared::item::{Inventory, Item};
use shared::survivor::{Order, SurvivorStats};

use crate::daynight::Clock;
use crate::net::{ConnectedPlayer, Lobby, WorldVersion};
use crate::world::{Chests, Deposits, GameWorld};
use crate::{ServerConfig, ShutdownFlag};

/// Change si le format du fichier change de façon incompatible.
pub const SAVE_VERSION: u32 = 6;
/// Intervalle entre deux sauvegardes automatiques, en secondes.
const AUTOSAVE_SECS: f32 = 60.0;
const FILE_NAME: &str = "world.sav";

pub struct SavePlugin;

impl Plugin for SavePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SavedPlayers>()
            .add_systems(Update, autosave)
            .add_systems(Last, save_on_shutdown.before(crate::exit_on_shutdown));
    }
}

/// Ce qu'on garde d'un joueur entre deux connexions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedPlayer {
    pub position: Vec3,
    pub health: f32,
    pub hunger: f32,
    pub inventory: Inventory,
}

impl SavedPlayer {
    pub fn from_player(player: &ConnectedPlayer) -> Self {
        Self {
            position: player.state.position,
            health: player.health,
            hunger: player.hunger,
            inventory: player.inventory.clone(),
        }
    }
}

/// Un survivant recruté, gardé d'une partie à l'autre.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedSurvivor {
    pub name: String,
    pub stats: SurvivorStats,
    pub health: f32,
    pub position: Vec3,
    /// Endroit gardé, récolté ou réparé.
    pub post: Vec3,
    pub leader: Option<String>,
    pub order: Order,
    /// Arme confiée (depuis la version 5).
    pub weapon: Option<Item>,
    /// Casque et armure confiés (depuis la version 6).
    pub armor: [Option<Item>; 2],
}

/// Survivant tel qu'enregistré en version 5, avant les armures.
#[derive(Deserialize)]
struct SavedSurvivorV5 {
    name: String,
    stats: SurvivorStats,
    health: f32,
    position: Vec3,
    post: Vec3,
    leader: Option<String>,
    order: Order,
    weapon: Option<Item>,
}

impl From<SavedSurvivorV5> for SavedSurvivor {
    fn from(s: SavedSurvivorV5) -> Self {
        Self {
            name: s.name,
            stats: s.stats,
            health: s.health,
            position: s.position,
            post: s.post,
            leader: s.leader,
            order: s.order,
            weapon: s.weapon,
            armor: [None; 2],
        }
    }
}

/// Survivant tel qu'enregistré en version 4, avant les armes.
#[derive(Deserialize)]
struct SavedSurvivorV4 {
    name: String,
    stats: SurvivorStats,
    health: f32,
    position: Vec3,
    post: Vec3,
    leader: Option<String>,
    order: Order,
}

impl From<SavedSurvivorV4> for SavedSurvivor {
    fn from(s: SavedSurvivorV4) -> Self {
        Self {
            name: s.name,
            stats: s.stats,
            health: s.health,
            position: s.position,
            post: s.post,
            leader: s.leader,
            order: s.order,
            weapon: None,
            armor: [None; 2],
        }
    }
}

/// Survivants recrutés, tenus à jour par la simulation pour la sauvegarde.
#[derive(Resource, Default, Debug)]
pub struct SavedSurvivors(pub Vec<SavedSurvivor>);

/// Joueurs connus mais absents, par pseudo.
#[derive(Resource, Default, Debug)]
pub struct SavedPlayers(pub HashMap<String, SavedPlayer>);

/// Contenu du fichier de sauvegarde.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct SaveFile {
    pub version: u32,
    pub world: WorldConfig,
    pub time: GameTime,
    pub edits: Vec<(IVec3, Block)>,
    pub players: Vec<(String, SavedPlayer)>,
    /// Récoltes restantes des filons entamés (depuis la version 2).
    pub deposits: Vec<(IVec3, u8)>,
    /// Contenu des coffres (depuis la version 3).
    pub chests: Vec<(IVec3, Inventory)>,
    /// Survivants recrutés (depuis la version 4).
    pub survivors: Vec<SavedSurvivor>,
}

pub fn save_path(data_dir: &Path) -> PathBuf {
    data_dir.join(FILE_NAME)
}

/// Lit la sauvegarde. `Ok(None)` s'il n'y en a pas encore.
pub fn load(data_dir: &Path) -> io::Result<Option<SaveFile>> {
    let bytes = match std::fs::read(save_path(data_dir)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let invalid = |e: postcard::Error| io::Error::new(io::ErrorKind::InvalidData, e.to_string());
    // Chaque version ajoute des champs à la fin : on lit le début commun, puis ce que
    // la version du fichier contient. Une ancienne sauvegarde se lit donc toujours.
    type Core = (
        u32,
        WorldConfig,
        GameTime,
        Vec<(IVec3, Block)>,
        Vec<(String, SavedPlayer)>,
    );
    let ((version, world, time, edits, players), rest) = postcard::take_from_bytes::<Core>(&bytes).map_err(invalid)?;
    let (deposits, rest) = if version >= 2 {
        postcard::take_from_bytes(rest).map_err(invalid)?
    } else {
        (Vec::new(), rest)
    };
    let (chests, rest) = if version >= 3 {
        postcard::take_from_bytes(rest).map_err(invalid)?
    } else {
        (Vec::new(), rest)
    };
    let survivors: Vec<SavedSurvivor> = if version >= 6 {
        postcard::take_from_bytes(rest).map_err(invalid)?.0
    } else if version == 5 {
        let (old, _) = postcard::take_from_bytes::<Vec<SavedSurvivorV5>>(rest).map_err(invalid)?;
        old.into_iter().map(Into::into).collect()
    } else if version == 4 {
        let (old, _) = postcard::take_from_bytes::<Vec<SavedSurvivorV4>>(rest).map_err(invalid)?;
        old.into_iter().map(Into::into).collect()
    } else {
        Vec::new()
    };
    if version > SAVE_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("sauvegarde de version {version}, plus récente que ce serveur"),
        ));
    }
    let save = SaveFile {
        version: SAVE_VERSION,
        world,
        time,
        edits,
        players,
        deposits,
        chests,
        survivors,
    };
    Ok(Some(save))
}

/// Écrit la sauvegarde dans un fichier temporaire puis le renomme : un arrêt brutal
/// pendant l'écriture ne corrompt pas l'ancienne sauvegarde.
pub fn write(data_dir: &Path, save: &SaveFile) -> io::Result<()> {
    std::fs::create_dir_all(data_dir)?;
    let bytes = postcard::to_allocvec(save).map_err(|e| io::Error::other(e.to_string()))?;
    let tmp = data_dir.join(format!("{FILE_NAME}.tmp"));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, save_path(data_dir))
}

fn snapshot(
    world: &GameWorld,
    clock: &Clock,
    lobby: &Lobby,
    saved: &SavedPlayers,
    deposits: &Deposits,
    chests: &Chests,
    survivors: &SavedSurvivors,
) -> SaveFile {
    let mut players = saved.0.clone();
    for player in lobby.players.values().filter(|p| p.is_ready()) {
        players.insert(player.name.clone(), SavedPlayer::from_player(player));
    }
    let mut edits: Vec<_> = world.edits().iter().map(|(p, b)| (*p, *b)).collect();
    edits.sort_by_key(|(p, _)| (p.x, p.y, p.z));
    let mut players: Vec<_> = players.into_iter().collect();
    players.sort_by(|a, b| a.0.cmp(&b.0));
    let mut deposits: Vec<_> = deposits.0.iter().map(|(p, n)| (*p, *n)).collect();
    deposits.sort_by_key(|(p, _)| (p.x, p.y, p.z));
    let mut chests: Vec<_> = chests.0.iter().map(|(p, inv)| (*p, inv.clone())).collect();
    chests.sort_by_key(|(p, _)| (p.x, p.y, p.z));
    SaveFile {
        version: SAVE_VERSION,
        world: world.config().clone(),
        time: clock.0,
        edits,
        players,
        deposits,
        chests,
        survivors: survivors.0.clone(),
    }
}

fn save_now(
    config: &ServerConfig,
    world: &GameWorld,
    clock: &Clock,
    lobby: &Lobby,
    saved: &SavedPlayers,
    deposits: &Deposits,
    chests: &Chests,
    survivors: &SavedSurvivors,
) {
    let save = snapshot(world, clock, lobby, saved, deposits, chests, survivors);
    match write(&config.data_dir, &save) {
        Ok(()) => debug!(
            "Partie sauvegardée : {} blocs modifiés, {} joueurs",
            save.edits.len(),
            save.players.len()
        ),
        Err(error) => error!(
            "Sauvegarde impossible dans {} : {error}",
            save_path(&config.data_dir).display()
        ),
    }
}

fn autosave(
    time: Res<Time>,
    config: Res<ServerConfig>,
    world: Res<GameWorld>,
    clock: Res<Clock>,
    lobby: Res<Lobby>,
    saved: Res<SavedPlayers>,
    version: Res<WorldVersion>,
    deposits: Res<Deposits>,
    chests: Res<Chests>,
    survivors: Res<SavedSurvivors>,
    mut since: Local<f32>,
    mut last_version: Local<u64>,
) {
    *since += time.delta_secs();
    if *since < AUTOSAVE_SECS {
        return;
    }
    *since = 0.0;
    // Rien à sauvegarder si personne n'a joué depuis la dernière fois.
    let someone_here = lobby.players.values().any(ConnectedPlayer::is_ready);
    if !someone_here && *last_version == version.0 {
        return;
    }
    *last_version = version.0;
    save_now(&config, &world, &clock, &lobby, &saved, &deposits, &chests, &survivors);
}

fn save_on_shutdown(
    flag: Res<ShutdownFlag>,
    config: Res<ServerConfig>,
    world: Res<GameWorld>,
    clock: Res<Clock>,
    lobby: Res<Lobby>,
    saved: Res<SavedPlayers>,
    deposits: Res<Deposits>,
    chests: Res<Chests>,
    survivors: Res<SavedSurvivors>,
    mut done: Local<bool>,
) {
    if flag.0.load(Ordering::Relaxed) && !*done {
        *done = true;
        save_now(&config, &world, &clock, &lobby, &saved, &deposits, &chests, &survivors);
        info!("Partie sauvegardée dans {}", save_path(&config.data_dir).display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::item::Item;

    fn loaded_fields(dir: &Path) -> SaveFile {
        load(dir).unwrap().unwrap()
    }

    #[test]
    fn save_file_round_trip() {
        let dir = std::env::temp_dir().join(format!("zs-save-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(load(&dir).unwrap().is_none());
        let mut inventory = Inventory::default();
        inventory.add(Item::Apple, 2);
        let save = SaveFile {
            version: SAVE_VERSION,
            world: WorldConfig::default(),
            time: GameTime::START,
            edits: vec![(IVec3::new(1, 2, 3), Block::Planks)],
            players: vec![(
                "Alice".into(),
                SavedPlayer {
                    position: Vec3::new(1.0, 2.0, 3.0),
                    health: 50.0,
                    hunger: 70.0,
                    inventory,
                },
            )],
            deposits: vec![(IVec3::new(4, 5, 6), 7)],
            chests: vec![(IVec3::new(8, 9, 10), Inventory::default())],
            survivors: vec![SavedSurvivor {
                name: "Marie".into(),
                stats: SurvivorStats::from_seed(42),
                health: 80.0,
                position: Vec3::new(1.0, 30.0, 2.0),
                post: Vec3::new(1.0, 30.0, 2.0),
                leader: Some("Alice".into()),
                order: Order::Guard,
                weapon: Some(Item::Bow),
                armor: [Some(Item::Helmet(shared::item::Tier::Iron)), None],
            }],
        };
        write(&dir, &save).unwrap();
        assert_eq!(load(&dir).unwrap(), Some(save));

        // Une sauvegarde de la version 1 (sans filons) se lit toujours.
        #[derive(Serialize)]
        struct V1<'a>(
            u32,
            &'a WorldConfig,
            GameTime,
            &'a [(IVec3, Block)],
            &'a [(String, SavedPlayer)],
        );
        let old = loaded_fields(&dir);
        let bytes = postcard::to_allocvec(&V1(1, &old.world, old.time, &old.edits, &old.players)).unwrap();
        std::fs::write(save_path(&dir), bytes).unwrap();
        let upgraded = load(&dir).unwrap().unwrap();
        assert_eq!(upgraded.edits, old.edits);
        assert!(upgraded.deposits.is_empty() && upgraded.chests.is_empty() && upgraded.survivors.is_empty());

        // Une sauvegarde de la version 4 garde ses survivants, sans arme.
        #[derive(Serialize)]
        struct V4<'a>(
            u32,
            &'a WorldConfig,
            GameTime,
            &'a [(IVec3, Block)],
            &'a [(String, SavedPlayer)],
            &'a [(IVec3, u8)],
            &'a [(IVec3, Inventory)],
            Vec<(&'a str, SurvivorStats, f32, Vec3, Vec3, Option<String>, Order)>,
        );
        let v4_survivors = vec![(
            "Marie",
            SurvivorStats::from_seed(42),
            80.0,
            Vec3::ONE,
            Vec3::ONE,
            None,
            Order::Gather,
        )];
        let bytes = postcard::to_allocvec(&V4(
            4,
            &old.world,
            old.time,
            &old.edits,
            &old.players,
            &[],
            &[],
            v4_survivors,
        ))
        .unwrap();
        std::fs::write(save_path(&dir), bytes).unwrap();
        let upgraded = load(&dir).unwrap().unwrap();
        assert_eq!(upgraded.survivors.len(), 1);
        assert_eq!(upgraded.survivors[0].order, Order::Gather);
        assert_eq!(upgraded.survivors[0].weapon, None);
        assert_eq!(upgraded.survivors[0].armor, [None; 2]);

        std::fs::write(save_path(&dir), b"abime").unwrap();
        assert!(load(&dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
