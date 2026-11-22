//! Textures simples, générées au démarrage : un motif de 16 × 16 pixels par bloc
//! (grain, planches, écorce, pierres, éclats de minerai), multiplié par la couleur du bloc.

use bevy::asset::RenderAssetUsages;
use bevy::image::{Image, ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use shared::block::Block;

/// Côté d'une tuile, en pixels.
pub const TILE: u32 = 16;
/// Nombre maximal de types de blocs (une texture par valeur possible de `Block`, avec de la marge).
#[cfg(test)]
pub const TILES: u32 = 64;

fn hash(x: u32, y: u32, salt: u32) -> f32 {
    let mut h = x.wrapping_mul(0x85EB_CA6B) ^ y.wrapping_mul(0xC2B2_AE35) ^ salt.wrapping_mul(0x9E37_79B9);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h & 0xFFFF) as f32 / 65535.0
}

/// Couleur (multiplicateur) d'un pixel de la tuile d'un bloc.
fn pixel(block: Block, x: u32, y: u32) -> [f32; 3] {
    let salt = block as u32 + 1;
    let n = hash(x, y, salt);
    let coarse = hash(x / 3, y / 3, salt + 100);
    let gray = |v: f32| [v, v, v];
    match block {
        Block::Grass | Block::Leaves => {
            if block == Block::Leaves && n < 0.12 {
                gray(0.55)
            } else {
                gray(0.84 + 0.16 * n)
            }
        }
        Block::Dirt => gray(if n > 0.9 { 0.7 } else { 0.85 + 0.15 * coarse }),
        Block::Sand => gray(0.9 + 0.1 * n),
        Block::Stone | Block::ReinforcedWall | Block::Forge => {
            // Pierres irrégulières : taches et joints sombres.
            let joint = (x + (y / 4) * 5).is_multiple_of(8) || y.is_multiple_of(4);
            let mut v = 0.82 + 0.1 * coarse + 0.08 * n;
            if joint && block != Block::Stone {
                v = 0.62;
            }
            if block == Block::Forge && (5..11).contains(&x) && (3..8).contains(&y) {
                return [0.9, 0.45, 0.2]; // le foyer
            }
            gray(v)
        }
        Block::IronOre | Block::CoalOre => {
            if n > 0.8 {
                if block == Block::IronOre {
                    [1.0, 0.72, 0.5]
                } else {
                    gray(0.45)
                }
            } else {
                gray(0.8 + 0.12 * coarse)
            }
        }
        Block::Wood => gray(0.78 + 0.18 * hash(x, 0, salt) + 0.04 * n),
        Block::Planks | Block::Door | Block::DoorOpen | Block::Chest | Block::Workbench => {
            // Planches horizontales, avec le fil du bois.
            let mut v = 0.86 + 0.1 * hash(x / 4, y, salt) + 0.04 * n;
            if y % 4 == 3 {
                v = 0.64;
            }
            if block == Block::Chest && (7..9).contains(&x) {
                v = if (6..9).contains(&y) { 1.0 } else { 0.55 }; // ferrure et serrure
            }
            if matches!(block, Block::Door | Block::DoorOpen) && (x == 1 || x == 14) {
                v = 0.6;
            }
            if block == Block::Workbench && y < 2 {
                v = 0.7;
            }
            gray(v)
        }
        Block::Spikes => gray(if (x + y).is_multiple_of(5) || (x + 16 - y).is_multiple_of(5) {
            1.0
        } else {
            0.7
        }),
        Block::Generator => {
            // Carter jaune avec des ailettes sombres.
            if (3..13).contains(&x) && y % 3 == 1 && (4..13).contains(&y) {
                return [0.25, 0.22, 0.18];
            }
            gray(if x == 0 || y == 0 || x == 15 || y == 15 {
                0.6
            } else {
                0.85 + 0.1 * n
            })
        }
        Block::Floodlight => {
            // Grande lentille claire dans un cadre sombre.
            let (dx, dy) = (x as f32 - 7.5, y as f32 - 7.5);
            if dx * dx + dy * dy < 30.0 {
                gray(1.0)
            } else {
                gray(0.35 + 0.1 * n)
            }
        }
        Block::ElectricDoor | Block::ElectricDoorOpen => {
            // Plaques d'acier avec une bande jaune et noire.
            if (7..9).contains(&y) {
                return if (x / 2).is_multiple_of(2) {
                    [1.6, 1.4, 0.2]
                } else {
                    [0.2, 0.2, 0.2]
                };
            }
            gray(if x == 0 || x == 15 { 0.6 } else { 0.8 + 0.12 * coarse })
        }
        Block::SupplyCrate => {
            // Caisse orange cerclée de sangles sombres.
            if (7..9).contains(&x) || (7..9).contains(&y) {
                return [0.3, 0.25, 0.2];
            }
            gray(if x == 0 || y == 0 || x == 15 || y == 15 {
                0.6
            } else {
                0.85 + 0.1 * n
            })
        }
        Block::Banner => {
            // Étoffe rouge frappée d'une croix blanche, sur un mât de bois.
            if x < 2 {
                return [0.45, 0.3, 0.15];
            }
            if (7..9).contains(&x) || (7..9).contains(&y) {
                return [1.5, 1.5, 1.4];
            }
            gray(0.85 + 0.1 * n)
        }
        Block::TeslaCoil => {
            // Bobinage cuivré.
            gray(if y.is_multiple_of(2) { 1.0 } else { 0.65 + 0.1 * n })
        }
        Block::Turret => {
            // Caisson riveté en fer, avec une meurtrière sombre.
            if (4..12).contains(&x) && (6..9).contains(&y) {
                return [0.15, 0.13, 0.12];
            }
            let rivet = (x == 2 || x == 13) && (y == 2 || y == 13);
            let edge = x == 0 || y == 0 || x == 15 || y == 15;
            gray(if rivet {
                1.0
            } else if edge {
                0.55
            } else {
                0.72 + 0.08 * n
            })
        }
        Block::MachineGun | Block::Cannon => {
            // Plaques de métal sombre, canon au centre.
            let barrel = (6..10).contains(&x) && (5..11).contains(&y);
            let seam = y.is_multiple_of(8) || x.is_multiple_of(8);
            if barrel {
                gray(if block == Block::Cannon { 0.25 } else { 0.4 })
            } else {
                gray(if seam { 0.55 } else { 0.75 + 0.1 * n })
            }
        }
        Block::Catapult => {
            // Charpente en bois croisée sur un fond de planches.
            let beam = (x + y) % 16 < 2 || (x + 16 - y) % 16 < 2;
            let mut v = 0.85 + 0.1 * hash(x / 4, y, salt);
            if beam {
                v = 0.6;
            }
            gray(v)
        }
        Block::Sprout | Block::YoungWheat | Block::RipeWheat => {
            // Tiges verticales sur fond de terre.
            let stalk = x % 3 == 1 && hash(x, 0, salt) > 0.2;
            let ear = block == Block::RipeWheat && y < 5 && stalk;
            if ear {
                gray(1.0)
            } else if stalk {
                gray(0.8 + 0.15 * n)
            } else {
                [0.35, 0.25, 0.15]
            }
        }
        Block::Campfire => {
            // Bûches croisées et braises.
            let log = (x + y) % 8 < 2 || (x + 16 - y) % 8 < 2;
            if log {
                [0.45, 0.3, 0.2]
            } else if n > 0.6 {
                [1.0, 0.85, 0.4]
            } else {
                gray(0.75 + 0.2 * n)
            }
        }
        Block::BarbedWire => {
            // Fils tendus avec des pointes.
            let wire = y % 5 == 2 || (x + y).is_multiple_of(7);
            let barb = wire && x % 4 == 1;
            if barb {
                gray(1.0)
            } else if wire {
                gray(0.75)
            } else {
                [0.35, 0.3, 0.25]
            }
        }
        Block::Water | Block::FlowingWater => {
            // Vaguelettes claires.
            let ripple = (x + 2 * (y / 3) + (hash(0, y / 3, salt) * 4.0) as u32).is_multiple_of(7);
            gray(if ripple { 1.0 } else { 0.85 + 0.08 * coarse })
        }
        Block::Snow => gray(0.94 + 0.06 * n),
        Block::Cactus => {
            // Côtes verticales et épines.
            let rib = x.is_multiple_of(4);
            if n > 0.93 {
                gray(1.0)
            } else if rib {
                gray(0.7)
            } else {
                gray(0.88 + 0.1 * coarse)
            }
        }
        Block::Air => gray(1.0),
    }
}

/// Texture d'un bloc, répétée sur les grandes faces fusionnées et échantillonnée sans lissage.
pub fn build_block_texture(block: Block) -> Image {
    let mut data = vec![255u8; (TILE * TILE * 4) as usize];
    for y in 0..TILE {
        for x in 0..TILE {
            let c = pixel(block, x, y);
            let i = ((y * TILE + x) * 4) as usize;
            for k in 0..3 {
                data[i + k] = (c[k].clamp(0.0, 1.0) * 255.0) as u8;
            }
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: TILE,
            height: TILE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        ..ImageSamplerDescriptor::nearest()
    });
    image
}

/// Tous les blocs visibles.
pub fn all_blocks() -> impl Iterator<Item = Block> {
    ALL_BLOCKS.into_iter().filter(|b| *b != Block::Air)
}

#[cfg(test)]
fn block_from_index(index: u32) -> Option<Block> {
    ALL_BLOCKS.iter().copied().find(|b| *b as u32 == index)
}

const ALL_BLOCKS: [Block; 37] = [
    Block::Air,
    Block::Grass,
    Block::Dirt,
    Block::Stone,
    Block::Sand,
    Block::Wood,
    Block::Leaves,
    Block::IronOre,
    Block::Planks,
    Block::ReinforcedWall,
    Block::Spikes,
    Block::CoalOre,
    Block::Door,
    Block::DoorOpen,
    Block::Chest,
    Block::Turret,
    Block::BarbedWire,
    Block::Workbench,
    Block::Forge,
    Block::MachineGun,
    Block::Cannon,
    Block::Catapult,
    Block::Sprout,
    Block::YoungWheat,
    Block::RipeWheat,
    Block::Campfire,
    Block::Water,
    Block::Snow,
    Block::Cactus,
    Block::FlowingWater,
    Block::Generator,
    Block::Floodlight,
    Block::ElectricDoor,
    Block::ElectricDoorOpen,
    Block::TeslaCoil,
    Block::SupplyCrate,
    Block::Banner,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_block_has_a_texture() {
        for block in all_blocks() {
            assert!((block as u32) < TILES);
            assert_eq!(block_from_index(block as u32), Some(block));
            assert_eq!(build_block_texture(block).width(), TILE);
        }
    }
}
