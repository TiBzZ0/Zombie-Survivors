//! Génération déterministe du terrain à partir d'une graine.
//!
//! Le serveur et le client appellent la même fonction : un chunk que personne n'a
//! modifié n'a pas besoin de passer sur le réseau, le client le régénère seul.

use fastnoise_lite::{FastNoiseLite, FractalType, NoiseType};

use crate::block::Block;
use crate::chunk::{CHUNK_SIZE, Chunk, ChunkPos};
use crate::config::WorldConfig;

/// Taille d'une cellule de la grille des arbres, en blocs.
const TREE_CELL: i32 = 9;
/// Pourcentage de cellules qui ont un arbre.
const TREE_CHANCE: u32 = 45;

/// Petit hachage entier, stable d'une plateforme à l'autre.
fn hash3(seed: i32, a: i32, b: i32) -> u32 {
    let mut h = (seed as u32).wrapping_mul(0x9E37_79B9)
        ^ (a as u32).wrapping_mul(0x85EB_CA6B)
        ^ (b as u32).wrapping_mul(0xC2B2_AE35);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    h
}

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
            return self.tree_block_at(x, y, z);
        }
        if y > 2 && y < surface - 4 && self.caves.get_noise_3d(x as f32, y as f32, z as f32) > 0.6 {
            return Block::Air;
        }
        let depth = surface - 1 - y;
        let beach = self.is_beach(surface);
        match depth {
            0 if beach => Block::Sand,
            0 => Block::Grass,
            1..=3 if beach => Block::Sand,
            1..=3 => Block::Dirt,
            _ if self.ore.get_noise_3d(x as f32, y as f32, z as f32) > 0.75 => Block::IronOre,
            _ => Block::Stone,
        }
    }

    /// Les colonnes trop basses sont des plages de sable, sans arbres.
    fn is_beach(&self, surface: i32) -> bool {
        surface <= (self.config.height_blocks() as f32 * 0.45) as i32 - 10
    }

    /// Arbre éventuel d'une cellule de la grille des arbres : pied du tronc et hauteur.
    ///
    /// La carte est découpée en cellules de `TREE_CELL` blocs ; chaque cellule a au plus
    /// un arbre, placé au hasard (de façon déterministe) loin des bords de la cellule.
    pub fn tree_in_cell(&self, cx: i32, cz: i32) -> Option<(i32, i32, i32, i32)> {
        let h = hash3(self.config.seed, cx, cz);
        if h % 100 >= TREE_CHANCE {
            return None;
        }
        let x = cx * TREE_CELL + 2 + ((h >> 8) % (TREE_CELL as u32 - 4)) as i32;
        let z = cz * TREE_CELL + 2 + ((h >> 16) % (TREE_CELL as u32 - 4)) as i32;
        let size = self.config.size_blocks();
        if x < 3 || z < 3 || x >= size - 3 || z >= size - 3 {
            return None;
        }
        let ground = self.surface_height(x, z);
        if self.is_beach(ground) || ground + 8 >= self.config.height_blocks() {
            return None;
        }
        let height = 4 + ((h >> 24) % 2) as i32;
        Some((x, ground, z, height))
    }

    /// Bloc d'arbre (tronc ou feuilles) au-dessus du sol, ou de l'air.
    fn tree_block_at(&self, x: i32, y: i32, z: i32) -> Block {
        let (cx, cz) = (x.div_euclid(TREE_CELL), z.div_euclid(TREE_CELL));
        for dx in -1..=1 {
            for dz in -1..=1 {
                let Some((tx, ground, tz, height)) = self.tree_in_cell(cx + dx, cz + dz) else {
                    continue;
                };
                let (ox, oy, oz) = (x - tx, y - ground, z - tz);
                if ox == 0 && oz == 0 && (0..height).contains(&oy) {
                    return Block::Wood;
                }
                let top = height - 1;
                let radius = match oy - top {
                    -2..=0 => 2,
                    1 => 1,
                    _ => continue,
                };
                if ox.abs() <= radius && oz.abs() <= radius && !(ox.abs() == 2 && oz.abs() == 2) {
                    return Block::Leaves;
                }
            }
        }
        Block::Air
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
    fn trees_grow_on_the_map() {
        let generator = WorldGenerator::new(WorldConfig::default());
        let trees: Vec<_> = (0..20)
            .flat_map(|cx| (0..20).map(move |cz| (cx, cz)))
            .filter_map(|(cx, cz)| generator.tree_in_cell(cx, cz))
            .collect();
        assert!(trees.len() > 50, "seulement {} arbres", trees.len());
        let (x, ground, z, height) = trees[0];
        assert_eq!(generator.block_at(x, ground, z), Block::Wood);
        assert_eq!(generator.block_at(x, ground + height - 1, z), Block::Wood);
        assert_eq!(generator.block_at(x + 1, ground + height - 1, z), Block::Leaves);
        assert_eq!(generator.block_at(x, ground + height, z), Block::Leaves);
        assert_eq!(generator.block_at(x, ground + height + 1, z), Block::Air);
    }

    #[test]
    fn map_has_ground_and_sky() {
        let generator = WorldGenerator::new(WorldConfig::default());
        assert!(generator.block_at(100, 1, 100).is_solid());
        let top = generator.config().height_blocks() - 1;
        assert_eq!(generator.block_at(100, top, 100), Block::Air);
    }
}
