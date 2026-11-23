//! Pluie et orage à l'écran : gouttes qui tombent autour de la caméra (pas sous un toit)
//! et éclairs qui zèbrent le ciel.

use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use crate::AppState;
use crate::net::{ClientWorld, NetEvent};
use crate::sky::GameClock;

pub struct RainPlugin;

impl Plugin for RainPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Playing), spawn_drops)
            .add_systems(Update, (fall, strike, fade_bolts).run_if(in_state(AppState::Playing)));
    }
}

/// Gouttes en jeu, au plus fort de l'orage.
const DROPS: usize = 420;
/// Demi-côté de la boîte de pluie autour de la caméra, et sa hauteur.
const SPREAD: f32 = 14.0;
const HEIGHT: f32 = 16.0;
const DROP_SPEED: f32 = 22.0;

#[derive(Component)]
struct Drop(usize);

#[derive(Component)]
struct Bolt(f32);

#[derive(Resource)]
struct BoltAssets {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
}

fn spawn_drops(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh = meshes.add(Cuboid::new(0.025, 0.55, 0.025));
    let material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.7, 0.78, 0.9, 0.45),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    for i in 0..DROPS {
        commands.spawn((
            DespawnOnExit(AppState::Playing),
            Drop(i),
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            Transform::default(),
            Visibility::Hidden,
            NotShadowCaster,
        ));
    }
    commands.insert_resource(BoltAssets {
        mesh: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        material: materials.add(StandardMaterial {
            base_color: Color::srgb(0.85, 0.9, 1.0),
            emissive: LinearRgba::rgb(6.0, 7.0, 10.0),
            unlit: true,
            ..default()
        }),
    });
}

/// Pseudo-hasard stable par goutte, pour les répartir dans la boîte.
fn hash(i: usize, salt: u32) -> f32 {
    let mut h = (i as u32).wrapping_mul(0x9E37_79B9) ^ salt.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h & 0xFFFF) as f32 / 65535.0
}

fn fall(
    time: Res<Time>,
    clock: Res<GameClock>,
    world: Res<ClientWorld>,
    camera: Single<&GlobalTransform, With<Camera3d>>,
    mut drops: Query<(&Drop, &mut Transform, &mut Visibility)>,
) {
    let eye = camera.translation();
    let shown = (clock.rain_level * DROPS as f32) as usize;
    // Sous un toit, pas de pluie à l'écran.
    let column = eye.floor().as_ivec3();
    let sheltered = (1..=12).any(|dy| world.is_solid(column + IVec3::Y * dy));
    let t = time.elapsed_secs();
    for (drop, mut transform, mut visibility) in &mut drops {
        if drop.0 >= shown || sheltered {
            *visibility = Visibility::Hidden;
            continue;
        }
        // Chaque goutte tombe en boucle dans sa colonne, qui suit la caméra par pas d'un mètre
        // (pour que la pluie ne glisse pas avec le regard).
        let x = (hash(drop.0, 1) - 0.5) * 2.0 * SPREAD;
        let z = (hash(drop.0, 2) - 0.5) * 2.0 * SPREAD;
        let phase = hash(drop.0, 3) * HEIGHT;
        let y = HEIGHT * 0.6 - (t * DROP_SPEED + phase).rem_euclid(HEIGHT);
        let at = Vec3::new(eye.x.floor() + x, eye.y + y, eye.z.floor() + z);
        // L'orage penche la pluie.
        let lean = if clock.rain_level > 0.7 { 0.25 } else { 0.08 };
        *transform = Transform::from_translation(at).with_rotation(Quat::from_rotation_z(lean));
        *visibility = if world.is_solid(at.floor().as_ivec3()) {
            Visibility::Hidden
        } else {
            Visibility::Visible
        };
    }
}

/// Un éclair : une ligne brisée du ciel jusqu'au point frappé, qui ne dure qu'un instant.
fn strike(mut commands: Commands, mut events: MessageReader<NetEvent>, assets: Option<Res<BoltAssets>>) {
    let Some(assets) = assets else { return };
    for event in events.read() {
        let NetEvent::Lightning { pos } = event else {
            continue;
        };
        let mut at = *pos;
        let mut height = 0.0;
        let mut i = 0;
        while height < 70.0 {
            let step = 4.0 + hash(i, pos.x.to_bits()) * 5.0;
            let next = at + Vec3::new((hash(i, 7) - 0.5) * 4.0, step, (hash(i, 11) - 0.5) * 4.0);
            let middle = (at + next) * 0.5;
            let length = at.distance(next);
            commands.spawn((
                DespawnOnExit(AppState::Playing),
                Bolt(0.3),
                Mesh3d(assets.mesh.clone()),
                MeshMaterial3d(assets.material.clone()),
                Transform::from_translation(middle)
                    .looking_at(next, Vec3::X)
                    .with_scale(Vec3::new(0.25, 0.25, length)),
                NotShadowCaster,
            ));
            at = next;
            height += step;
            i += 1;
        }
    }
}

fn fade_bolts(mut commands: Commands, time: Res<Time>, mut bolts: Query<(Entity, &mut Bolt)>) {
    for (entity, mut bolt) in &mut bolts {
        bolt.0 -= time.delta_secs();
        if bolt.0 <= 0.0 {
            commands.entity(entity).despawn();
        }
    }
}
