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

use crate::claims::Claims;
use crate::daynight::Clock;
use crate::net::{ConnectedPlayer, Lobby, WorldVersion};
use crate::world::{Chests, Deposits, GameWorld};
use crate::{ServerConfig, ShutdownFlag};
use shared::achievements::Record;
use shared::rescue::RescueState;

use crate::rescue::Rescue;
use shared::skills::Progress;

/// Change si le format du fichier change de façon incompatible.
pub const SAVE_VERSION: u32 = 12;
/// Expérience donnée aux joueurs des sauvegardes d'avant les compétences (niveau 6).
pub const LEGACY_XP: u32 = {
    let mut xp = 0;
    let mut level = 1;
    while level < 6 {
        xp += shared::skills::xp_for_next(level);
        level += 1;
    }
    xp
};
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
    /// Soif (depuis la version 7).
    pub thirst: f32,
    /// Infection : enregistrée à part, à la fin du fichier (depuis la version 8).
    #[serde(skip)]
    pub infection: f32,
    /// Expérience et compétences : enregistrées à part, à la fin du fichier (depuis la version 10).
    #[serde(skip)]
    pub progress: Progress,
    /// Statistiques et succès : enregistrés à part, à la fin du fichier (depuis la version 11).
    #[serde(skip)]
    pub record: Record,
}

impl SavedPlayer {
    pub fn from_player(player: &ConnectedPlayer) -> Self {
        Self {
            position: player.state.position,
            health: player.health,
            hunger: player.hunger,
            inventory: player.inventory.clone(),
            thirst: player.thirst,
            infection: player.infection,
            progress: player.progress.clone(),
            record: player.record.clone(),
        }
    }
}

/// Joueur tel qu'enregistré jusqu'à la version 6, avant la soif.
#[derive(Serialize, Deserialize)]
struct SavedPlayerV6 {
    position: Vec3,
    health: f32,
    hunger: f32,
    inventory: Inventory,
}

impl From<SavedPlayerV6> for SavedPlayer {
    fn from(p: SavedPlayerV6) -> Self {
        Self {
            position: p.position,
            health: p.health,
            hunger: p.hunger,
            inventory: p.inventory,
            thirst: crate::net::MAX_THIRST,
            infection: 0.0,
            progress: Progress::default(),
            record: Record::default(),
        }
    }
}

/// Paramètres du monde tels qu'enregistrés jusqu'à la version 6 : terrain d'origine.
#[derive(Serialize, Deserialize)]
struct WorldConfigV6 {
    seed: i32,
    size_chunks: i32,
    height_chunks: i32,
}

impl From<WorldConfigV6> for WorldConfig {
    fn from(w: WorldConfigV6) -> Self {
        Self {
            seed: w.seed,
            size_chunks: w.size_chunks,
            height_chunks: w.height_chunks,
            generation: 1,
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
    /// Joueurs infectés et avancée de l'infection (depuis la version 8).
    pub infections: Vec<(String, f32)>,
    /// Bannières plantées et leur propriétaire (depuis la version 9).
    pub claims: Vec<(IVec3, String)>,
    /// Expérience et compétences des joueurs (depuis la version 10).
    pub progress: Vec<(String, Progress)>,
    /// Statistiques et succès des joueurs (depuis la version 11).
    pub records: Vec<(String, Record)>,
    /// Appel de détresse en cours (depuis la version 12).
    pub rescue: Option<RescueState>,
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
    // Jusqu'à la version 6, ni génération du terrain ni soif : on relit l'ancien début.
    let (version, _) = postcard::take_from_bytes::<u32>(&bytes).map_err(invalid)?;
    type Core<W, P> = (u32, W, GameTime, Vec<(IVec3, Block)>, Vec<(String, P)>);
    let ((_, world, time, edits, players), rest) = if version >= 7 {
        postcard::take_from_bytes::<Core<WorldConfig, SavedPlayer>>(&bytes).map_err(invalid)?
    } else {
        let ((v, world, time, edits, players), rest) =
            postcard::take_from_bytes::<Core<WorldConfigV6, SavedPlayerV6>>(&bytes).map_err(invalid)?;
        let players = players.into_iter().map(|(name, p)| (name, p.into())).collect();
        ((v, world.into(), time, edits, players), rest)
    };
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
    let (survivors, rest): (Vec<SavedSurvivor>, &[u8]) = if version >= 6 {
        postcard::take_from_bytes(rest).map_err(invalid)?
    } else if version == 5 {
        let (old, rest) = postcard::take_from_bytes::<Vec<SavedSurvivorV5>>(rest).map_err(invalid)?;
        (old.into_iter().map(Into::into).collect(), rest)
    } else if version == 4 {
        let (old, rest) = postcard::take_from_bytes::<Vec<SavedSurvivorV4>>(rest).map_err(invalid)?;
        (old.into_iter().map(Into::into).collect(), rest)
    } else {
        (Vec::new(), rest)
    };
    let (infections, rest): (Vec<(String, f32)>, &[u8]) = if version >= 8 {
        postcard::take_from_bytes(rest).map_err(invalid)?
    } else {
        (Vec::new(), rest)
    };
    let (claims, rest): (Vec<(IVec3, String)>, &[u8]) = if version >= 9 {
        postcard::take_from_bytes(rest).map_err(invalid)?
    } else {
        (Vec::new(), rest)
    };
    let (progress, rest): (Vec<(String, Progress)>, &[u8]) = if version >= 10 {
        postcard::take_from_bytes(rest).map_err(invalid)?
    } else {
        (Vec::new(), rest)
    };
    let (records, rest): (Vec<(String, Record)>, &[u8]) = if version >= 11 {
        postcard::take_from_bytes(rest).map_err(invalid)?
    } else {
        (Vec::new(), rest)
    };
    let rescue: Option<RescueState> = if version >= 12 {
        postcard::take_from_bytes(rest).map_err(invalid)?.0
    } else {
        None
    };
    let mut players = players;
    for (name, player) in &mut players {
        if let Some((_, amount)) = infections.iter().find(|(n, _)| n == name) {
            player.infection = *amount;
        }
        if let Some((_, p)) = progress.iter().find(|(n, _)| n == name) {
            player.progress = p.clone();
        } else if version < 10 {
            // Les joueurs d'avant les compétences repartent avec de quoi débloquer l'essentiel.
            player.progress.xp = LEGACY_XP;
        }
        if let Some((_, r)) = records.iter().find(|(n, _)| n == name) {
            player.record = r.clone();
        }
    }
    if version > SAVE_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("sauvegarde de version {version}, plus récente que ce serveur"),
        ));
    }
    let progress = players.iter().map(|(n, p)| (n.clone(), p.progress.clone())).collect();
    let records = players
        .iter()
        .filter(|(_, p)| p.record != Record::default())
        .map(|(n, p)| (n.clone(), p.record.clone()))
        .collect();
    let save = SaveFile {
        version: SAVE_VERSION,
        world,
        time,
        edits,
        players,
        deposits,
        chests,
        survivors,
        infections,
        claims,
        progress,
        records,
        rescue,
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

#[allow(clippy::too_many_arguments)]
fn snapshot(
    world: &GameWorld,
    clock: &Clock,
    lobby: &Lobby,
    saved: &SavedPlayers,
    deposits: &Deposits,
    chests: &Chests,
    survivors: &SavedSurvivors,
    claims: &Claims,
    rescue: &Rescue,
) -> SaveFile {
    let mut players = saved.0.clone();
    for player in lobby.players.values().filter(|p| p.is_ready()) {
        players.insert(player.name.clone(), SavedPlayer::from_player(player));
    }
    let mut edits: Vec<_> = world.edits().iter().map(|(p, b)| (*p, *b)).collect();
    edits.sort_by_key(|(p, _)| (p.x, p.y, p.z));
    let mut players: Vec<_> = players.into_iter().collect();
    players.sort_by(|a, b| a.0.cmp(&b.0));
    let progress = players
        .iter()
        .filter(|(_, p)| p.progress != Progress::default())
        .map(|(name, p)| (name.clone(), p.progress.clone()))
        .collect();
    let records = players
        .iter()
        .filter(|(_, p)| p.record != Record::default())
        .map(|(name, p)| (name.clone(), p.record.clone()))
        .collect();
    let infections = players
        .iter()
        .filter(|(_, p)| p.infection > 0.0)
        .map(|(name, p)| (name.clone(), p.infection))
        .collect();
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
        infections,
        claims: {
            let mut claims: Vec<_> = claims.0.iter().map(|(p, n)| (*p, n.clone())).collect();
            claims.sort_by_key(|(p, _)| (p.x, p.y, p.z));
            claims
        },
        progress,
        records,
        rescue: rescue.0,
    }
}

#[allow(clippy::too_many_arguments)]
fn save_now(
    config: &ServerConfig,
    world: &GameWorld,
    clock: &Clock,
    lobby: &Lobby,
    saved: &SavedPlayers,
    deposits: &Deposits,
    chests: &Chests,
    survivors: &SavedSurvivors,
    claims: &Claims,
    rescue: &Rescue,
) {
    let save = snapshot(world, clock, lobby, saved, deposits, chests, survivors, claims, rescue);
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
    claims: Res<Claims>,
    rescue: Res<Rescue>,
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
    save_now(
        &config, &world, &clock, &lobby, &saved, &deposits, &chests, &survivors, &claims, &rescue,
    );
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
    claims: Res<Claims>,
    rescue: Res<Rescue>,
    mut done: Local<bool>,
) {
    if flag.0.load(Ordering::Relaxed) && !*done {
        *done = true;
        save_now(
            &config, &world, &clock, &lobby, &saved, &deposits, &chests, &survivors, &claims, &rescue,
        );
        info!("Partie sauvegardée dans {}", save_path(&config.data_dir).display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::item::Item;
    use shared::skills::Skill;

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
                    thirst: 40.0,
                    infection: 30.0,
                    progress: Progress {
                        xp: 500,
                        skills: [(Skill::Brawler, 2)].into_iter().collect(),
                    },
                    record: Record {
                        nights: 3,
                        kills: vec![4, 1],
                        unlocked: vec![shared::achievements::Achievement::FirstBlood],
                        ..default()
                    },
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
            infections: vec![("Alice".into(), 30.0)],
            claims: vec![(IVec3::new(5, 40, 5), "Alice".into())],
            progress: vec![(
                "Alice".into(),
                Progress {
                    xp: 500,
                    skills: [(Skill::Brawler, 2)].into_iter().collect(),
                },
            )],
            records: vec![(
                "Alice".into(),
                Record {
                    nights: 3,
                    kills: vec![4, 1],
                    unlocked: vec![shared::achievements::Achievement::FirstBlood],
                    ..default()
                },
            )],
            rescue: Some(RescueState::new(IVec3::new(7, 40, 7), 4)),
        };
        write(&dir, &save).unwrap();
        assert_eq!(load(&dir).unwrap(), Some(save));

        // Une sauvegarde de la version 1 (sans filons) se lit toujours.
        #[derive(Serialize)]
        struct V1<'a>(
            u32,
            &'a WorldConfigV6,
            GameTime,
            &'a [(IVec3, Block)],
            &'a [(String, SavedPlayerV6)],
        );
        let old = loaded_fields(&dir);
        let old_world = WorldConfigV6 {
            seed: old.world.seed,
            size_chunks: old.world.size_chunks,
            height_chunks: old.world.height_chunks,
        };
        let old_players: Vec<(String, SavedPlayerV6)> = old
            .players
            .iter()
            .map(|(name, p)| {
                let p = SavedPlayerV6 {
                    position: p.position,
                    health: p.health,
                    hunger: p.hunger,
                    inventory: p.inventory.clone(),
                };
                (name.clone(), p)
            })
            .collect();
        let bytes = postcard::to_allocvec(&V1(1, &old_world, old.time, &old.edits, &old_players)).unwrap();
        std::fs::write(save_path(&dir), bytes).unwrap();
        let upgraded = load(&dir).unwrap().unwrap();
        assert_eq!(upgraded.edits, old.edits);
        // Un ancien monde garde son terrain d'origine, et ses joueurs n'ont pas soif.
        assert_eq!(upgraded.world.generation, 1);
        assert_eq!(upgraded.players[0].1.thirst, crate::net::MAX_THIRST);
        assert!(upgraded.deposits.is_empty() && upgraded.chests.is_empty() && upgraded.survivors.is_empty());

        // Une sauvegarde de la version 4 garde ses survivants, sans arme.
        #[derive(Serialize)]
        struct V4<'a>(
            u32,
            &'a WorldConfigV6,
            GameTime,
            &'a [(IVec3, Block)],
            &'a [(String, SavedPlayerV6)],
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
            &old_world,
            old.time,
            &old.edits,
            &old_players,
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
