//! Électricité côté client : les projecteurs alimentés s'allument (vraies lumières pour les
//! plus proches du joueur).

use std::collections::HashMap;

use bevy::prelude::*;
use shared::block::{Block, POWER_RADIUS};

use crate::AppState;
use crate::net::{ClientWorld, NetEvent};
use crate::player::LocalPlayer;

/// Nombre maximal de projecteurs éclairés en même temps (les plus proches).
const MAX_LIGHTS: usize = 8;
/// Distance au-delà de laquelle un projecteur n'éclaire plus pour ce joueur.
const LIGHT_DIST: f32 = 64.0;

pub struct ElectricityPlugin;

impl Plugin for ElectricityPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PoweredGenerators>()
            .add_systems(
                Update,
                (receive_power, update_lights)
                    .chain()
                    .run_if(in_state(AppState::Playing))
                    .run_if(resource_exists::<ClientWorld>),
            )
            .add_systems(OnExit(AppState::Playing), clear_lights);
    }
}

/// Générateurs en marche, d'après le serveur.
#[derive(Resource, Default)]
pub struct PoweredGenerators(pub Vec<IVec3>);

impl PoweredGenerators {
    pub fn powers(&self, pos: IVec3) -> bool {
        self.0
            .iter()
            .any(|g| g.as_vec3().distance(pos.as_vec3()) <= POWER_RADIUS)
    }
}

#[derive(Component)]
struct FloodlightLamp(IVec3);

fn receive_power(mut events: MessageReader<NetEvent>, mut power: ResMut<PoweredGenerators>) {
    for event in events.read() {
        if let NetEvent::Power(running) = event {
            power.0 = running.clone();
        }
    }
}

fn update_lights(
    mut commands: Commands,
    world: Res<ClientWorld>,
    power: Res<PoweredGenerators>,
    player: Res<LocalPlayer>,
    lamps: Query<(Entity, &FloodlightLamp)>,
    time: Res<Time>,
    mut timer: Local<f32>,
) {
    *timer -= time.delta_secs();
    if *timer > 0.0 {
        return;
    }
    *timer = 0.5;
    let mut lit: Vec<(IVec3, f32)> = world
        .edits()
        .iter()
        .filter(|(p, b)| **b == Block::Floodlight && power.powers(**p))
        .map(|(p, _)| (*p, p.as_vec3().distance(player.position)))
        .filter(|(_, d)| *d < LIGHT_DIST)
        .collect();
    lit.sort_by(|a, b| a.1.total_cmp(&b.1));
    lit.truncate(MAX_LIGHTS);
    let wanted: HashMap<IVec3, ()> = lit.iter().map(|(p, _)| (*p, ())).collect();
    let mut present = HashMap::new();
    for (entity, lamp) in &lamps {
        if wanted.contains_key(&lamp.0) {
            present.insert(lamp.0, ());
        } else {
            commands.entity(entity).despawn();
        }
    }
    for (pos, _) in lit {
        if present.contains_key(&pos) {
            continue;
        }
        commands.spawn((
            FloodlightLamp(pos),
            PointLight {
                color: Color::srgb(1.0, 0.95, 0.8),
                intensity: 2_000_000.0,
                range: 18.0,
                ..default()
            },
            Transform::from_translation(pos.as_vec3() + Vec3::new(0.5, 1.2, 0.5)),
        ));
    }
}

fn clear_lights(
    mut commands: Commands,
    lamps: Query<Entity, With<FloodlightLamp>>,
    mut power: ResMut<PoweredGenerators>,
) {
    for entity in &lamps {
        commands.entity(entity).despawn();
    }
    power.0.clear();
}
