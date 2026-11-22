//! État du monde côté serveur : le terrain généré et les modifications des joueurs.

use std::collections::HashMap;

use bevy::prelude::*;
use shared::block::Block;
use shared::item::{Inventory, Item, Tier};
use shared::world::WorldState;

use crate::ServerConfig;
use crate::daynight::Clock;
use crate::save::{self, SavedPlayers};

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, setup_world);
    }
}

/// Le monde de la partie. Seul le serveur a le droit de le modifier.
#[derive(Resource, Deref, DerefMut)]
pub struct GameWorld(pub WorldState);

/// Récoltes restantes des blocs de filon déjà entamés (absents = intacts).
#[derive(Resource, Default, Debug)]
pub struct Deposits(pub HashMap<IVec3, u8>);

/// Contenu des coffres posés dans le monde.
#[derive(Resource, Default, Debug)]
pub struct Chests(pub HashMap<IVec3, Inventory>);

impl Chests {
    /// Un coffre de ruine jamais ouvert reçoit son butin (tiré au sort d'après la graine et la position).
    /// Le coffre vidé reste dans la liste : il ne se remplit pas une seconde fois.
    pub fn fill_ruin_chest(&mut self, world: &WorldState, pos: IVec3) {
        let generated = world.generator().block_at(pos.x, pos.y, pos.z) == Block::Chest;
        if !generated || world.edits().contains_key(&pos) || self.0.contains_key(&pos) {
            return;
        }
        let seed = (world.config().seed as u64) << 32 ^ (pos.x as u64) << 20 ^ (pos.y as u64) << 10 ^ pos.z as u64;
        self.0.insert(pos, ruin_loot(seed));
    }

    /// Vrai si ce coffre fait partie d'une ruine générée (on garde alors sa trace même vide).
    pub fn is_ruin_chest(world: &WorldState, pos: IVec3) -> bool {
        world.generator().block_at(pos.x, pos.y, pos.z) == Block::Chest
    }
}

/// Butin d'un coffre de ruine : de quoi tenir quelques nuits, parfois une arme.
pub fn ruin_loot(seed: u64) -> Inventory {
    let mut rng = fastrand::Rng::with_seed(seed);
    let mut loot = Inventory::default();
    let mut maybe = |chance: f32, item: Item, min: u32, max: u32| {
        if rng.f32() < chance {
            let n = rng.u32(min..=max);
            loot.add(item, n);
        }
    };
    maybe(0.8, Item::Apple, 2, 5);
    maybe(0.5, Item::Ration, 1, 2);
    maybe(0.5, Item::Bandage, 1, 2);
    maybe(0.6, Item::Block(Block::Planks), 4, 10);
    maybe(0.35, Item::IronIngot, 1, 3);
    maybe(0.4, Item::Arrow, 6, 12);
    maybe(0.2, Item::Ammo, 4, 10);
    maybe(0.12, Item::Bow, 1, 1);
    maybe(0.15, Item::Sword(Tier::Stone), 1, 1);
    maybe(0.06, Item::Rifle, 1, 1);
    if loot.is_empty() {
        loot.add(Item::Apple, 3);
    }
    loot
}

/// Point d'apparition des joueurs, au centre de la carte, posé sur le sol.
#[derive(Resource, Clone, Copy, Debug)]
pub struct SpawnPoint(pub Vec3);

fn setup_world(mut commands: Commands, mut config: ResMut<ServerConfig>) {
    let save = match save::load(&config.data_dir) {
        Ok(save) => save,
        Err(error) => {
            // On met le fichier illisible de côté plutôt que de l'écraser.
            let path = save::save_path(&config.data_dir);
            let aside = path.with_extension("sav.illisible");
            error!(
                "Sauvegarde illisible ({error}) : déplacée vers {}, nouvelle partie",
                aside.display()
            );
            let _ = std::fs::rename(&path, &aside);
            None
        }
    };
    let Some(save) = save else {
        info!("Nouvelle partie (aucune sauvegarde dans {})", config.data_dir.display());
        let world = WorldState::new(config.world.clone());
        commands.insert_resource(SpawnPoint(find_spawn(&world)));
        commands.insert_resource(GameWorld(world));
        commands.insert_resource(Deposits::default());
        commands.insert_resource(Chests::default());
        commands.insert_resource(save::SavedSurvivors::default());
        return;
    };
    if save.world != config.world {
        warn!(
            "La sauvegarde utilise la graine {} et une carte de {} chunks : ces réglages remplacent ceux de la configuration",
            save.world.seed, save.world.size_chunks
        );
        config.world = save.world.clone();
    }
    let mut world = WorldState::new(save.world);
    for (pos, block) in &save.edits {
        world.set_block(*pos, *block);
    }
    info!(
        "Sauvegarde chargée : jour {}, {} blocs modifiés, {} joueurs connus",
        save.time.day,
        save.edits.len(),
        save.players.len()
    );
    commands.insert_resource(Clock(save.time));
    commands.insert_resource(SavedPlayers(save.players.into_iter().collect()));
    commands.insert_resource(Deposits(save.deposits.into_iter().collect()));
    commands.insert_resource(Chests(save.chests.into_iter().collect()));
    commands.insert_resource(save::SavedSurvivors(save.survivors));
    commands.insert_resource(SpawnPoint(find_spawn(&world)));
    commands.insert_resource(GameWorld(world));
}

/// Cherche, en spirale autour du centre, une colonne avec un vrai sol (pas une grotte ouverte).
pub fn find_spawn(world: &WorldState) -> Vec3 {
    let center = world.config().size_blocks() / 2;
    for radius in 0..64i32 {
        for dx in -radius..=radius {
            for dz in -radius..=radius {
                if dx.abs() != radius && dz.abs() != radius {
                    continue;
                }
                let (x, z) = (center + dx, center + dz);
                let ground = world.generator().surface_height(x, z);
                let solid = |y: i32| world.is_solid(IVec3::new(x, y, z));
                let dry = world.block(IVec3::new(x, ground, z)) == Block::Air;
                if dry && solid(ground - 1) && solid(ground - 2) && !solid(ground) && !solid(ground + 1) {
                    return Vec3::new(x as f32 + 0.5, ground as f32, z as f32 + 0.5);
                }
            }
        }
    }
    let ground = world.generator().surface_height(center, center);
    Vec3::new(center as f32 + 0.5, ground as f32 + 1.0, center as f32 + 0.5)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::config::WorldConfig;
    use shared::physics::{PLAYER_SIZE, overlaps_solid};

    #[test]
    fn spawn_is_standing_on_solid_ground() {
        for seed in [1, 42, 1234] {
            let world = WorldState::new(WorldConfig {
                seed,
                ..WorldConfig::default()
            });
            let spawn = find_spawn(&world);
            let below = (spawn - Vec3::Y * 0.5).floor().as_ivec3();
            assert!(world.is_solid(below), "graine {seed} : pas de sol sous {spawn}");
            assert!(!overlaps_solid(spawn, PLAYER_SIZE, &|p| world.is_solid(p)));
        }
    }
}

#[cfg(test)]
mod ruin_tests {
    use super::*;
    use shared::config::WorldConfig;

    #[test]
    fn ruin_chests_are_filled_once() {
        let world = WorldState::new(WorldConfig::default());
        let generator = world.generator().clone();
        let ruin = (0..8)
            .flat_map(|cx| (0..8).map(move |cz| (cx, cz)))
            .find_map(|(cx, cz)| generator.ruin_in_cell(cx, cz))
            .expect("une ruine");
        let (x, y, z) = generator.ruin_chests(ruin)[0];
        let pos = IVec3::new(x, y, z);
        let mut chests = Chests::default();
        chests.fill_ruin_chest(&world, pos);
        let loot = chests.0.get(&pos).cloned().expect("butin");
        assert!(!loot.is_empty());
        // Le même coffre donne le même butin, et un coffre vidé ne se remplit plus.
        assert_eq!(ruin_loot(1), ruin_loot(1));
        chests.0.insert(pos, Inventory::default());
        chests.fill_ruin_chest(&world, pos);
        assert!(chests.0[&pos].is_empty());
        // Un coffre posé ailleurs par un joueur n'a pas de butin.
        let mut other = Chests::default();
        other.fill_ruin_chest(&world, pos + IVec3::Y * 5);
        assert!(other.0.is_empty());
    }
}
