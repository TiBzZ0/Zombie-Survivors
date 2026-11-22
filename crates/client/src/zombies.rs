//! Affichage des zombies envoyés par le serveur : modèle simple par type,
//! interpolation, animation de marche et d'attaque.

use std::collections::HashMap;

use bevy::prelude::*;
use shared::protocol::ZombieState;
use shared::zombie::ZombieKind;

use crate::AppState;
use crate::net::NetEvent;

pub struct ZombiesPlugin;

impl Plugin for ZombiesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ZombieRegistry>()
            .add_systems(Startup, setup_assets)
            .add_systems(
                Update,
                (receive_zombies, animate_zombies, expire_zombies)
                    .chain()
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(OnExit(AppState::Playing), clear_zombies);
    }
}

/// Zombies affichés, par identifiant serveur.
#[derive(Resource, Default)]
pub struct ZombieRegistry {
    pub entities: HashMap<u32, Entity>,
}

/// Un zombie affiché.
#[derive(Component)]
pub struct ZombieView {
    pub id: u32,
    pub kind: ZombieKind,
    target: Vec3,
    target_yaw: f32,
    pub flags: u8,
    /// Part de vie restante (0 à 1).
    pub health: f32,
    last_seen: f32,
    walk_phase: f32,
}

#[derive(Component)]
struct Arm(f32);

/// Partie du corps, avec sa couleur normale (pour revenir après le flash rouge).
#[derive(Component)]
struct ZombieBodyPart(Handle<StandardMaterial>);

#[derive(Resource)]
struct ZombieAssets {
    cube: Handle<Mesh>,
    skin: [Handle<StandardMaterial>; 3],
    clothes: [Handle<StandardMaterial>; 3],
    hurt: Handle<StandardMaterial>,
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
    commands.insert_resource(ZombieAssets {
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        skin: [mat(0.38, 0.55, 0.30), mat(0.55, 0.62, 0.36), mat(0.30, 0.40, 0.25)],
        clothes: [mat(0.25, 0.22, 0.35), mat(0.45, 0.20, 0.18), mat(0.18, 0.16, 0.14)],
        hurt: mat(0.9, 0.15, 0.1),
    });
}

/// Hauteur d'un zombie selon son type (le modèle est mis à l'échelle).
fn scale(kind: ZombieKind) -> Vec3 {
    let s = kind.stats().size;
    Vec3::new(s.half_width / 0.3, s.height / 1.8, s.half_width / 0.3)
}

fn spawn_zombie(commands: &mut Commands, assets: &ZombieAssets, state: &ZombieState, now: f32) -> Entity {
    let kind = state.kind();
    let i = kind as usize;
    let part = |mat: &Handle<StandardMaterial>, size: Vec3, at: Vec3| {
        (
            ZombieBodyPart(mat.clone()),
            Mesh3d(assets.cube.clone()),
            MeshMaterial3d(mat.clone()),
            Transform::from_translation(at).with_scale(size),
        )
    };
    commands
        .spawn((
            ZombieView {
                id: state.id,
                kind,
                target: state.position(),
                target_yaw: state.yaw(),
                flags: state.flags,
                health: state.health_fraction(),
                last_seen: now,
                walk_phase: (state.id % 7) as f32,
            },
            Transform::from_translation(state.position())
                .with_rotation(Quat::from_rotation_y(state.yaw()))
                .with_scale(scale(kind)),
            Visibility::default(),
        ))
        .with_children(|z| {
            // Jambes, torse, tête. Le zombie regarde vers -Z.
            z.spawn(part(
                &assets.clothes[i],
                Vec3::new(0.22, 0.8, 0.22),
                Vec3::new(-0.13, 0.4, 0.0),
            ));
            z.spawn(part(
                &assets.clothes[i],
                Vec3::new(0.22, 0.8, 0.22),
                Vec3::new(0.13, 0.4, 0.0),
            ));
            z.spawn(part(
                &assets.clothes[i],
                Vec3::new(0.55, 0.65, 0.3),
                Vec3::new(0.0, 1.12, 0.0),
            ));
            z.spawn(part(
                &assets.skin[i],
                Vec3::new(0.42, 0.42, 0.42),
                Vec3::new(0.0, 1.62, -0.02),
            ));
            // Bras tendus vers l'avant, pivot à l'épaule.
            for side in [-1.0f32, 1.0] {
                z.spawn((
                    Arm(side),
                    Transform::from_xyz(side * 0.36, 1.38, 0.0),
                    Visibility::default(),
                ))
                .with_children(|arm| {
                    arm.spawn(part(
                        &assets.skin[i],
                        Vec3::new(0.16, 0.16, 0.7),
                        Vec3::new(0.0, 0.0, -0.32),
                    ));
                });
            }
        })
        .id()
}

fn receive_zombies(
    mut commands: Commands,
    mut events: MessageReader<NetEvent>,
    mut registry: ResMut<ZombieRegistry>,
    mut views: Query<&mut ZombieView>,
    assets: Res<ZombieAssets>,
    time: Res<Time>,
) {
    let now = time.elapsed_secs();
    for event in events.read() {
        match event {
            NetEvent::Zombies(states) => {
                for state in states {
                    match registry.entities.get(&state.id).and_then(|e| views.get_mut(*e).ok()) {
                        Some(mut view) => {
                            view.target = state.position();
                            view.target_yaw = state.yaw();
                            view.flags = state.flags;
                            view.health = state.health_fraction();
                            view.last_seen = now;
                        }
                        None => {
                            let entity = spawn_zombie(&mut commands, &assets, state, now);
                            registry.entities.insert(state.id, entity);
                        }
                    }
                }
            }
            NetEvent::ZombieKilled(id) => {
                if let Some(entity) = registry.entities.remove(id) {
                    commands.entity(entity).despawn();
                }
            }
            _ => {}
        }
    }
}

fn animate_zombies(
    time: Res<Time>,
    assets: Res<ZombieAssets>,
    mut zombies: Query<(&mut ZombieView, &mut Transform, &Children), Without<Arm>>,
    mut arms: Query<(&Arm, &mut Transform), Without<ZombieView>>,
    mut parts: Query<(&mut MeshMaterial3d<StandardMaterial>, &ZombieBodyPart)>,
    children: Query<&Children>,
) {
    let dt = time.delta_secs();
    let t = (dt * 10.0).min(1.0);
    for (mut view, mut transform, kids) in &mut zombies {
        let moved = transform.translation.distance(view.target);
        transform.translation = transform.translation.lerp(view.target, t);
        transform.rotation = transform.rotation.slerp(Quat::from_rotation_y(view.target_yaw), t);
        let speed = view.kind.stats().speed;
        view.walk_phase += (moved / dt.max(1e-3)).min(speed) * dt * 3.0;
        let attacking = view.flags & ZombieState::ATTACKING != 0;
        let hurt = view.flags & ZombieState::HURT != 0;
        for kid in kids.iter() {
            if let Ok((arm, mut arm_transform)) = arms.get_mut(kid) {
                let swing = if attacking {
                    (time.elapsed_secs() * 14.0 + arm.0).sin() * 0.7 - 0.2
                } else {
                    (view.walk_phase + arm.0).sin() * 0.15
                };
                arm_transform.rotation = Quat::from_rotation_x(swing);
            }
        }
        // Flash rouge quand il est touché.
        for kid in kids.iter() {
            let grandchildren: Vec<Entity> = children.get(kid).map(|c| c.iter().collect()).unwrap_or_default();
            for e in std::iter::once(kid).chain(grandchildren) {
                if let Ok((mut material, part)) = parts.get_mut(e) {
                    let wanted = if hurt { &assets.hurt } else { &part.0 };
                    if material.0 != *wanted {
                        material.0 = wanted.clone();
                    }
                }
            }
        }
    }
}

/// Retire les zombies que le serveur n'envoie plus (morts ou trop loin).
fn expire_zombies(
    mut commands: Commands,
    time: Res<Time>,
    mut registry: ResMut<ZombieRegistry>,
    views: Query<(Entity, &ZombieView)>,
) {
    let now = time.elapsed_secs();
    for (entity, view) in &views {
        if now - view.last_seen > 1.0 {
            registry.entities.remove(&view.id);
            commands.entity(entity).despawn();
        }
    }
}

fn clear_zombies(mut commands: Commands, mut registry: ResMut<ZombieRegistry>) {
    for entity in registry.entities.values() {
        commands.entity(*entity).despawn();
    }
    registry.entities.clear();
}
