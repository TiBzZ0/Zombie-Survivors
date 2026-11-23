//! Maillages des objets tenus en main : de vraies armes en volume (épée, hache, pioche, lance,
//! masse), et pour les autres objets leur icône extrudée pixel par pixel, comme une figurine.
//!
//! Repère commun à tous : l'icône de 1 × 1 centrée sur l'origine, x à droite, y en haut,
//! manche en bas à gauche, épaisseur selon z. Les couleurs sont portées par les sommets.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use shared::item::{Item, Tier};

use crate::icons::{N, draw};

/// Accumule des pavés colorés.
#[derive(Default)]
struct Builder {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

const FACES: [([f32; 3], [[f32; 3]; 4]); 6] = [
    ([1., 0., 0.], [[1., 0., 1.], [1., 0., 0.], [1., 1., 0.], [1., 1., 1.]]),
    ([-1., 0., 0.], [[0., 0., 0.], [0., 0., 1.], [0., 1., 1.], [0., 1., 0.]]),
    ([0., 1., 0.], [[0., 1., 1.], [1., 1., 1.], [1., 1., 0.], [0., 1., 0.]]),
    ([0., -1., 0.], [[0., 0., 0.], [1., 0., 0.], [1., 0., 1.], [0., 0., 1.]]),
    ([0., 0., 1.], [[0., 0., 1.], [1., 0., 1.], [1., 1., 1.], [0., 1., 1.]]),
    ([0., 0., -1.], [[1., 0., 0.], [0., 0., 0.], [0., 1., 0.], [1., 1., 0.]]),
];

fn linear(c: [f32; 3]) -> [f32; 4] {
    let l = Color::srgb(c[0], c[1], c[2]).to_linear();
    [l.red, l.green, l.blue, 1.0]
}

impl Builder {
    /// Un pavé de `min` à `max`, faces choisies par `show` (dans l'ordre de `FACES`).
    fn cuboid(&mut self, min: Vec3, max: Vec3, color: [f32; 3], show: [bool; 6]) {
        let color = linear(color);
        for ((normal, corners), shown) in FACES.into_iter().zip(show) {
            if !shown {
                continue;
            }
            // Un léger ombrage fixe aide à lire le volume.
            let shade = match normal {
                [0., 1., 0.] => 1.0,
                [0., -1., 0.] => 0.6,
                [_, 0., 0.] => 0.82,
                _ => 0.92,
            };
            let base = self.positions.len() as u32;
            for c in corners {
                self.positions.push((min + Vec3::from(c) * (max - min)).to_array());
                self.normals.push(normal);
                self.colors
                    .push([color[0] * shade, color[1] * shade, color[2] * shade, 1.0]);
            }
            self.indices
                .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }

    fn cube(&mut self, min: Vec3, max: Vec3, color: [f32; 3]) {
        self.cuboid(min, max, color, [true; 6]);
    }

    fn mesh(self, transform: Mat4) -> Mesh {
        let positions: Vec<[f32; 3]> = self
            .positions
            .iter()
            .map(|p| transform.transform_point3(Vec3::from(*p)).to_array())
            .collect();
        let normals: Vec<[f32; 3]> = self
            .normals
            .iter()
            .map(|n| transform.transform_vector3(Vec3::from(*n)).normalize().to_array())
            .collect();
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colors)
            .with_inserted_indices(Indices::U32(self.indices))
    }
}

fn tier_rgb(tier: Tier) -> [f32; 3] {
    match tier {
        Tier::Wood => [0.62, 0.43, 0.22],
        Tier::Stone => [0.58, 0.58, 0.6],
        Tier::Iron => [0.86, 0.88, 0.92],
        Tier::Steel => [0.55, 0.66, 0.82],
    }
}

const GRIP: [f32; 3] = [0.32, 0.2, 0.12];
const HAFT: [f32; 3] = [0.5, 0.34, 0.18];
const STEEL_DARK: [f32; 3] = [0.3, 0.3, 0.33];

/// Arme ou outil modelé le long de +Y (manche en bas), puis couché en diagonale comme l'icône.
fn tool(item: Item) -> Option<Builder> {
    let mut b = Builder::default();
    let v = Vec3::new;
    match item {
        Item::Sword(t) => {
            let blade = tier_rgb(t);
            let edge = blade.map(|c| (c * 1.15).min(1.0));
            b.cube(v(-0.05, -0.62, -0.05), v(0.05, -0.54, 0.05), STEEL_DARK); // pommeau
            b.cube(v(-0.03, -0.54, -0.03), v(0.03, -0.36, 0.03), GRIP); // poignée
            b.cube(v(-0.16, -0.36, -0.045), v(0.16, -0.3, 0.045), STEEL_DARK); // garde
            b.cube(v(-0.05, -0.3, -0.014), v(0.05, 0.42, 0.014), blade); // lame
            b.cube(v(-0.012, -0.28, -0.018), v(0.012, 0.36, 0.018), edge.map(|c| c * 0.75)); // gouttière
            b.cube(v(-0.033, 0.42, -0.012), v(0.033, 0.5, 0.012), edge); // pointe
            b.cube(v(-0.015, 0.5, -0.01), v(0.015, 0.55, 0.01), edge);
        }
        Item::Axe(t) => {
            b.cube(v(-0.03, -0.6, -0.03), v(0.03, 0.48, 0.03), HAFT);
            b.cube(v(0.03, 0.18, -0.025), v(0.14, 0.44, 0.025), tier_rgb(t)); // fer
            b.cube(v(0.14, 0.12, -0.018), v(0.24, 0.5, 0.018), tier_rgb(t)); // tranchant
            b.cube(v(-0.1, 0.27, -0.03), v(-0.03, 0.37, 0.03), STEEL_DARK); // talon
        }
        Item::Pickaxe(t) => {
            b.cube(v(-0.03, -0.6, -0.03), v(0.03, 0.42, 0.03), HAFT);
            b.cube(v(-0.3, 0.34, -0.035), v(0.3, 0.44, 0.035), tier_rgb(t));
            b.cube(v(0.24, 0.26, -0.03), v(0.36, 0.36, 0.03), tier_rgb(t));
            b.cube(v(-0.36, 0.26, -0.03), v(-0.24, 0.36, 0.03), tier_rgb(t));
            b.cube(v(-0.05, 0.32, -0.045), v(0.05, 0.46, 0.045), STEEL_DARK);
        }
        Item::Spear(t) => {
            b.cube(v(-0.022, -0.75, -0.022), v(0.022, 0.42, 0.022), HAFT);
            b.cube(v(-0.035, 0.38, -0.035), v(0.035, 0.44, 0.035), GRIP); // ligature
            b.cube(v(-0.055, 0.44, -0.012), v(0.055, 0.6, 0.012), tier_rgb(t));
            b.cube(v(-0.03, 0.6, -0.01), v(0.03, 0.68, 0.01), tier_rgb(t));
        }
        Item::Mace => {
            b.cube(v(-0.03, -0.6, -0.03), v(0.03, 0.3, 0.03), HAFT);
            b.cube(v(-0.035, -0.6, -0.035), v(0.035, -0.4, 0.035), GRIP);
            b.cube(v(-0.11, 0.3, -0.11), v(0.11, 0.52, 0.11), [0.45, 0.45, 0.48]);
            let spike = [0.7, 0.7, 0.72];
            b.cube(v(0.11, 0.37, -0.03), v(0.17, 0.45, 0.03), spike);
            b.cube(v(-0.17, 0.37, -0.03), v(-0.11, 0.45, 0.03), spike);
            b.cube(v(-0.03, 0.37, 0.11), v(0.03, 0.45, 0.17), spike);
            b.cube(v(-0.03, 0.37, -0.17), v(0.03, 0.45, -0.11), spike);
            b.cube(v(-0.03, 0.52, -0.03), v(0.03, 0.58, 0.03), spike);
        }
        _ => return None,
    }
    Some(b)
}

/// L'icône extrudée : un petit pavé par pixel plein, côtés seulement au bord du dessin.
fn extruded(item: Item) -> Builder {
    let canvas = draw(item);
    let pixels = canvas.pixels();
    // Le cerne sombre de l'icône (semi-transparent) n'a pas de sens en volume.
    let solid = |x: i32, y: i32| {
        (0..N as i32).contains(&x) && (0..N as i32).contains(&y) && pixels[y as usize][x as usize][3] == 255
    };
    let mut b = Builder::default();
    let px = 1.0 / N as f32;
    for y in 0..N as i32 {
        for x in 0..N as i32 {
            if !solid(x, y) {
                continue;
            }
            let [r, g, bl, _] = pixels[y as usize][x as usize];
            let color = [r as f32 / 255.0, g as f32 / 255.0, bl as f32 / 255.0];
            let min = Vec3::new(x as f32 * px - 0.5, 0.5 - (y + 1) as f32 * px, -px * 0.5);
            let max = min + Vec3::new(px, px, px);
            let show = [
                !solid(x + 1, y),
                !solid(x - 1, y),
                !solid(x, y - 1),
                !solid(x, y + 1),
                true,
                true,
            ];
            b.cuboid(min, max, color, show);
        }
    }
    b
}

/// Maillage d'un objet tenu (pas d'un bloc).
pub fn item_mesh(item: Item) -> Mesh {
    match tool(item) {
        // Couché à 45° : la pointe vers le haut à droite, comme sur l'icône.
        Some(b) => b.mesh(Mat4::from_rotation_z(-std::f32::consts::FRAC_PI_4) * Mat4::from_scale(Vec3::splat(0.95))),
        None => extruded(item).mesh(Mat4::IDENTITY),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weapons_are_modelled_and_other_items_extruded() {
        let sword = item_mesh(Item::Sword(Tier::Iron));
        assert!(sword.count_vertices() >= 6 * 4 * 6);
        let apple = extruded(Item::Apple);
        // Un pixel isolé aurait 6 faces : l'extrusion ne garde que les faces visibles.
        let pixels = apple.positions.len() / 4;
        assert!(pixels > 0 && pixels < 6 * 16 * 16);
        assert!(tool(Item::Apple).is_none());
    }
}
