//! Éclats des blocs cassés et feuilles qui tombent des arbres en tournoyant.

use std::collections::HashMap;

use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use shared::block::Block;

use crate::AppState;
use crate::net::{ClientWorld, NetEvent};

pub struct ParticlesPlugin;

impl Plugin for ParticlesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ParticleMaterials>()
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                (burst, falling_leaves, animate).run_if(in_state(AppState::Playing)),
            )
            .add_systems(OnExit(AppState::Playing), clear);
    }
}

/// Au-delà de cette distance, on ne dessine pas les éclats.
const MAX_DISTANCE: f32 = 48.0;
/// Rayon autour du joueur où des feuilles se détachent des arbres.
const LEAF_RADIUS: i32 = 12;

#[derive(Component)]
struct Particle {
    velocity: Vec3,
    spin: Vec3,
    age: f32,
    life: f32,
    /// Une feuille plane et se balance ; un éclat tombe comme une pierre.
    leaf: bool,
    size: f32,
}

#[derive(Resource)]
struct ParticleMeshes {
    chip: Handle<Mesh>,
    leaf: Handle<Mesh>,
}

#[derive(Resource, Default)]
struct ParticleMaterials(HashMap<Block, Handle<StandardMaterial>>);

impl ParticleMaterials {
    fn get(&mut self, block: Block, materials: &mut Assets<StandardMaterial>) -> Handle<StandardMaterial> {
        self.0
            .entry(block)
            .or_insert_with(|| {
                let [r, g, b] = block.color();
                materials.add(StandardMaterial {
                    base_color: Color::srgb(r, g, b),
                    perceptual_roughness: 1.0,
                    double_sided: true,
                    cull_mode: None,
                    ..default()
                })
            })
            .clone()
    }
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(ParticleMeshes {
        chip: meshes.add(Cuboid::from_length(1.0)),
        leaf: meshes.add(Rectangle::new(1.0, 1.0)),
    });
}

fn clear(mut commands: Commands, particles: Query<Entity, With<Particle>>) {
    for entity in &particles {
        commands.entity(entity).despawn();
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn(
    commands: &mut Commands,
    meshes: &ParticleMeshes,
    cache: &mut ParticleMaterials,
    materials: &mut Assets<StandardMaterial>,
    block: Block,
    at: Vec3,
    velocity: Vec3,
    leaf: bool,
) {
    let size = if leaf { 0.14 } else { 0.08 + fastrand::f32() * 0.06 };
    let life = if leaf {
        3.0 + fastrand::f32() * 2.0
    } else {
        0.6 + fastrand::f32() * 0.5
    };
    let rand = || Vec3::new(fastrand::f32() - 0.5, fastrand::f32() - 0.5, fastrand::f32() - 0.5);
    commands.spawn((
        Particle {
            velocity,
            spin: rand() * if leaf { 3.0 } else { 12.0 },
            age: 0.0,
            life,
            leaf,
            size,
        },
        Mesh3d(if leaf { meshes.leaf.clone() } else { meshes.chip.clone() }),
        MeshMaterial3d(cache.get(block, materials)),
        Transform::from_translation(at)
            .with_rotation(Quat::from_euler(
                EulerRot::XYZ,
                rand().x * 6.0,
                rand().y * 6.0,
                rand().z * 6.0,
            ))
            .with_scale(Vec3::splat(size)),
        NotShadowCaster,
    ));
}

/// Un bloc cassé vole en éclats ; un feuillage lâche quelques feuilles.
fn burst(
    mut commands: Commands,
    mut events: MessageReader<NetEvent>,
    meshes: Res<ParticleMeshes>,
    mut cache: ResMut<ParticleMaterials>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    camera: Single<&Transform, With<Camera3d>>,
) {
    for event in events.read() {
        let NetEvent::BlockBroken { pos, block } = event else {
            continue;
        };
        let center = pos.as_vec3() + Vec3::splat(0.5);
        if center.distance(camera.translation) > MAX_DISTANCE || *block == Block::Air || block.is_water() {
            continue;
        }
        let leaf = block.is_leaves();
        let count = if leaf { 6 } else { 8 };
        for _ in 0..count {
            let dir = Vec3::new(fastrand::f32() - 0.5, fastrand::f32(), fastrand::f32() - 0.5);
            let (at, velocity) = if leaf {
                (center + (dir - Vec3::Y * 0.5) * 0.8, Vec3::new(dir.x, -0.3, dir.z))
            } else {
                (
                    center + (dir - Vec3::Y * 0.3) * 0.6,
                    dir * Vec3::new(4.0, 4.0, 4.0) + Vec3::Y,
                )
            };
            spawn(
                &mut commands,
                &meshes,
                &mut cache,
                &mut materials,
                *block,
                at,
                velocity,
                leaf,
            );
        }
    }
}

/// De temps en temps, une feuille se détache d'un arbre proche.
fn falling_leaves(
    mut commands: Commands,
    time: Res<Time>,
    world: Option<Res<ClientWorld>>,
    meshes: Res<ParticleMeshes>,
    mut cache: ResMut<ParticleMaterials>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    camera: Single<&Transform, With<Camera3d>>,
    mut timer: Local<f32>,
) {
    let Some(world) = world else { return };
    *timer += time.delta_secs();
    let tries = (*timer / 0.05) as u32;
    *timer -= tries as f32 * 0.05;
    let eye = camera.translation.as_ivec3();
    for _ in 0..tries {
        let r = LEAF_RADIUS;
        let pos = eye + IVec3::new(fastrand::i32(-r..=r), fastrand::i32(-3..=8), fastrand::i32(-r..=r));
        let block = world.block(pos);
        if block.is_leaves() && world.block(pos - IVec3::Y) == Block::Air && fastrand::f32() < 0.15 {
            let at = pos.as_vec3() + Vec3::new(fastrand::f32(), -0.05, fastrand::f32());
            let drift = Vec3::new(fastrand::f32() - 0.5, -0.25, fastrand::f32() - 0.5);
            spawn(
                &mut commands,
                &meshes,
                &mut cache,
                &mut materials,
                block,
                at,
                drift,
                true,
            );
        }
    }
}

fn animate(
    mut commands: Commands,
    time: Res<Time>,
    world: Option<Res<ClientWorld>>,
    mut particles: Query<(Entity, &mut Particle, &mut Transform)>,
) {
    let dt = time.delta_secs();
    let t = time.elapsed_secs();
    for (entity, mut p, mut transform) in &mut particles {
        p.age += dt;
        if p.age >= p.life {
            commands.entity(entity).despawn();
            continue;
        }
        if p.leaf {
            // Chute lente, balancée de droite à gauche.
            p.velocity.y = (p.velocity.y - dt * 0.6).max(-0.9);
            let sway = Vec3::new((t * 2.3 + p.life * 7.0).sin(), 0.0, (t * 1.9 + p.life * 5.0).cos()) * 0.6;
            transform.translation += (p.velocity + sway) * dt;
        } else {
            p.velocity.y -= 18.0 * dt;
            transform.translation += p.velocity * dt;
        }
        // Posé au sol : il ne bouge plus.
        if let Some(world) = &world
            && world.block(transform.translation.floor().as_ivec3()).is_solid()
        {
            transform.translation -= p.velocity * dt;
            p.velocity = Vec3::ZERO;
            p.spin = Vec3::ZERO;
        }
        let spin = p.spin * dt;
        transform.rotate(Quat::from_euler(EulerRot::XYZ, spin.x, spin.y, spin.z));
        let fade = ((p.life - p.age) / 0.4).clamp(0.0, 1.0);
        transform.scale = Vec3::splat(p.size * fade);
    }
}
