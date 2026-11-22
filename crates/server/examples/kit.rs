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
        (Item::Block(Block::Turret), 2),
        (Item::Block(Block::BarbedWire), 16),
        (Item::Bow, 1),
        (Item::Arrow, 32),
        (Item::Rifle, 1),
        (Item::Ammo, 48),
        (Item::Pickaxe(Tier::Iron), 1),
        (Item::Sword(Tier::Iron), 1),
        (Item::Spear(Tier::Iron), 1),
        (Item::Axe(Tier::Stone), 1),
        (Item::Mace, 1),
        (Item::Crossbow, 1),
        (Item::Bolt, 24),
        (Item::Grenade, 6),
        (Item::Dynamite, 4),
        (Item::CannonBall, 12),
        (Item::Seeds, 8),
        (Item::RawMeat, 3),
        (Item::Wheat, 6),
        (Item::Block(Block::Campfire), 1),
        (Item::Block(Block::Stone), 32),
        (Item::Helmet(Tier::Iron), 1),
        (Item::BodyArmor(Tier::Wood), 1),
        (Item::Ration, 5),
        (Item::Flask, 1),
        (Item::WaterFlask, 2),
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
        weapon: leader.as_ref().map(|_| Item::Bow),
        armor: [None; 2],
        leader,
        order,
    };
    let survivors = vec![
        survivor("Marie", ground(0.0, -4.0), None, Order::Idle),
        survivor("Hugo", ground(2.5, -5.0), Some(name.clone()), Order::Guard),
    ];
    // Une rangée de barbelés et une tourelle chargée, un peu plus loin.
    let block_at = |dx: f32, dz: f32| ground(dx, dz).floor().as_ivec3();
    let mut edits: Vec<_> = (-2..=1)
        .map(|dx| (block_at(dx as f32, -8.0), Block::BarbedWire))
        .collect();
    let turret = block_at(3.0, -7.0);
    edits.push((turret, Block::Turret));
    // Les trois armes fixes, à portée de main sur la gauche.
    for (i, gun) in [Block::MachineGun, Block::Cannon, Block::Catapult]
        .into_iter()
        .enumerate()
    {
        edits.push((block_at(-2.0, -1.0 - 2.0 * i as f32), gun));
    }
    let mut ammo = Inventory::default();
    ammo.add(Item::Arrow, 20);
    let save = SaveFile {
        version: SAVE_VERSION,
        world: WorldConfig::default(),
        time: GameTime::START,
        edits,
        players: vec![(
            name,
            SavedPlayer {
                position: spawn,
                health: 100.0,
                hunger: 100.0,
                inventory,
                thirst: 100.0,
                infection: 0.0,
            },
        )],
        deposits: Vec::new(),
        chests: vec![(turret, ammo)],
        survivors,
        infections: Vec::new(),
    };
    write(std::path::Path::new(&dir), &save).expect("écriture de la sauvegarde");
    println!("Sauvegarde écrite dans {dir}");
}
