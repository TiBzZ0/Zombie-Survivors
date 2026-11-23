//! Vie des arbres : les pommiers se couvrent de pommes qui finissent par tomber et pourrir,
//! les pousses plantées deviennent des arbres, et le feuillage d'un arbre abattu tombe peu
//! à peu, en laissant parfois une pousse au pied.

use std::collections::HashSet;

use bevy::prelude::*;
use bevy_renet::RenetServer;
use shared::block::Block;
use shared::protocol::ServerMessage;
use shared::trees::TreeKind;
use shared::worldgen::TREE_CELL;

use crate::net::{Lobby, WorldVersion, broadcast};
use crate::world::GameWorld;

pub struct TreesPlugin;

impl Plugin for TreesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LeafDecay>()
            .add_systems(FixedUpdate, (orchards, leaf_decay));
    }
}

/// Durées moyennes, en secondes : une pomme mûrit, tombe, pourrit, puis disparaît.
pub const RIPEN_SECS: f32 = 240.0;
pub const DROP_SECS: f32 = 300.0;
pub const ROT_SECS: f32 = 150.0;
pub const VANISH_SECS: f32 = 150.0;
/// Durée moyenne avant qu'une pousse ne devienne un arbre.
pub const SAPLING_SECS: f32 = 300.0;
/// Les pommiers naturels ne vivent qu'autour des joueurs (dans ce rayon, en blocs).
const ORCHARD_RADIUS: i32 = 40;
/// Distance (en blocs, sur chaque axe) au-delà de laquelle une feuille n'est plus tenue par un tronc.
const LEAF_REACH: i32 = 3;
/// Chance qu'une feuille qui tombe laisse une pousse au sol.
const SAPLING_DROP_CHANCE: f32 = 0.06;

/// Feuilles qui vont tomber, avec l'instant de leur chute.
#[derive(Resource, Default)]
pub struct LeafDecay {
    pending: Vec<(IVec3, f64)>,
    queued: HashSet<IVec3>,
}

/// Case où se pose ce qui tombe de `from` : le premier vide au-dessus d'un sol solide.
pub fn landing(world: &GameWorld, from: IVec3) -> Option<IVec3> {
    let mut pos = from - IVec3::Y;
    for _ in 0..48 {
        let below = world.block(pos - IVec3::Y);
        if below.is_solid() {
            return matches!(world.block(pos), Block::Air | Block::TallGrass).then_some(pos);
        }
        if !matches!(world.block(pos), Block::Air | Block::TallGrass) || pos.y <= 1 {
            return None;
        }
        pos -= IVec3::Y;
    }
    None
}

/// Une feuille est-elle encore reliée à un tronc proche ?
fn held_by_trunk(world: &GameWorld, leaf: IVec3) -> bool {
    let r = LEAF_REACH;
    (-r..=r).any(|x| (-r - 1..=r).any(|y| (-r..=r).any(|z| world.block(leaf + IVec3::new(x, y, z)).is_log())))
}

fn apply(
    world: &mut GameWorld,
    version: &mut WorldVersion,
    server: &mut Option<ResMut<RenetServer>>,
    changes: Vec<(IVec3, Block)>,
) {
    for (pos, block) in changes {
        if world.block(pos) != block && world.set_block(pos, block) {
            version.0 += 1;
            if let Some(server) = server.as_deref_mut() {
                broadcast(server, &ServerMessage::BlockChanged { pos, block });
            }
        }
    }
}

/// Les blocs d'un arbre qui pousse à la place d'une pousse, s'il a la place.
pub fn grown_tree(world: &GameWorld, pos: IVec3, kind: TreeKind, height: i32) -> Option<Vec<(IVec3, Block)>> {
    let blocks = kind.blocks(height);
    let trunk_free = blocks
        .iter()
        .filter(|(o, b)| *b == kind.log() && *o != IVec3::ZERO)
        .all(|(o, _)| world.block(pos + *o).is_replaceable());
    trunk_free.then(|| {
        blocks
            .into_iter()
            .map(|(o, b)| (pos + o, b))
            .filter(|(p, _)| *p == pos || world.block(*p).is_replaceable())
            .collect()
    })
}

/// Une fois par seconde : pommes, pousses et fruits tombés évoluent.
fn orchards(
    time: Res<Time>,
    mut world: ResMut<GameWorld>,
    mut version: ResMut<WorldVersion>,
    lobby: Res<Lobby>,
    mut server: Option<ResMut<RenetServer>>,
    mut timer: Local<f32>,
) {
    *timer += time.delta_secs();
    if *timer < 1.0 {
        return;
    }
    let elapsed = std::mem::take(&mut *timer);
    let chance = |secs: f32| fastrand::f32() < elapsed / secs;
    // Ce qui a déjà changé (pousses, pommes tombées, feuillages modifiés)...
    let mut candidates: HashSet<IVec3> = world
        .edits()
        .iter()
        .filter(|(_, b)| {
            b.is_sapling()
                || matches!(
                    b,
                    Block::FruitLeaves | Block::RipeFruitLeaves | Block::FallenFruit | Block::RottenFruit
                )
        })
        .map(|(p, _)| *p)
        .collect();
    // ... et les pommiers naturels autour des joueurs.
    let generator = world.generator().clone();
    for player in lobby.players.values().filter(|p| p.is_alive()) {
        let p = player.state.position.as_ivec3();
        let cells = |c: i32| (c - ORCHARD_RADIUS).div_euclid(TREE_CELL)..=(c + ORCHARD_RADIUS).div_euclid(TREE_CELL);
        for cx in cells(p.x) {
            for cz in cells(p.z) {
                let Some(tree) = generator.tree_in_cell(cx, cz).filter(|t| t.kind == TreeKind::Apple) else {
                    continue;
                };
                let foot = IVec3::new(tree.x, tree.ground, tree.z);
                candidates.extend(
                    tree.kind
                        .blocks(tree.height)
                        .into_iter()
                        .filter(|(_, b)| b.is_leaves())
                        .map(|(o, _)| foot + o),
                );
            }
        }
    }
    let mut changes = Vec::new();
    for pos in candidates {
        match world.block(pos) {
            // Les pommes pendent sous le feuillage.
            Block::FruitLeaves if world.block(pos - IVec3::Y) == Block::Air && chance(RIPEN_SECS) => {
                changes.push((pos, Block::RipeFruitLeaves));
            }
            Block::RipeFruitLeaves if chance(DROP_SECS) => {
                changes.push((pos, Block::FruitLeaves));
                if let Some(spot) = landing(&world, pos) {
                    changes.push((spot, Block::FallenFruit));
                }
            }
            Block::FallenFruit if chance(ROT_SECS) => changes.push((pos, Block::RottenFruit)),
            Block::RottenFruit if chance(VANISH_SECS) => changes.push((pos, Block::Air)),
            sapling if sapling.is_sapling() => {
                if !matches!(world.block(pos - IVec3::Y), Block::Grass | Block::Dirt) {
                    changes.push((pos, Block::Air));
                } else if chance(SAPLING_SECS)
                    && let Some(kind) = TreeKind::of_sapling(sapling)
                    && let Some(tree) = grown_tree(&world, pos, kind, kind.height(fastrand::u32(..)))
                {
                    changes.extend(tree);
                }
            }
            _ => {}
        }
    }
    apply(&mut world, &mut version, &mut server, changes);
}

/// Le feuillage d'un arbre abattu tombe feuille à feuille, en quelques secondes.
fn leaf_decay(
    time: Res<Time>,
    mut world: ResMut<GameWorld>,
    mut version: ResMut<WorldVersion>,
    mut decay: ResMut<LeafDecay>,
    mut server: Option<ResMut<RenetServer>>,
) {
    let now = time.elapsed_secs_f64();
    for log in world.take_felled() {
        let r = LEAF_REACH + 1;
        for x in -r..=r {
            for y in -r..=r + 1 {
                for z in -r..=r {
                    let leaf = log + IVec3::new(x, y, z);
                    if world.block(leaf).is_leaves() && !decay.queued.contains(&leaf) && !held_by_trunk(&world, leaf) {
                        decay.queued.insert(leaf);
                        decay.pending.push((leaf, now + 0.5 + fastrand::f64() * 6.0));
                    }
                }
            }
        }
    }
    if decay.pending.is_empty() {
        return;
    }
    let (due, later): (Vec<_>, Vec<_>) = decay.pending.drain(..).partition(|(_, t)| *t <= now);
    decay.pending = later;
    let mut changes = Vec::new();
    for (leaf, _) in due {
        decay.queued.remove(&leaf);
        let block = world.block(leaf);
        if !block.is_leaves() || held_by_trunk(&world, leaf) {
            continue;
        }
        changes.push((leaf, Block::Air));
        let spot = landing(&world, leaf).filter(|s| !changes.iter().any(|(p, _)| p == s));
        if let Some(spot) = spot {
            if block == Block::RipeFruitLeaves {
                changes.push((spot, Block::FallenFruit));
            } else if fastrand::f32() < SAPLING_DROP_CHANCE
                && matches!(world.block(spot - IVec3::Y), Block::Grass | Block::Dirt)
                && let Some(sapling) = TreeKind::of_leaves(block).and_then(TreeKind::sapling)
            {
                changes.push((spot, sapling));
            }
        }
    }
    apply(&mut world, &mut version, &mut server, changes);
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;
    use shared::config::WorldConfig;
    use shared::world::WorldState;

    use super::*;

    fn app(world: GameWorld) -> App {
        let mut app = App::new();
        app.insert_resource(world)
            .init_resource::<WorldVersion>()
            .init_resource::<LeafDecay>()
            .init_resource::<Lobby>()
            .init_resource::<Time>();
        app
    }

    /// Fait tourner le verger seconde par seconde.
    fn run_for(app: &mut App, secs: u32) {
        for _ in 0..secs {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::from_secs(1));
            app.world_mut().run_system_once(orchards).unwrap();
        }
    }

    /// Un coin d'herbe naturelle dégagé.
    fn meadow(world: &GameWorld) -> IVec3 {
        (20..400)
            .flat_map(|x| (20..400).step_by(7).map(move |z| (x, z)))
            .map(|(x, z)| IVec3::new(x, world.generator().surface_height(x, z), z))
            .find(|p| {
                world.block(*p - IVec3::Y) == Block::Grass
                    && (-6..=6).all(|dx| {
                        (-6..=6).all(|dz| (-2..9).all(|dy| !world.block(*p + IVec3::new(dx, dy, dz)).is_log()))
                    })
                    && (-2..=2).all(|dx| {
                        (-2..=2).all(|dz| (0..9).all(|dy| !world.block(*p + IVec3::new(dx, dy, dz)).is_solid()))
                    })
            })
            .expect("un pré")
    }

    #[test]
    fn a_sapling_grows_into_an_apple_tree_that_bears_fruit() {
        let mut world = GameWorld(WorldState::new(WorldConfig::default()));
        let foot = meadow(&world);
        assert!(world.set_block(foot, Block::AppleSapling));
        let mut app = app(world);
        run_for(&mut app, 3600);
        let world = app.world().resource::<GameWorld>();
        assert_eq!(world.block(foot), Block::Wood, "la pousse est devenue un tronc");
        let fruit = world
            .edits()
            .values()
            .filter(|b| matches!(b, Block::RipeFruitLeaves | Block::FallenFruit | Block::RottenFruit))
            .count();
        assert!(fruit > 0, "des pommes ont poussé");
    }

    #[test]
    fn felling_a_tree_drops_its_leaves() {
        let mut world = GameWorld(WorldState::new(WorldConfig::default()));
        world.track_felled();
        let foot = meadow(&world);
        let tree = grown_tree(&world, foot, TreeKind::Oak, 4).unwrap();
        for (p, b) in &tree {
            world.set_block(*p, *b);
        }
        world.take_felled();
        for (p, b) in &tree {
            if b.is_log() {
                world.set_block(*p, Block::Air);
            }
        }
        let mut app = app(world);
        for _ in 0..20 {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::from_millis(500));
            app.world_mut().run_system_once(leaf_decay).unwrap();
        }
        let world = app.world().resource::<GameWorld>();
        assert!(
            tree.iter().all(|(p, _)| !world.block(*p).is_leaves()),
            "plus une feuille en l'air"
        );
    }

    #[test]
    fn fallen_fruit_rots_then_vanishes() {
        let mut world = GameWorld(WorldState::new(WorldConfig::default()));
        let spot = meadow(&world);
        assert!(world.set_block(spot, Block::FallenFruit));
        let mut app = app(world);
        run_for(&mut app, 3000);
        assert_eq!(app.world().resource::<GameWorld>().block(spot), Block::Air);
    }
}
