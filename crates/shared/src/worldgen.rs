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

/// Taille d'une cellule de la grille des rochers de filon, en blocs.
const BOULDER_CELL: i32 = 24;
/// Pourcentage de cellules qui ont un rocher de filon.
const BOULDER_CHANCE: u32 = 40;

/// Taille d'une cellule de la grille des ruines, en blocs.
pub const RUIN_CELL: i32 = 72;
/// Pourcentage de cellules qui ont une ruine (si le terrain est assez plat).
const RUIN_CHANCE: u32 = 45;
/// Demi-largeur d'une ruine : les murs sont à cette distance du centre.
pub const RUIN_HALF: i32 = 3;

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
    coal: FastNoiseLite,
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
        ore.set_frequency(Some(0.07));

        let mut coal = FastNoiseLite::with_seed(config.seed.wrapping_add(3));
        coal.set_noise_type(Some(NoiseType::OpenSimplex2));
        coal.set_frequency(Some(0.08));

        Self {
            config,
            height,
            caves,
            ore,
            coal,
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
            if let Some(block) = self.ruin_block_at(x, y, z) {
                return block;
            }
            let deposit = self.boulder_block_at(x, y, z);
            if deposit != Block::Air {
                return deposit;
            }
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
            // Filons souterrains : de grosses poches plutôt que des blocs isolés.
            _ if self.ore.get_noise_3d(x as f32, y as f32, z as f32) > 0.8 => Block::IronOre,
            4..=24 if self.coal.get_noise_3d(x as f32, y as f32, z as f32) > 0.78 => Block::CoalOre,
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

    /// Rocher de filon éventuel d'une cellule : centre au sol et type de filon.
    pub fn boulder_in_cell(&self, cx: i32, cz: i32) -> Option<(i32, i32, i32, Block)> {
        let h = hash3(self.config.seed.wrapping_add(77), cx, cz);
        if h % 100 >= BOULDER_CHANCE {
            return None;
        }
        let x = cx * BOULDER_CELL + 3 + ((h >> 8) % (BOULDER_CELL as u32 - 6)) as i32;
        let z = cz * BOULDER_CELL + 3 + ((h >> 16) % (BOULDER_CELL as u32 - 6)) as i32;
        let size = self.config.size_blocks();
        if x < 3 || z < 3 || x >= size - 3 || z >= size - 3 {
            return None;
        }
        let ground = self.surface_height(x, z);
        if ground + 4 >= self.config.height_blocks() {
            return None;
        }
        let kind = if (h >> 24) % 5 < 2 {
            Block::IronOre
        } else {
            Block::CoalOre
        };
        Some((x, ground, z, kind))
    }

    /// Bloc de rocher de filon posé sur le sol (une croix de 5 blocs surmontée d'un bloc), ou de l'air.
    fn boulder_block_at(&self, x: i32, y: i32, z: i32) -> Block {
        let (cx, cz) = (x.div_euclid(BOULDER_CELL), z.div_euclid(BOULDER_CELL));
        let Some((bx, ground, bz, kind)) = self.boulder_in_cell(cx, cz) else {
            return Block::Air;
        };
        let (dx, dy, dz) = (x - bx, y - ground, z - bz);
        let inside = match dy {
            0 => dx.abs() + dz.abs() <= 1,
            1 => dx == 0 && dz == 0,
            _ => false,
        };
        if inside { kind } else { Block::Air }
    }

    /// Ruine éventuelle d'une cellule : centre au sol, et côté de la porte (0 à 3).
    ///
    /// Une maison en ruine de 7 × 7 blocs, aux murs troués, avec un coffre dans un coin.
    pub fn ruin_in_cell(&self, cx: i32, cz: i32) -> Option<(i32, i32, i32, u32)> {
        let (x, z, h) = self.ruin_spot(cx, cz)?;
        let ground = self.surface_height(x, z);
        if self.is_beach(ground) || ground + 6 >= self.config.height_blocks() {
            return None;
        }
        // Seulement sur un terrain presque plat.
        let r = RUIN_HALF;
        let flat = [(-r, -r), (r, -r), (-r, r), (r, r)]
            .iter()
            .all(|(dx, dz)| (self.surface_height(x + dx, z + dz) - ground).abs() <= 1);
        flat.then_some((x, ground, z, (h >> 24) % 4))
    }

    /// Position (sans le relief) de la ruine éventuelle d'une cellule : peu coûteux.
    fn ruin_spot(&self, cx: i32, cz: i32) -> Option<(i32, i32, u32)> {
        let h = hash3(self.config.seed.wrapping_add(4242), cx, cz);
        if h % 100 >= RUIN_CHANCE {
            return None;
        }
        let margin = RUIN_HALF + 2;
        let span = (RUIN_CELL - 2 * margin) as u32;
        let x = cx * RUIN_CELL + margin + ((h >> 8) % span) as i32;
        let z = cz * RUIN_CELL + margin + ((h >> 16) % span) as i32;
        let size = self.config.size_blocks();
        (x >= margin && z >= margin && x < size - margin && z < size - margin).then_some((x, z, h))
    }

    /// Position du coffre d'une ruine.
    pub fn ruin_chest(&self, ruin: (i32, i32, i32, u32)) -> (i32, i32, i32) {
        let (x, ground, z, _) = ruin;
        let (cx, cz) = (x + RUIN_HALF - 1, z + RUIN_HALF - 1);
        // Posé sur le sol, même si le terrain monte d'un bloc dans ce coin.
        (cx, ground.max(self.surface_height(cx, cz)), cz)
    }

    /// Bloc d'une ruine (murs, coffre, ou air à l'intérieur), ou `None` hors des ruines.
    fn ruin_block_at(&self, x: i32, y: i32, z: i32) -> Option<Block> {
        let (cx, cz) = (x.div_euclid(RUIN_CELL), z.div_euclid(RUIN_CELL));
        let (rx, rz, _) = self.ruin_spot(cx, cz)?;
        let (dx, dz) = (x - rx, z - rz);
        if dx.abs() > RUIN_HALF || dz.abs() > RUIN_HALF {
            return None;
        }
        let ruin = self.ruin_in_cell(cx, cz)?;
        let (_, ground, _, door) = ruin;
        let dy = y - ground;
        if !(0..=4).contains(&dy) {
            return None;
        }
        let (chest_x, chest_y, chest_z) = self.ruin_chest(ruin);
        if (x, y, z) == (chest_x, chest_y, chest_z) {
            return Some(Block::Chest);
        }
        let wall = dx.abs() == RUIN_HALF || dz.abs() == RUIN_HALF;
        if !wall || dy > 2 {
            return Some(Block::Air);
        }
        let doorway = match door {
            0 => dz == -RUIN_HALF && dx == 0,
            1 => dz == RUIN_HALF && dx == 0,
            2 => dx == -RUIN_HALF && dz == 0,
            _ => dx == RUIN_HALF && dz == 0,
        };
        if doorway && dy <= 1 {
            return Some(Block::Air);
        }
        // Murs écroulés : de plus en plus de trous en montant.
        let decay = hash3(self.config.seed.wrapping_add(5), x * 31 + y, z) % 100;
        if decay < 20 + dy as u32 * 20 {
            return Some(Block::Air);
        }
        Some(if dy == 0 { Block::Stone } else { Block::Planks })
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
    fn deposits_sit_on_the_surface() {
        let generator = WorldGenerator::new(WorldConfig::default());
        let boulders: Vec<_> = (0..20)
            .flat_map(|cx| (0..20).map(move |cz| (cx, cz)))
            .filter_map(|(cx, cz)| generator.boulder_in_cell(cx, cz))
            .collect();
        assert!(boulders.len() > 60, "seulement {} rochers", boulders.len());
        assert!(boulders.iter().any(|b| b.3 == Block::IronOre));
        assert!(boulders.iter().any(|b| b.3 == Block::CoalOre));
        let (x, ground, z, kind) = boulders[0];
        assert!(kind.is_deposit());
        assert_eq!(generator.block_at(x, ground, z), kind);
        assert_eq!(generator.block_at(x + 1, ground, z), kind);
        assert_eq!(generator.block_at(x, ground + 1, z), kind);
        assert_eq!(generator.block_at(x, ground + 2, z), Block::Air);
    }

    #[test]
    fn ruins_have_a_chest() {
        let generator = WorldGenerator::new(WorldConfig::default());
        let ruins: Vec<_> = (0..8)
            .flat_map(|cx| (0..8).map(move |cz| (cx, cz)))
            .filter_map(|(cx, cz)| generator.ruin_in_cell(cx, cz))
            .collect();
        assert!(ruins.len() >= 5, "seulement {} ruines", ruins.len());
        for ruin in ruins {
            let (x, y, z) = generator.ruin_chest(ruin);
            assert_eq!(generator.block_at(x, y, z), Block::Chest);
            // L'intérieur est dégagé.
            assert_eq!(generator.block_at(ruin.0, ruin.1 + 1, ruin.2), Block::Air);
        }
    }

    #[test]
    fn map_has_ground_and_sky() {
        let generator = WorldGenerator::new(WorldConfig::default());
        assert!(generator.block_at(100, 1, 100).is_solid());
        let top = generator.config().height_blocks() - 1;
        assert_eq!(generator.block_at(100, top, 100), Block::Air);
    }
}
