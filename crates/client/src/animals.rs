//! Affichage du gibier envoyé par le serveur : un modèle de blocs par espèce,
//! interpolé, avec des pattes qui trottent.

use std::collections::HashMap;

use bevy::prelude::*;
use shared::animal::{AnimalKind, AnimalState};
use shared::raycast::ray_aabb;

use crate::AppState;
use crate::net::NetEvent;

pub struct AnimalsPlugin;

impl Plugin for AnimalsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AnimalRegistry>()
            .add_systems(Startup, setup_assets)
            .add_systems(
                Update,
                (receive_animals, animate_animals)
                    .chain()
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(OnExit(AppState::Playing), clear_animals);
    }
}

#[derive(Resource, Default)]
struct AnimalRegistry(HashMap<u32, Entity>);

#[derive(Component)]
pub struct AnimalView {
    pub id: u32,
    pub kind: AnimalKind,
    target: Vec3,
    target_yaw: f32,
    flags: u8,
    phase: f32,
}

/// Patte, avec son côté (pour alterner le pas).
#[derive(Component)]
struct Leg(f32);

#[derive(Resource)]
struct AnimalAssets {
    cube: Handle<Mesh>,
    /// Pelage, ventre (ou bois, défenses) par espèce.
    fur: [Handle<StandardMaterial>; 3],
    detail: [Handle<StandardMaterial>; 3],
}

fn setup_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut mat = |r, g, b| {
        materials.add(StandardMaterial {
            base_color: Color::srgb(r, g, b),
            perceptual_roughness: 1.0,
            ..default()
        })
    };
    commands.insert_resource(AnimalAssets {
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        fur: [mat(0.62, 0.55, 0.48), mat(0.55, 0.36, 0.20), mat(0.30, 0.24, 0.20)],
        detail: [mat(0.92, 0.90, 0.86), mat(0.85, 0.78, 0.62), mat(0.88, 0.85, 0.75)],
    });
}

fn spawn_animal(commands: &mut Commands, assets: &AnimalAssets, state: &AnimalState) -> Entity {
    let kind = state.kind();
    let i = kind as usize;
    let size = kind.stats().size;
    let (w, h) = (size.half_width * 2.0, size.height);
    let part = |mat: &Handle<StandardMaterial>, scale: Vec3, at: Vec3| {
        (
            Mesh3d(assets.cube.clone()),
            MeshMaterial3d(mat.clone()),
            Transform::from_translation(at).with_scale(scale),
        )
    };
    let leg_h = h * 0.4;
    commands
        .spawn((
            AnimalView {
                id: state.id,
                kind,
                target: state.position(),
                target_yaw: state.yaw(),
                flags: state.flags,
                phase: state.id as f32,
            },
            Transform::from_translation(state.position()).with_rotation(Quat::from_rotation_y(state.yaw())),
            Visibility::default(),
        ))
        .with_children(|a| {
            // Corps allongé vers -Z, tête devant.
            let body_h = h * if kind == AnimalKind::Deer { 0.35 } else { 0.5 };
            a.spawn(part(
                &assets.fur[i],
                Vec3::new(w * 0.8, body_h, w * 1.6),
                Vec3::new(0.0, leg_h + body_h * 0.5, 0.0),
            ));
            let head = Vec3::new(0.0, leg_h + body_h * 1.1, -w * 0.9);
            a.spawn(part(&assets.fur[i], Vec3::splat(w * 0.5), head));
            match kind {
                AnimalKind::Rabbit => {
                    for side in [-1.0, 1.0] {
                        a.spawn(part(
                            &assets.fur[i],
                            Vec3::new(w * 0.1, w * 0.5, w * 0.1),
                            head + Vec3::new(side * w * 0.12, w * 0.45, w * 0.05),
                        ));
                    }
                    a.spawn(part(
                        &assets.detail[i],
                        Vec3::splat(w * 0.25),
                        Vec3::new(0.0, leg_h + body_h * 0.7, w * 0.8),
                    ));
                }
                AnimalKind::Deer => {
                    for side in [-1.0, 1.0] {
                        a.spawn(part(
                            &assets.detail[i],
                            Vec3::new(w * 0.06, w * 0.6, w * 0.06),
                            head + Vec3::new(side * w * 0.18, w * 0.5, 0.0),
                        ));
                    }
                }
                AnimalKind::Boar => {
                    for side in [-1.0, 1.0] {
                        a.spawn(part(
                            &assets.detail[i],
                            Vec3::new(w * 0.06, w * 0.06, w * 0.2),
                            head + Vec3::new(side * w * 0.2, -w * 0.12, -w * 0.3),
                        ));
                    }
                }
            }
            for (x, z) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                a.spawn((
                    Leg(x * z),
                    Transform::from_xyz(x * w * 0.28, leg_h, z * w * 0.6),
                    Visibility::default(),
                ))
                .with_children(|leg| {
                    leg.spawn(part(
                        &assets.fur[i],
                        Vec3::new(w * 0.18, leg_h, w * 0.18),
                        Vec3::new(0.0, -leg_h * 0.5, 0.0),
                    ));
                });
            }
        })
        .id()
}

fn receive_animals(
    mut commands: Commands,
    mut events: MessageReader<NetEvent>,
    mut registry: ResMut<AnimalRegistry>,
    mut views: Query<&mut AnimalView>,
    assets: Res<AnimalAssets>,
) {
    for event in events.read() {
        let NetEvent::Animals(states) = event else { continue };
        // Chaque envoi liste tous les animaux visibles : les absents ont disparu.
        let seen: Vec<u32> = states.iter().map(|s| s.id).collect();
        registry.0.retain(|id, entity| {
            let keep = seen.contains(id);
            if !keep {
                commands.entity(*entity).despawn();
            }
            keep
        });
        for state in states {
            match registry.0.get(&state.id).and_then(|e| views.get_mut(*e).ok()) {
                Some(mut view) => {
                    view.target = state.position();
                    view.target_yaw = state.yaw();
                    view.flags = state.flags;
                }
                None => {
                    let entity = spawn_animal(&mut commands, &assets, state);
                    registry.0.insert(state.id, entity);
                }
            }
        }
    }
}

fn animate_animals(
    time: Res<Time>,
    mut animals: Query<(&mut AnimalView, &mut Transform, &Children), Without<Leg>>,
    mut legs: Query<(&Leg, &mut Transform), Without<AnimalView>>,
) {
    let dt = time.delta_secs();
    let t = (dt * 10.0).min(1.0);
    for (mut view, mut transform, kids) in &mut animals {
        let moved = transform.translation.distance(view.target);
        transform.translation = transform.translation.lerp(view.target, t);
        transform.rotation = transform.rotation.slerp(Quat::from_rotation_y(view.target_yaw), t);
        let running = view.flags & AnimalState::RUNNING != 0;
        view.phase += (moved / dt.max(1e-3)).min(8.0) * dt * if running { 4.0 } else { 3.0 };
        // Touché : il sursaute.
        let hop = if view.flags & AnimalState::HURT != 0 { 0.08 } else { 0.0 };
        transform.translation.y += hop;
        for kid in kids.iter() {
            if let Ok((leg, mut leg_transform)) = legs.get_mut(kid) {
                leg_transform.rotation =
                    Quat::from_rotation_x((view.phase + leg.0).sin() * 0.5 * (moved > 0.001) as u8 as f32);
            }
        }
    }
}

fn clear_animals(mut commands: Commands, mut registry: ResMut<AnimalRegistry>) {
    for entity in registry.0.values() {
        commands.entity(*entity).despawn();
    }
    registry.0.clear();
}

/// Animal visé à moins de `reach`, et sa distance.
pub fn aimed_animal(
    eye: Vec3,
    dir: Vec3,
    reach: f32,
    animals: &Query<(&AnimalView, &Transform)>,
) -> Option<(u32, f32)> {
    animals
        .iter()
        .filter_map(|(view, transform)| {
            let size = view.kind.stats().size;
            let half = Vec3::new(size.half_width + 0.1, 0.0, size.half_width + 0.1);
            let min = transform.translation - half;
            let max = transform.translation + half + Vec3::Y * size.height;
            ray_aabb(eye, dir, min, max)
                .filter(|t| *t <= reach)
                .map(|t| (view.id, t))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
}
