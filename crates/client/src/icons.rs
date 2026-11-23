//! Icônes des objets, dessinées au pixel près au démarrage (16 × 16, affichées sans lissage).
//! Un bloc devient un petit cube en perspective avec sa texture ; les autres objets ont un
//! dessin simple (outils, armes, nourriture...), cerné de sombre pour se lire sur tout fond.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::image::{Image, ImageSampler};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use shared::block::Block;
use shared::item::{Item, Tier};

use crate::textures::{TILE, tile_color};

const N: usize = 16;

/// Icônes déjà dessinées.
#[derive(Resource, Default)]
pub struct ItemIcons(HashMap<Item, Handle<Image>>);

impl ItemIcons {
    /// Icône d'un objet (dessinée à la première demande).
    pub fn get(&mut self, images: &mut Assets<Image>, item: Item) -> Handle<Image> {
        self.0
            .entry(item)
            .or_insert_with(|| images.add(to_image(&draw(item))))
            .clone()
    }
}

type Rgba = [u8; 4];

fn rgb(r: f32, g: f32, b: f32) -> Rgba {
    [(r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8, 255]
}

/// Une image de 16 × 16 pixels, transparente au départ.
#[derive(Clone)]
pub struct Canvas([[Rgba; N]; N]);

impl Canvas {
    fn new() -> Self {
        Self([[[0; 4]; N]; N])
    }

    #[cfg(test)]
    pub fn get(&self, x: usize, y: usize) -> Rgba {
        self.0[y][x]
    }

    fn set(&mut self, x: i32, y: i32, c: Rgba) {
        if (0..N as i32).contains(&x) && (0..N as i32).contains(&y) {
            self.0[y as usize][x as usize] = c;
        }
    }

    fn line(&mut self, (x0, y0): (i32, i32), (x1, y1): (i32, i32), c: Rgba) {
        let steps = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let x = x0 as f32 + (x1 - x0) as f32 * t;
            let y = y0 as f32 + (y1 - y0) as f32 * t;
            self.set(x.round() as i32, y.round() as i32, c);
        }
    }

    /// Trait épais de deux pixels.
    fn thick(&mut self, a: (i32, i32), b: (i32, i32), c: Rgba) {
        self.line(a, b, c);
        self.line((a.0 + 1, a.1), (b.0 + 1, b.1), c);
    }

    fn disc(&mut self, cx: f32, cy: f32, r: f32, c: Rgba) {
        for y in 0..N as i32 {
            for x in 0..N as i32 {
                let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                if dx * dx + dy * dy <= r * r {
                    self.set(x, y, c);
                }
            }
        }
    }

    fn rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: Rgba) {
        for y in y0..=y1 {
            for x in x0..=x1 {
                self.set(x, y, c);
            }
        }
    }

    /// Cerne de sombre tout ce qui est dessiné.
    fn outline(&mut self) {
        let src = self.0;
        for y in 0..N as i32 {
            for x in 0..N as i32 {
                if src[y as usize][x as usize][3] != 0 {
                    continue;
                }
                let near = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dy)| {
                    let (nx, ny) = (x + dx, y + dy);
                    (0..N as i32).contains(&nx) && (0..N as i32).contains(&ny) && src[ny as usize][nx as usize][3] != 0
                });
                if near {
                    self.0[y as usize][x as usize] = [20, 16, 12, 200];
                }
            }
        }
    }
}

fn tier_color(tier: Tier) -> Rgba {
    match tier {
        Tier::Wood => rgb(0.62, 0.43, 0.22),
        Tier::Stone => rgb(0.58, 0.58, 0.6),
        Tier::Iron => rgb(0.86, 0.88, 0.92),
    }
}

const HANDLE: [f32; 3] = [0.45, 0.3, 0.15];

fn handle() -> Rgba {
    rgb(HANDLE[0], HANDLE[1], HANDLE[2])
}

/// Petit cube en perspective, texturé comme le bloc.
/// La tuile d'un bloc telle quelle, avec ses trous (pousses).
fn flat(block: Block) -> Canvas {
    let mut c = Canvas::new();
    for y in 0..N {
        for x in 0..N {
            if crate::textures::opaque(block, x as u32, y as u32) {
                let [r, g, b] = tile_color(block, x as u32, y as u32);
                c.set(x as i32, y as i32, rgb(r, g, b));
            }
        }
    }
    c
}

fn cube(block: Block) -> Canvas {
    let mut c = Canvas::new();
    for y in 0..N {
        for x in 0..N {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let (face, shade) = if fx < 8.0 {
                let top = (4.0 - fx / 2.0, 4.0 + fx / 2.0);
                if fy >= top.0 && fy < top.1 {
                    (0, 1.0)
                } else if fy >= top.1 && fy < 12.0 + fx / 2.0 {
                    (1, 0.78)
                } else {
                    continue;
                }
            } else {
                let d = fx - 8.0;
                let top = (d / 2.0, 8.0 - d / 2.0);
                if fy >= top.0 && fy < top.1 {
                    (0, 1.0)
                } else if fy >= top.1 && fy < 16.0 - d / 2.0 {
                    (2, 0.6)
                } else {
                    continue;
                }
            };
            let (u, v) = match face {
                0 => ((x * 2 + y) % TILE as usize, (y * 3) % TILE as usize),
                _ => ((x * 2) % TILE as usize, (y + x / 2) % TILE as usize),
            };
            let col = tile_color(block, u as u32, v as u32);
            c.set(x as i32, y as i32, rgb(col[0] * shade, col[1] * shade, col[2] * shade));
        }
    }
    c
}

/// Dessin d'un objet.
pub fn draw(item: Item) -> Canvas {
    let mut c = Canvas::new();
    let red = rgb(0.8, 0.15, 0.12);
    let white = rgb(0.94, 0.93, 0.9);
    let dark = rgb(0.12, 0.12, 0.13);
    match item {
        // Une plante se montre à plat, telle qu'on la voit plantée.
        Item::Block(block) if block.is_plant() => return flat(block),
        Item::Block(block) => return cube(block),
        Item::Stick => c.thick((4, 12), (11, 3), handle()),
        Item::IronIngot => {
            c.rect(3, 7, 12, 11, rgb(0.78, 0.8, 0.84));
            c.rect(5, 5, 10, 6, rgb(0.9, 0.92, 0.95));
        }
        Item::RawIron => {
            c.disc(8.0, 8.5, 5.0, rgb(0.5, 0.47, 0.45));
            for (x, y) in [(6, 7), (9, 6), (8, 10), (10, 9)] {
                c.set(x, y, rgb(0.95, 0.65, 0.45));
            }
        }
        Item::Coal => {
            c.disc(8.0, 8.5, 5.0, rgb(0.16, 0.16, 0.17));
            c.set(6, 6, rgb(0.4, 0.4, 0.42));
            c.set(9, 9, rgb(0.35, 0.35, 0.38));
        }
        Item::Apple => {
            c.disc(8.0, 9.0, 5.0, red);
            c.set(6, 7, rgb(1.0, 0.6, 0.55));
            c.line((8, 4), (8, 3), handle());
            c.line((9, 3), (11, 2), rgb(0.3, 0.7, 0.2));
        }
        Item::Ration => {
            c.rect(4, 5, 11, 12, rgb(0.6, 0.62, 0.55));
            c.rect(4, 7, 11, 9, rgb(0.75, 0.3, 0.2));
        }
        Item::Bandage => {
            c.disc(8.0, 8.0, 5.0, white);
            c.rect(7, 5, 8, 10, red);
            c.rect(5, 7, 10, 8, red);
        }
        Item::Sword(t) => {
            c.thick((5, 10), (12, 3), tier_color(t));
            c.line((3, 9), (6, 12), rgb(0.35, 0.3, 0.25));
            c.line((2, 13), (4, 11), handle());
        }
        Item::Pickaxe(t) => {
            c.thick((4, 13), (10, 5), handle());
            c.line((5, 3), (9, 3), tier_color(t));
            c.line((9, 3), (13, 7), tier_color(t));
            c.line((5, 3), (4, 5), tier_color(t));
            c.line((13, 7), (12, 8), tier_color(t));
        }
        Item::Axe(t) => {
            c.thick((4, 13), (10, 4), handle());
            c.rect(9, 2, 13, 6, tier_color(t));
        }
        Item::Spear(t) => {
            c.line((2, 14), (11, 5), handle());
            c.thick((11, 4), (13, 2), tier_color(t));
        }
        Item::Mace => {
            c.thick((4, 13), (9, 7), handle());
            c.disc(11.0, 5.0, 3.2, rgb(0.5, 0.5, 0.52));
            for (x, y) in [(11, 0), (15, 5), (11, 9), (7, 5)] {
                c.set(x, y, rgb(0.7, 0.7, 0.72));
            }
        }
        Item::Bow | Item::Crossbow => {
            for i in 0..12 {
                let t = i as f32 / 11.0;
                let x = 4.0 + (t * std::f32::consts::PI).sin() * 6.0;
                c.set(x as i32, 2 + i, handle());
            }
            c.line((4, 2), (4, 13), white);
            if item == Item::Crossbow {
                c.thick((2, 8), (13, 8), rgb(0.4, 0.28, 0.15));
            }
        }
        Item::Arrow | Item::Bolt => {
            c.line((3, 12), (12, 3), handle());
            c.set(12, 3, rgb(0.7, 0.7, 0.72));
            c.set(13, 2, rgb(0.7, 0.7, 0.72));
            if item == Item::Arrow {
                c.set(3, 11, white);
                c.set(4, 12, white);
            }
        }
        Item::Rifle => {
            c.rect(2, 6, 13, 7, rgb(0.25, 0.25, 0.27));
            c.rect(2, 8, 6, 10, rgb(0.45, 0.3, 0.15));
            c.rect(8, 8, 8, 10, rgb(0.25, 0.25, 0.27));
        }
        Item::Ammo => {
            for x in [4, 8, 12] {
                c.rect(x - 1, 6, x, 12, rgb(0.85, 0.65, 0.25));
                c.rect(x - 1, 4, x, 5, rgb(0.7, 0.45, 0.25));
            }
        }
        Item::Helmet(t) => {
            c.disc(8.0, 9.0, 6.0, tier_color(t));
            c.rect(0, 9, 15, 15, [0; 4]);
            c.rect(2, 9, 13, 10, tier_color(t));
        }
        Item::BodyArmor(t) => {
            c.rect(4, 3, 11, 13, tier_color(t));
            c.rect(2, 3, 3, 7, tier_color(t));
            c.rect(12, 3, 13, 7, tier_color(t));
            c.rect(7, 3, 8, 4, [0; 4]);
        }
        Item::Grenade => {
            c.disc(8.0, 9.0, 4.5, rgb(0.3, 0.4, 0.22));
            c.rect(7, 3, 8, 4, rgb(0.6, 0.6, 0.6));
            c.set(10, 3, rgb(0.8, 0.8, 0.3));
        }
        Item::Dynamite => {
            c.rect(5, 4, 10, 13, red);
            c.line((8, 3), (10, 1), white);
            c.set(11, 0, rgb(1.0, 0.8, 0.2));
        }
        Item::CannonBall => {
            c.disc(8.0, 8.0, 5.0, dark);
            c.set(6, 6, rgb(0.4, 0.4, 0.42));
        }
        Item::Seeds => {
            for (x, y) in [(5, 6), (9, 5), (7, 9), (11, 10), (4, 11)] {
                c.rect(x, y, x + 1, y, rgb(0.75, 0.62, 0.3));
            }
        }
        Item::Wheat => {
            for x in [5, 8, 11] {
                c.line((x, 13), (x, 5), rgb(0.7, 0.6, 0.25));
                c.rect(x - 1, 2, x, 5, rgb(0.92, 0.78, 0.35));
            }
        }
        Item::Bread => {
            c.disc(8.0, 9.0, 5.5, rgb(0.78, 0.52, 0.25));
            c.rect(0, 11, 15, 15, [0; 4]);
            c.line((5, 7), (6, 6), rgb(0.95, 0.75, 0.45));
            c.line((9, 7), (10, 6), rgb(0.95, 0.75, 0.45));
        }
        Item::RawMeat | Item::CookedMeat => {
            let (flesh, fat) = if item == Item::RawMeat {
                (rgb(0.85, 0.3, 0.32), rgb(0.97, 0.85, 0.85))
            } else {
                (rgb(0.55, 0.3, 0.15), rgb(0.8, 0.6, 0.4))
            };
            c.disc(9.0, 7.0, 4.5, flesh);
            c.disc(9.0, 7.0, 1.5, fat);
            c.thick((3, 13), (6, 10), white);
        }
        Item::Flask | Item::WaterFlask => {
            c.rect(5, 6, 10, 13, rgb(0.55, 0.4, 0.25));
            c.rect(7, 3, 8, 5, rgb(0.4, 0.3, 0.2));
            if item == Item::WaterFlask {
                c.rect(6, 9, 9, 12, rgb(0.3, 0.55, 0.9));
            }
        }
        Item::Hide => {
            c.disc(8.0, 8.0, 5.5, rgb(0.6, 0.42, 0.25));
            c.set(2, 3, [0; 4]);
            c.rect(3, 2, 4, 3, rgb(0.6, 0.42, 0.25));
            c.rect(11, 12, 12, 13, rgb(0.6, 0.42, 0.25));
        }
        Item::FurCoat => {
            c.rect(4, 3, 11, 14, rgb(0.55, 0.38, 0.22));
            c.rect(2, 4, 3, 11, rgb(0.55, 0.38, 0.22));
            c.rect(12, 4, 13, 11, rgb(0.55, 0.38, 0.22));
            c.rect(4, 3, 11, 4, rgb(0.9, 0.85, 0.75));
        }
        Item::StrawHat => {
            c.rect(1, 10, 14, 11, rgb(0.92, 0.8, 0.4));
            c.disc(8.0, 10.0, 4.0, rgb(0.85, 0.72, 0.35));
            c.rect(0, 11, 15, 15, [0; 4]);
            c.rect(4, 9, 11, 9, red);
        }
        Item::Antidote => potion(&mut c, rgb(0.35, 0.85, 0.35)),
        Item::Bucket | Item::WaterBucket => {
            c.rect(4, 6, 11, 13, rgb(0.6, 0.62, 0.66));
            c.line((4, 6), (8, 2), rgb(0.4, 0.4, 0.42));
            c.line((11, 6), (8, 2), rgb(0.4, 0.4, 0.42));
            if item == Item::WaterBucket {
                c.rect(5, 6, 10, 7, rgb(0.3, 0.55, 0.9));
            }
        }
        Item::Berries => {
            for (cx, cy) in [(6.0, 8.0), (10.0, 9.0), (8.0, 11.0)] {
                c.disc(cx, cy, 2.2, rgb(0.6, 0.1, 0.3));
                c.set(cx as i32 - 1, cy as i32 - 1, rgb(0.9, 0.5, 0.6));
            }
            c.line((8, 6), (9, 3), rgb(0.3, 0.6, 0.2));
        }
        Item::Herb => {
            for x in [5, 8, 11] {
                c.line((x, 13), (x, 5), rgb(0.3, 0.6, 0.25));
                c.disc(x as f32, 5.0, 2.0, rgb(0.45, 0.75, 0.35));
            }
            c.disc(8.0, 9.0, 1.5, rgb(0.9, 0.85, 0.4));
        }
        Item::CarrotSeeds => {
            for (x, y) in [(5, 6), (9, 5), (7, 9), (11, 10), (4, 11)] {
                c.rect(x, y, x + 1, y, rgb(0.85, 0.55, 0.3));
            }
        }
        Item::Carrot => {
            c.thick((10, 4), (6, 13), rgb(1.0, 0.55, 0.15));
            c.disc(10.0, 4.5, 1.8, rgb(1.0, 0.6, 0.2));
            c.line((11, 3), (13, 1), rgb(0.3, 0.7, 0.25));
            c.line((10, 3), (9, 1), rgb(0.3, 0.7, 0.25));
        }
        Item::Coffee => {
            c.rect(4, 7, 11, 13, rgb(0.9, 0.9, 0.92));
            c.rect(5, 8, 10, 11, rgb(0.3, 0.17, 0.1));
            c.rect(11, 8, 13, 11, rgb(0.9, 0.9, 0.92));
            c.set(7, 5, rgb(0.8, 0.8, 0.85));
            c.set(9, 4, rgb(0.8, 0.8, 0.85));
        }
        Item::Beer => {
            c.rect(5, 5, 10, 14, rgb(0.9, 0.6, 0.15));
            c.rect(5, 4, 10, 6, rgb(0.98, 0.95, 0.85));
            c.rect(10, 7, 12, 12, rgb(0.75, 0.78, 0.82));
        }
        Item::HerbalTea => potion(&mut c, rgb(0.5, 0.78, 0.4)),
        Item::Stew => {
            c.rect(3, 8, 12, 13, rgb(0.5, 0.5, 0.55));
            c.disc(7.5, 8.0, 4.0, rgb(0.6, 0.35, 0.18));
            c.set(6, 7, rgb(1.0, 0.6, 0.2));
            c.set(9, 8, rgb(0.5, 0.7, 0.3));
            c.set(7, 5, rgb(0.7, 0.7, 0.72));
        }
    }
    c.outline();
    c
}

/// Fiole remplie d'un liquide coloré.
fn potion(c: &mut Canvas, liquid: Rgba) {
    c.disc(8.0, 10.0, 4.5, rgb(0.85, 0.9, 0.95));
    c.disc(8.0, 10.5, 3.5, liquid);
    c.rect(7, 3, 8, 6, rgb(0.85, 0.9, 0.95));
    c.rect(7, 2, 8, 2, rgb(0.5, 0.35, 0.2));
}

fn to_image(canvas: &Canvas) -> Image {
    let mut data = Vec::with_capacity(N * N * 4);
    for row in canvas.0.iter() {
        for px in row {
            data.extend_from_slice(px);
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: N as u32,
            height: N as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::nearest();
    image
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::crafting::RECIPES;

    /// Chaque objet fabricable ou utilisé dans une recette a une icône non vide.
    #[test]
    fn every_item_has_a_visible_icon() {
        let mut items: Vec<Item> = RECIPES.iter().map(|r| r.output).collect();
        items.extend(RECIPES.iter().flat_map(|r| r.inputs.iter().map(|(i, _)| *i)));
        items.extend([
            Item::Apple,
            Item::RawMeat,
            Item::Coal,
            Item::RawIron,
            Item::Seeds,
            Item::Hide,
        ]);
        for item in items {
            let canvas = draw(item);
            let filled = (0..N)
                .flat_map(|y| (0..N).map(move |x| (x, y)))
                .filter(|(x, y)| canvas.get(*x, *y)[3] != 0)
                .count();
            assert!(filled >= 12, "{item:?} : {filled} pixels");
        }
    }
}
