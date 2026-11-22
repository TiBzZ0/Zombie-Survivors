//! Effondrement des constructions : quand un bloc porteur disparaît (cassé, explosé, abattu
//! par les zombies), les blocs posés qui ne tiennent plus tombent tout droit et écrasent ce
//! qui se trouve dessous. Le calcul de solidité est dans `shared::structure`.

use bevy::prelude::*;
use bevy_renet::RenetServer;
use shared::block::Block;
use shared::protocol::ServerMessage;
use shared::structure::unsupported;

use crate::net::{Lobby, WorldVersion, broadcast, chest_destroyed};
use crate::world::{Chests, GameWorld};
use crate::zombies::{Zombie, ZombieBody, ZombieSet, hurt_player};

pub struct StructurePlugin;

impl Plugin for StructurePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            collapse_structures.after(ZombieSet::Ai).before(ZombieSet::Deaths),
        );
    }
}

/// Dégâts d'un bloc qui tombe, par bloc de chute.
const DEBRIS_DAMAGE_PER_BLOCK: f32 = 12.0;
const DEBRIS_DAMAGE_MAX: f32 = 60.0;
/// Chute au-delà de laquelle un bloc se brise au lieu de se reposer.
const MAX_FALL: i32 = 48;

/// Un bloc qui s'est effondré.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fall {
    pub from: IVec3,
    /// Case où il s'est reposé, ou `None` s'il s'est brisé.
    pub to: Option<IVec3>,
    pub block: Block,
}

/// Fait tomber les blocs posés qui ne tiennent plus depuis les dernières disparitions.
pub fn settle(world: &mut GameWorld) -> Vec<Fall> {
    world.track_removals();
    let removed = world.take_removals();
    if removed.is_empty() {
        return Vec::new();
    }
    let seeds: Vec<IVec3> = removed
        .iter()
        .flat_map(|p| [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z, IVec3::Y, IVec3::NEG_Y].map(|d| *p + d))
        .collect();
    let fallen = unsupported(&seeds, |p| world.block(p), |p| world.edits().contains_key(&p));
    let mut falls = Vec::with_capacity(fallen.len());
    for from in fallen {
        let block = world.block(from);
        world.set_block(from, Block::Air);
        // Tombe jusqu'au premier appui ; l'eau et les cultures ne l'arrêtent pas.
        let mut y = from.y;
        while y > 1 && from.y - y < MAX_FALL && !world.is_solid(IVec3::new(from.x, y - 1, from.z)) {
            y -= 1;
        }
        let spot = IVec3::new(from.x, y, from.z);
        let lands = world.is_solid(spot - IVec3::Y) && world.block(spot).is_replaceable();
        let to = (lands && world.set_block(spot, block)).then_some(spot);
        falls.push(Fall { from, to, block });
    }
    // Les blocs qui viennent de bouger ne doivent pas relancer un calcul inutile.
    world.take_removals();
    falls
}

#[allow(clippy::too_many_arguments)]
fn collapse_structures(
    mut world: ResMut<GameWorld>,
    mut version: ResMut<WorldVersion>,
    mut chests: ResMut<Chests>,
    mut lobby: ResMut<Lobby>,
    mut zombies: Query<(&mut Zombie, &ZombieBody)>,
    mut server: Option<ResMut<RenetServer>>,
) {
    let falls = settle(&mut world);
    if falls.is_empty() {
        return;
    }
    version.0 += 1;
    let mut center = Vec3::ZERO;
    for fall in &falls {
        center += fall.from.as_vec3();
        // Un coffre emporte son contenu dans sa chute, s'il se repose.
        if fall.block.is_storage() {
            let contents = chests.0.remove(&fall.from);
            if let Some(server) = server.as_deref_mut() {
                chest_destroyed(server, &mut lobby, &mut chests, fall.from, None);
            }
            if let (Some(to), Some(contents)) = (fall.to, contents) {
                chests.0.insert(to, contents);
            }
        }
        // Ce qui se trouve dans la colonne de chute est écrasé.
        let bottom = fall.to.map_or(fall.from.y - MAX_FALL, |t| t.y) as f32;
        let top = fall.from.y as f32 + 1.0;
        let height = (top - 1.0 - bottom).max(1.0);
        let damage = (height * DEBRIS_DAMAGE_PER_BLOCK).min(DEBRIS_DAMAGE_MAX);
        let column = |pos: Vec3, half: f32, tall: f32| {
            let (x, z) = (fall.from.x as f32, fall.from.z as f32);
            pos.x + half > x
                && pos.x - half < x + 1.0
                && pos.z + half > z
                && pos.z - half < z + 1.0
                && pos.y + tall > bottom
                && pos.y < top
        };
        for (mut z, body) in &mut zombies {
            let size = z.kind.stats().size;
            if column(body.position, size.half_width, size.height) {
                z.health -= damage;
                z.hurt_timer = 0.3;
            }
        }
        for (id, player) in lobby.players.iter_mut().filter(|(_, p)| p.is_alive()) {
            let size = shared::physics::PLAYER_SIZE;
            if column(player.state.position, size.half_width, size.height) {
                let amount = damage * (1.0 - player.inventory.armor_reduction());
                hurt_player(*id, player, amount, "Écrasé par un effondrement", server.as_deref_mut());
            }
        }
    }
    let Some(server) = server.as_deref_mut() else { return };
    for fall in &falls {
        broadcast(
            server,
            &ServerMessage::BlockChanged {
                pos: fall.from,
                block: Block::Air,
            },
        );
        if let Some(to) = fall.to {
            broadcast(
                server,
                &ServerMessage::BlockChanged {
                    pos: to,
                    block: fall.block,
                },
            );
        }
    }
    broadcast(
        server,
        &ServerMessage::Collapse {
            pos: center / falls.len() as f32 + Vec3::splat(0.5),
            blocks: falls.len().min(u16::MAX as usize) as u16,
        },
    );
}

#[cfg(test)]
mod tests {
    use shared::config::WorldConfig;
    use shared::world::WorldState;

    use super::*;

    /// Un bout de terrain naturel plat et sec, sur lequel on construit.
    fn site() -> (GameWorld, IVec3) {
        let mut world = GameWorld(WorldState::new(WorldConfig::default()));
        world.track_removals();
        let flat = |world: &GameWorld, x: i32, z: i32| {
            let h = world.generator().surface_height(x, z);
            (0..6).all(|dx| {
                let p = IVec3::new(x + dx, h - 1, z);
                world.generator().surface_height(x + dx, z) == h
                    && world.block(p).is_solid()
                    && (1..6).all(|dy| world.block(p + IVec3::Y * dy) == Block::Air)
            })
        };
        let (x, z) = (20..400)
            .flat_map(|x| (20..400).step_by(7).map(move |z| (x, z)))
            .find(|(x, z)| flat(&world, *x, *z))
            .expect("un terrain plat");
        let base = IVec3::new(x, world.generator().surface_height(x, z), z);
        (world, base)
    }

    #[test]
    fn breaking_a_pillar_brings_down_the_roof() {
        let (mut world, base) = site();
        for y in 0..3 {
            world.set_block(base + IVec3::Y * y, Block::Planks);
        }
        for x in 1..=3 {
            world.set_block(base + IVec3::new(x, 2, 0), Block::Planks);
        }
        assert!(settle(&mut world).is_empty(), "rien ne tombe tant que le pilier tient");
        world.set_block(base, Block::Air);
        let falls = settle(&mut world);
        assert_eq!(falls.len(), 5, "{falls:?}");
        // Tout est retombé au sol.
        for x in 1..=3 {
            assert_eq!(world.block(base + IVec3::new(x, 2, 0)), Block::Air);
            assert_eq!(world.block(base + IVec3::new(x, 0, 0)), Block::Planks);
        }
        assert_eq!(world.block(base), Block::Planks);
        assert_eq!(world.block(base + IVec3::Y), Block::Planks);
        assert!(settle(&mut world).is_empty(), "les décombres tiennent");
    }

    #[test]
    fn a_wall_on_the_ground_loses_nothing_when_a_block_is_removed() {
        let (mut world, base) = site();
        for x in 0..5 {
            for y in 0..3 {
                world.set_block(base + IVec3::new(x, y, 0), Block::Stone);
            }
        }
        world.set_block(base + IVec3::new(2, 0, 0), Block::Air);
        assert!(settle(&mut world).is_empty());
    }

    #[test]
    fn opening_a_door_keeps_the_lintel() {
        let (mut world, base) = site();
        world.set_block(base, Block::Door);
        world.set_block(base + IVec3::Y, Block::Door);
        world.set_block(base + IVec3::Y * 2, Block::Planks);
        world.set_block(base, Block::DoorOpen);
        world.set_block(base + IVec3::Y, Block::DoorOpen);
        assert!(settle(&mut world).is_empty());
    }
}
