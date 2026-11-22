//! Outil de test : crée une sauvegarde où un joueur possède un lot d'objets.
//! `cargo run -p server --example kit -- <dossier> <pseudo>`

use server::save::{SAVE_VERSION, SaveFile, SavedPlayer, write};
use shared::block::Block;
use shared::config::WorldConfig;
use shared::daytime::GameTime;
use shared::item::{Inventory, Item, Tier};
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
    let save = SaveFile {
        version: SAVE_VERSION,
        world: WorldConfig::default(),
        time: GameTime::START,
        edits: Vec::new(),
        players: vec![(
            name,
            SavedPlayer {
                position: server::world::find_spawn(&world),
                health: 100.0,
                hunger: 100.0,
                inventory,
            },
        )],
        deposits: Vec::new(),
        chests: Vec::new(),
    };
    write(std::path::Path::new(&dir), &save).expect("écriture de la sauvegarde");
    println!("Sauvegarde écrite dans {dir}");
}
