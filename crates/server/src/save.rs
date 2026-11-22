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
use shared::item::Inventory;

use crate::daynight::Clock;
use crate::net::{ConnectedPlayer, Lobby, WorldVersion};
use crate::world::GameWorld;
use crate::{ServerConfig, ShutdownFlag};

/// Change si le format du fichier change de façon incompatible.
const SAVE_VERSION: u32 = 1;
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
    let save: SaveFile =
        postcard::from_bytes(&bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    if save.version != SAVE_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("version de sauvegarde {} inconnue", save.version),
        ));
    }
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

fn snapshot(world: &GameWorld, clock: &Clock, lobby: &Lobby, saved: &SavedPlayers) -> SaveFile {
    let mut players = saved.0.clone();
    for player in lobby.players.values().filter(|p| p.is_ready()) {
        players.insert(player.name.clone(), SavedPlayer::from_player(player));
    }
    let mut edits: Vec<_> = world.edits().iter().map(|(p, b)| (*p, *b)).collect();
    edits.sort_by_key(|(p, _)| (p.x, p.y, p.z));
    let mut players: Vec<_> = players.into_iter().collect();
    players.sort_by(|a, b| a.0.cmp(&b.0));
    SaveFile {
        version: SAVE_VERSION,
        world: world.config().clone(),
        time: clock.0,
        edits,
        players,
    }
}

fn save_now(config: &ServerConfig, world: &GameWorld, clock: &Clock, lobby: &Lobby, saved: &SavedPlayers) {
    let save = snapshot(world, clock, lobby, saved);
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
    save_now(&config, &world, &clock, &lobby, &saved);
}

fn save_on_shutdown(
    flag: Res<ShutdownFlag>,
    config: Res<ServerConfig>,
    world: Res<GameWorld>,
    clock: Res<Clock>,
    lobby: Res<Lobby>,
    saved: Res<SavedPlayers>,
    mut done: Local<bool>,
) {
    if flag.0.load(Ordering::Relaxed) && !*done {
        *done = true;
        save_now(&config, &world, &clock, &lobby, &saved);
        info!("Partie sauvegardée dans {}", save_path(&config.data_dir).display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::item::Item;

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
        };
        write(&dir, &save).unwrap();
        assert_eq!(load(&dir).unwrap(), Some(save));
        std::fs::write(save_path(&dir), b"abime").unwrap();
        assert!(load(&dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
