//! Cultures : le blé semé pousse tout seul, en trois étapes.

use bevy::prelude::*;
use bevy_renet::RenetServer;
use shared::block::Block;
use shared::protocol::ServerMessage;

use crate::net::{WorldVersion, broadcast};
use crate::world::GameWorld;

pub struct FarmingPlugin;

impl Plugin for FarmingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, grow_crops);
    }
}

/// Durée moyenne d'une étape de pousse, en secondes (environ 3 minutes de la graine au blé mûr).
pub const STAGE_SECS: f32 = 90.0;

/// Étape suivante d'une culture.
pub const fn next_stage(block: Block) -> Option<Block> {
    match block {
        Block::Sprout => Some(Block::YoungWheat),
        Block::YoungWheat => Some(Block::RipeWheat),
        Block::CarrotSprout => Some(Block::CarrotPlant),
        Block::CarrotPlant => Some(Block::RipeCarrot),
        _ => None,
    }
}

/// Une fois par seconde, chaque culture a une chance de passer à l'étape suivante.
/// Une culture qui n'a plus de terre sous elle se dessèche.
fn grow_crops(
    time: Res<Time>,
    mut world: ResMut<GameWorld>,
    mut version: ResMut<WorldVersion>,
    mut server: Option<ResMut<RenetServer>>,
    mut timer: Local<f32>,
) {
    *timer += time.delta_secs();
    if *timer < 1.0 {
        return;
    }
    let elapsed = std::mem::take(&mut *timer);
    let changes: Vec<(IVec3, Block)> = world
        .edits()
        .iter()
        .filter(|(_, b)| b.is_crop())
        .filter_map(|(pos, block)| {
            let soil = matches!(world.block(*pos - IVec3::Y), Block::Grass | Block::Dirt);
            if !soil {
                Some((*pos, Block::Air))
            } else {
                next_stage(*block)
                    .filter(|_| fastrand::f32() < elapsed / STAGE_SECS)
                    .map(|next| (*pos, next))
            }
        })
        .collect();
    for (pos, block) in changes {
        if world.set_block(pos, block) {
            version.0 += 1;
            if let Some(server) = server.as_deref_mut() {
                broadcast(server, &ServerMessage::BlockChanged { pos, block });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;
    use shared::config::WorldConfig;
    use shared::world::WorldState;

    use super::*;

    #[test]
    fn wheat_grows_and_withers_without_soil() {
        let mut world = GameWorld(WorldState::new(WorldConfig::default()));
        let y = world.generator().surface_height(20, 20);
        let field = IVec3::new(20, y, 20);
        assert!(world.set_block(field - IVec3::Y, Block::Dirt));
        assert!(world.set_block(field, Block::Sprout));
        // Une pousse en l'air, sans terre dessous.
        let floating = IVec3::new(20, y + 6, 20);
        assert!(world.set_block(floating, Block::Sprout));

        let mut app = App::new();
        app.insert_resource(world)
            .init_resource::<WorldVersion>()
            .init_resource::<Time>();
        // Vingt minutes de jeu, seconde par seconde : largement de quoi mûrir.
        for _ in 0..1200 {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::from_secs(1));
            app.world_mut().run_system_once(grow_crops).unwrap();
        }
        let world = app.world().resource::<GameWorld>();
        assert_eq!(world.block(field), Block::RipeWheat);
        assert_eq!(world.block(floating), Block::Air);
    }
}
