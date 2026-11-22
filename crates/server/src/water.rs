//! Eau qui s'écoule. Les lacs et l'eau versée au seau sont des sources (`Block::Water`) ;
//! autour, l'eau courante (`Block::FlowingWater`) tombe dans les trous et s'étale jusqu'à
//! 7 blocs sur un sol plat. Si la source disparaît, l'eau courante se retire.
//!
//! Seuls les blocs qui viennent de changer, et leurs voisins, sont réexaminés.

use std::collections::{HashMap, HashSet, VecDeque};

use bevy::prelude::*;
use bevy_renet::RenetServer;
use shared::block::Block;
use shared::protocol::ServerMessage;

use crate::net::{WorldVersion, broadcast};
use crate::world::GameWorld;

/// Niveau d'une source ; l'eau courante perd un niveau par bloc parcouru à plat.
pub const SOURCE_LEVEL: u8 = 8;
/// Intervalle entre deux pas d'écoulement, en secondes.
const STEP_SECS: f32 = 0.25;
/// Cases examinées au plus par pas.
const CELLS_PER_STEP: usize = 512;

const SIDES: [IVec3; 4] = [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z];

pub struct WaterPlugin;

impl Plugin for WaterPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WaterSim>().add_systems(FixedUpdate, flow_water);
    }
}

/// État de l'écoulement.
#[derive(Resource, Default, Debug)]
pub struct WaterSim {
    /// Niveau de chaque bloc d'eau courante (1 à 7).
    levels: HashMap<IVec3, u8>,
    /// Cases à réexaminer, sans doublon.
    queue: VecDeque<IVec3>,
    queued: HashSet<IVec3>,
    timer: f32,
}

impl WaterSim {
    fn push(&mut self, pos: IVec3) {
        if self.queued.insert(pos) {
            self.queue.push_back(pos);
        }
    }

    fn push_around(&mut self, pos: IVec3) {
        self.push(pos);
        self.push(pos + IVec3::Y);
        self.push(pos - IVec3::Y);
        for side in SIDES {
            self.push(pos + side);
        }
    }

    /// Niveau de l'eau à cette case (0 si pas d'eau).
    fn level(&self, world: &GameWorld, pos: IVec3) -> u8 {
        match world.block(pos) {
            Block::Water => SOURCE_LEVEL,
            // Eau courante d'une ancienne partie, sans niveau connu : la plus faible.
            Block::FlowingWater => self.levels.get(&pos).copied().unwrap_or(1),
            _ => 0,
        }
    }

    /// Niveau que l'eau voisine donne à cette case.
    fn supplied(&self, world: &GameWorld, pos: IVec3) -> u8 {
        if world.block(pos + IVec3::Y).is_water() {
            return SOURCE_LEVEL - 1;
        }
        SIDES
            .iter()
            .map(|side| pos + *side)
            .filter(|n| {
                // Une eau qui tombe ne s'étale pas : il lui faut un appui (ou être une source).
                let below = world.block(*n - IVec3::Y);
                world.block(*n) == Block::Water || (below.is_solid() || below == Block::Water)
            })
            .map(|n| self.level(world, n))
            .max()
            .unwrap_or(0)
            .saturating_sub(1)
    }

    /// Réexamine une case ; renvoie le nouveau bloc s'il change.
    fn update(&mut self, world: &GameWorld, pos: IVec3) -> Option<Block> {
        let block = world.block(pos);
        if block != Block::Air && block != Block::FlowingWater {
            self.levels.remove(&pos);
            return None;
        }
        if !world.in_bounds(pos) {
            return None;
        }
        let wanted = self.supplied(world, pos);
        let current = if block == Block::FlowingWater {
            self.levels.get(&pos).copied().unwrap_or(0)
        } else {
            0
        };
        if wanted == current {
            return None;
        }
        if wanted == 0 {
            self.levels.remove(&pos);
            return Some(Block::Air);
        }
        self.levels.insert(pos, wanted);
        // Seul le niveau change : rien à dire aux clients, mais les voisins sont à revoir.
        self.push_around(pos);
        (block != Block::FlowingWater).then_some(Block::FlowingWater)
    }
}

/// Fait avancer l'écoulement d'un pas.
pub fn step(sim: &mut WaterSim, world: &mut GameWorld) -> Vec<(IVec3, Block)> {
    world.track_changes();
    for pos in world.take_changes() {
        sim.push_around(pos);
    }
    let mut changed = Vec::new();
    for _ in 0..CELLS_PER_STEP.min(sim.queue.len()) {
        let Some(pos) = sim.queue.pop_front() else { break };
        sim.queued.remove(&pos);
        if let Some(block) = sim.update(world, pos)
            && world.set_block(pos, block)
        {
            changed.push((pos, block));
        }
    }
    // Les changements de ce pas ont déjà mis leurs voisins en file.
    for pos in world.take_changes() {
        sim.push_around(pos);
    }
    changed
}

fn flow_water(
    time: Res<Time>,
    mut sim: ResMut<WaterSim>,
    mut world: ResMut<GameWorld>,
    mut version: ResMut<WorldVersion>,
    mut server: Option<ResMut<RenetServer>>,
) {
    sim.timer += time.delta_secs();
    if sim.timer < STEP_SECS {
        return;
    }
    sim.timer = 0.0;
    for (pos, block) in step(&mut sim, &mut world) {
        version.0 += 1;
        if let Some(server) = server.as_deref_mut() {
            broadcast(server, &ServerMessage::BlockChanged { pos, block });
        }
    }
}

#[cfg(test)]
mod tests {
    use shared::config::WorldConfig;
    use shared::world::WorldState;

    use super::*;

    /// Une dalle de pierre dans le ciel, entourée d'un rebord.
    fn basin() -> (GameWorld, IVec3) {
        let mut world = GameWorld(WorldState::new(WorldConfig::default()));
        world.track_changes();
        let y = world.generator().surface_height(60, 60) + 20;
        for x in 40..=80 {
            for z in 40..=80 {
                world.set_block(IVec3::new(x, y - 1, z), Block::Stone);
            }
        }
        (world, IVec3::new(60, y, 60))
    }

    fn settle(sim: &mut WaterSim, world: &mut GameWorld) {
        for _ in 0..200 {
            step(sim, world);
        }
    }

    #[test]
    fn a_poured_source_spreads_seven_blocks_then_stops() {
        let (mut world, center) = basin();
        let mut sim = WaterSim::default();
        world.set_block(center, Block::Water);
        settle(&mut sim, &mut world);
        for d in 1..=7 {
            assert_eq!(world.block(center + IVec3::X * d), Block::FlowingWater, "à {d} blocs");
        }
        assert_eq!(world.block(center + IVec3::X * 8), Block::Air);
        assert_eq!(world.block(center + IVec3::Y), Block::Air, "l'eau ne monte pas");
    }

    #[test]
    fn water_falls_down_a_hole() {
        let (mut world, center) = basin();
        let mut sim = WaterSim::default();
        let hole = center + IVec3::new(2, -1, 0);
        world.set_block(hole, Block::Air);
        world.set_block(hole - IVec3::Y, Block::Stone);
        world.set_block(center, Block::Water);
        settle(&mut sim, &mut world);
        assert_eq!(world.block(hole), Block::FlowingWater);
    }

    #[test]
    fn flowing_water_dries_up_when_the_source_is_removed() {
        let (mut world, center) = basin();
        let mut sim = WaterSim::default();
        world.set_block(center, Block::Water);
        settle(&mut sim, &mut world);
        world.set_block(center, Block::Air);
        settle(&mut sim, &mut world);
        for d in 0..=8 {
            assert_eq!(world.block(center + IVec3::X * d), Block::Air, "à {d} blocs");
        }
    }

    #[test]
    fn a_wall_holds_the_water_back() {
        let (mut world, center) = basin();
        let mut sim = WaterSim::default();
        for z in -10..=10 {
            world.set_block(center + IVec3::new(2, 0, z), Block::Stone);
        }
        world.set_block(center, Block::Water);
        settle(&mut sim, &mut world);
        assert_eq!(world.block(center + IVec3::X), Block::FlowingWater);
        assert_eq!(world.block(center + IVec3::X * 3), Block::Air);
    }
}
