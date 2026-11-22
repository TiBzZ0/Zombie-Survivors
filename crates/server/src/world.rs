//! État du monde côté serveur : le terrain généré et les modifications des joueurs.

use bevy::prelude::*;
use shared::world::WorldState;

use crate::ServerConfig;
use crate::daynight::Clock;
use crate::save::{self, SavedPlayers};

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, setup_world);
    }
}

/// Le monde de la partie. Seul le serveur a le droit de le modifier.
#[derive(Resource, Deref, DerefMut)]
pub struct GameWorld(pub WorldState);

/// Point d'apparition des joueurs, au centre de la carte, posé sur le sol.
#[derive(Resource, Clone, Copy, Debug)]
pub struct SpawnPoint(pub Vec3);

fn setup_world(mut commands: Commands, mut config: ResMut<ServerConfig>) {
    let save = match save::load(&config.data_dir) {
        Ok(save) => save,
        Err(error) => {
            // On met le fichier illisible de côté plutôt que de l'écraser.
            let path = save::save_path(&config.data_dir);
            let aside = path.with_extension("sav.illisible");
            error!(
                "Sauvegarde illisible ({error}) : déplacée vers {}, nouvelle partie",
                aside.display()
            );
            let _ = std::fs::rename(&path, &aside);
            None
        }
    };
    let Some(save) = save else {
        info!("Nouvelle partie (aucune sauvegarde dans {})", config.data_dir.display());
        let world = WorldState::new(config.world.clone());
        commands.insert_resource(SpawnPoint(find_spawn(&world)));
        commands.insert_resource(GameWorld(world));
        return;
    };
    if save.world != config.world {
        warn!(
            "La sauvegarde utilise la graine {} et une carte de {} chunks : ces réglages remplacent ceux de la configuration",
            save.world.seed, save.world.size_chunks
        );
        config.world = save.world.clone();
    }
    let mut world = WorldState::new(save.world);
    for (pos, block) in &save.edits {
        world.set_block(*pos, *block);
    }
    info!(
        "Sauvegarde chargée : jour {}, {} blocs modifiés, {} joueurs connus",
        save.time.day,
        save.edits.len(),
        save.players.len()
    );
    commands.insert_resource(Clock(save.time));
    commands.insert_resource(SavedPlayers(save.players.into_iter().collect()));
    commands.insert_resource(SpawnPoint(find_spawn(&world)));
    commands.insert_resource(GameWorld(world));
}

/// Cherche, en spirale autour du centre, une colonne avec un vrai sol (pas une grotte ouverte).
pub fn find_spawn(world: &WorldState) -> Vec3 {
    let center = world.config().size_blocks() / 2;
    for radius in 0..64i32 {
        for dx in -radius..=radius {
            for dz in -radius..=radius {
                if dx.abs() != radius && dz.abs() != radius {
                    continue;
                }
                let (x, z) = (center + dx, center + dz);
                let ground = world.generator().surface_height(x, z);
                let solid = |y: i32| world.is_solid(IVec3::new(x, y, z));
                if solid(ground - 1) && solid(ground - 2) && !solid(ground) && !solid(ground + 1) {
                    return Vec3::new(x as f32 + 0.5, ground as f32, z as f32 + 0.5);
                }
            }
        }
    }
    let ground = world.generator().surface_height(center, center);
    Vec3::new(center as f32 + 0.5, ground as f32 + 1.0, center as f32 + 0.5)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::config::WorldConfig;
    use shared::physics::{PLAYER_SIZE, overlaps_solid};

    #[test]
    fn spawn_is_standing_on_solid_ground() {
        for seed in [1, 42, 1234] {
            let world = WorldState::new(WorldConfig {
                seed,
                ..WorldConfig::default()
            });
            let spawn = find_spawn(&world);
            let below = (spawn - Vec3::Y * 0.5).floor().as_ivec3();
            assert!(world.is_solid(below), "graine {seed} : pas de sol sous {spawn}");
            assert!(!overlaps_solid(spawn, PLAYER_SIZE, &|p| world.is_solid(p)));
        }
    }
}
