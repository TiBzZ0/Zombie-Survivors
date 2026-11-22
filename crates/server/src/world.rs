//! État du monde côté serveur.
//!
//! Le serveur ne garde en mémoire que les chunks chargés. Un chunk jamais modifié
//! se régénère depuis la graine ; seuls les chunks modifiés seront sauvegardés
//! dans `data_dir` (prochain jalon).

use std::collections::HashMap;

use bevy::prelude::*;
use shared::chunk::{Chunk, ChunkPos};
use shared::worldgen::WorldGenerator;

use crate::ServerConfig;

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, setup_world)
            .add_systems(Startup, load_spawn_area);
    }
}

/// Chunks chargés et générateur du monde.
#[derive(Resource)]
pub struct VoxelWorld {
    pub generator: WorldGenerator,
    pub chunks: HashMap<ChunkPos, Chunk>,
}

impl VoxelWorld {
    /// Renvoie le chunk, en le générant s'il n'est pas encore chargé.
    pub fn chunk_or_generate(&mut self, pos: ChunkPos) -> &Chunk {
        let generator = &self.generator;
        self.chunks
            .entry(pos)
            .or_insert_with(|| generator.generate_chunk(pos))
    }
}

fn setup_world(mut commands: Commands, config: Res<ServerConfig>) {
    commands.insert_resource(VoxelWorld {
        generator: WorldGenerator::new(config.world.clone()),
        chunks: HashMap::new(),
    });
}

/// Charge la zone d'apparition, au centre de la carte.
fn load_spawn_area(mut world: ResMut<VoxelWorld>) {
    let config = world.generator.config().clone();
    let center = config.size_chunks / 2;
    let start = std::time::Instant::now();
    for x in center - 2..=center + 2 {
        for z in center - 2..=center + 2 {
            for y in 0..config.height_chunks {
                world.chunk_or_generate(ChunkPos::new(x, y, z));
            }
        }
    }
    info!(
        "Zone d'apparition chargée : {} chunks en {} ms",
        world.chunks.len(),
        start.elapsed().as_millis()
    );
}
