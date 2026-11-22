//! Génération déterministe du terrain à partir d'une graine.
//!
//! Le serveur et le client appellent la même fonction : un chunk que personne n'a
//! modifié n'a pas besoin de passer sur le réseau, le client le régénère seul.

use fastnoise_lite::{FastNoiseLite, FractalType, NoiseType};

use crate::block::Block;
use crate::chunk::{CHUNK_SIZE, Chunk, ChunkPos};
use crate::config::WorldConfig;

/// Générateur de terrain. Peu coûteux à créer, mais autant le garder.
pub struct WorldGenerator {
    config: WorldConfig,
    height: FastNoiseLite,
    caves: FastNoiseLite,
    ore: FastNoiseLite,
}

impl WorldGenerator {
    pub fn new(config: WorldConfig) -> Self {
        let mut height = FastNoiseLite::with_seed(config.seed);
        height.set_noise_type(Some(NoiseType::OpenSimplex2));
        height.set_fractal_type(Some(FractalType::FBm));
        height.set_fractal_octaves(Some(4));
        height.set_frequency(Some(0.006));

        let mut caves = FastNoiseLite::with_seed(config.seed.wrapping_add(1));
        caves.set_noise_type(Some(NoiseType::OpenSimplex2));
        caves.set_frequency(Some(0.04));

        let mut ore = FastNoiseLite::with_seed(config.seed.wrapping_add(2));
        ore.set_noise_type(Some(NoiseType::OpenSimplex2));
        ore.set_frequency(Some(0.12));

        Self {
            config,
            height,
            caves,
            ore,
        }
    }

    pub fn config(&self) -> &WorldConfig {
        &self.config
    }

    /// Hauteur du sol (premier bloc d'air) à une colonne donnée.
    pub fn surface_height(&self, x: i32, z: i32) -> i32 {
        let base = self.config.height_blocks() as f32 * 0.45;
        let n = self.height.get_noise_2d(x as f32, z as f32); // entre -1 et 1
        (base + n * 24.0) as i32
    }

    /// Bloc à une position monde. Hors de la carte, tout est de l'air.
    pub fn block_at(&self, x: i32, y: i32, z: i32) -> Block {
        let size = self.config.size_blocks();
        if x < 0 || z < 0 || x >= size || z >= size || y < 0 || y >= self.config.height_blocks() {
            return Block::Air;
        }
        // Pas de grotte sous y = 2 (le fond de la carte), ni juste sous la surface (le sol reste propre).
        let surface = self.surface_height(x, z);
        if y >= surface {
            return Block::Air;
        }
        if y > 2 && y < surface - 4 && self.caves.get_noise_3d(x as f32, y as f32, z as f32) > 0.6 {
            return Block::Air;
        }
        let depth = surface - 1 - y;
        let beach = surface <= (self.config.height_blocks() as f32 * 0.45) as i32 - 10;
        match depth {
            0 if beach => Block::Sand,
            0 => Block::Grass,
            1..=3 if beach => Block::Sand,
            1..=3 => Block::Dirt,
            _ if self.ore.get_noise_3d(x as f32, y as f32, z as f32) > 0.75 => Block::IronOre,
            _ => Block::Stone,
        }
    }

    /// Génère tout un chunk.
    pub fn generate_chunk(&self, pos: ChunkPos) -> Chunk {
        let mut chunk = Chunk::default();
        if !self.config.contains_chunk(pos) {
            return chunk;
        }
        let [ox, oy, oz] = pos.origin();
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                for y in 0..CHUNK_SIZE {
                    let block = self.block_at(ox + x as i32, oy + y as i32, oz + z as i32);
                    if block.is_solid() {
                        chunk.set(x, y, z, block);
                    }
                }
            }
        }
        chunk
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_gives_same_chunk() {
        let a = WorldGenerator::new(WorldConfig::default());
        let b = WorldGenerator::new(WorldConfig::default());
        let pos = ChunkPos::new(3, 1, 7);
        assert!(a.generate_chunk(pos) == b.generate_chunk(pos));
    }

    #[test]
    fn different_seed_gives_different_terrain() {
        let a = WorldGenerator::new(WorldConfig::default());
        let b = WorldGenerator::new(WorldConfig {
            seed: 7,
            ..WorldConfig::default()
        });
        let differs = (0..64).any(|i| a.surface_height(i * 13, i * 7) != b.surface_height(i * 13, i * 7));
        assert!(differs);
    }

    #[test]
    fn outside_the_map_is_air() {
        let generator = WorldGenerator::new(WorldConfig::default());
        let size = generator.config().size_blocks();
        assert_eq!(generator.block_at(-1, 10, 10), Block::Air);
        assert_eq!(generator.block_at(size, 10, 10), Block::Air);
        assert!(generator.generate_chunk(ChunkPos::new(-1, 0, 0)).is_empty());
    }

    #[test]
    fn map_has_ground_and_sky() {
        let generator = WorldGenerator::new(WorldConfig::default());
        assert!(generator.block_at(100, 1, 100).is_solid());
        let top = generator.config().height_blocks() - 1;
        assert_eq!(generator.block_at(100, top, 100), Block::Air);
    }
}
