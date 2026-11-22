//! Outil de test : crée une sauvegarde où un joueur possède un lot d'objets.
//! `cargo run -p server --example kit -- <dossier> <pseudo>`

use bevy::math::Vec3;
use server::save::{SAVE_VERSION, SaveFile, SavedPlayer, SavedSurvivor, write};
use shared::block::Block;
use shared::config::WorldConfig;
use shared::daytime::GameTime;
use shared::item::{Inventory, Item, Tier};
use shared::survivor::{Order, SurvivorStats};
use shared::world::WorldState;

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().expect("dossier de sauvegarde");
    let name = args.next().unwrap_or_else(|| "Testeur".into());
    let world = WorldState::new(WorldConfig::default());
    let mut inventory = Inventory::default();
    for (item, n) in [
        (Item::Block(Block::Door), 4),
        (Item::Block(Block::Chest), 2),
        (Item::Block(Block::Planks), 64),
        (Item::Block(Block::ReinforcedWall), 16),
        (Item::Block(Block::Spikes), 8),
        (Item::Bow, 1),
        (Item::Arrow, 32),
        (Item::Rifle, 1),
        (Item::Ammo, 48),
        (Item::Pickaxe(Tier::Iron), 1),
        (Item::Sword(Tier::Iron), 1),
        (Item::Ration, 5),
    ] {
        inventory.add(item, n);
    }
    // Deux survivants devant le joueur (qui regarde vers -Z) : un à recruter, un déjà recruté.
    let spawn = server::world::find_spawn(&world);
    let ground = |dx: f32, dz: f32| {
        let (x, z) = ((spawn.x + dx).floor(), (spawn.z + dz).floor());
        let y = world.generator().surface_height(x as i32, z as i32) as f32;
        Vec3::new(x + 0.5, y, z + 0.5)
    };
    let survivor = |survivor_name: &str, at: Vec3, leader: Option<String>, order: Order| SavedSurvivor {
        name: survivor_name.into(),
        stats: SurvivorStats::from_seed(survivor_name.len() as u32 * 7919),
        health: 70.0,
        position: at,
        post: at,
        leader,
        order,
    };
    let survivors = vec![
        survivor("Marie", ground(0.0, -4.0), None, Order::Idle),
        survivor("Hugo", ground(2.5, -5.0), Some(name.clone()), Order::Guard),
    ];
    let save = SaveFile {
        version: SAVE_VERSION,
        world: WorldConfig::default(),
        time: GameTime::START,
        edits: Vec::new(),
        players: vec![(
            name,
            SavedPlayer {
                position: spawn,
                health: 100.0,
                hunger: 100.0,
                inventory,
            },
        )],
        deposits: Vec::new(),
        chests: Vec::new(),
        survivors,
    };
    write(std::path::Path::new(&dir), &save).expect("écriture de la sauvegarde");
    println!("Sauvegarde écrite dans {dir}");
}
