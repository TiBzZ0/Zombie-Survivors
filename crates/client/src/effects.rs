//! Effets visuels : trajectoires des tirs, fissures des blocs attaqués, vie des zombies.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::AppState;
use crate::net::NetEvent;
use crate::zombies::ZombieView;

pub struct EffectsPlugin;

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Tracers>()
            .init_resource::<Cracks>()
            .add_systems(OnExit(AppState::Playing), |mut cracks: ResMut<Cracks>| cracks.0.clear())
            .add_systems(
                Update,
                (collect_shots, draw_tracers, draw_cracks, draw_zombie_health)
                    .chain()
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

struct Tracer {
    origin: Vec3,
    end: Vec3,
    rifle: bool,
    age: f32,
}

#[derive(Resource, Default)]
struct Tracers(Vec<Tracer>);

/// Dégâts subis par les blocs attaqués par les zombies (0 à 255).
#[derive(Resource, Default)]
struct Cracks(HashMap<IVec3, u8>);

fn collect_shots(mut events: MessageReader<NetEvent>, mut tracers: ResMut<Tracers>, mut cracks: ResMut<Cracks>) {
    for event in events.read() {
        match event {
            NetEvent::Shot { origin, end, rifle } => tracers.0.push(Tracer {
                // Part un peu sous l'œil, comme depuis l'arme.
                origin: *origin - Vec3::Y * 0.25,
                end: *end,
                rifle: *rifle,
                age: 0.0,
            }),
            NetEvent::BlockDamage { pos, damage } => {
                cracks.0.insert(*pos, *damage);
            }
            NetEvent::BlockChanged(pos) => {
                cracks.0.remove(pos);
            }
            _ => {}
        }
    }
}

/// Contour du bloc qui rougit avec les dégâts, et fissures en croix au-delà de 40 %.
fn draw_cracks(cracks: Res<Cracks>, mut gizmos: Gizmos) {
    for (pos, damage) in &cracks.0 {
        let k = *damage as f32 / 255.0;
        let color = Color::srgba(0.2 + 0.7 * k, 0.15, 0.1, 0.4 + 0.5 * k);
        let center = pos.as_vec3() + Vec3::splat(0.5);
        gizmos.cube(Transform::from_translation(center).with_scale(Vec3::splat(1.01)), color);
        if k > 0.4 {
            let min = pos.as_vec3() - Vec3::splat(0.005);
            let max = min + Vec3::splat(1.01);
            for axis in 0..3 {
                for side in [min[axis], max[axis]] {
                    let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
                    let corner = |a: f32, b: f32| {
                        let mut p = Vec3::ZERO;
                        p[axis] = side;
                        p[u] = a;
                        p[v] = b;
                        p
                    };
                    gizmos.line(corner(min[u], min[v]), corner(max[u], max[v]), color);
                    if k > 0.7 {
                        gizmos.line(corner(min[u], max[v]), corner(max[u], min[v]), color);
                    }
                }
            }
        }
    }
}

/// Barre de vie au-dessus des zombies blessés, tournée vers la caméra.
fn draw_zombie_health(
    camera: Single<&Transform, With<Camera3d>>,
    zombies: Query<(&ZombieView, &Transform)>,
    mut gizmos: Gizmos,
) {
    let right = camera.right().as_vec3();
    for (view, transform) in &zombies {
        if view.health >= 1.0 || view.health <= 0.0 {
            continue;
        }
        let height = view.kind.stats().size.height;
        let top = transform.translation + Vec3::Y * (height + 0.3);
        let half = 0.4;
        let left = top - right * half;
        let filled = left + right * (2.0 * half * view.health);
        gizmos.line(left, top + right * half, Color::srgb(0.25, 0.05, 0.05));
        gizmos.line(left, filled, Color::srgb(0.2, 0.9, 0.2));
    }
}

fn draw_tracers(time: Res<Time>, mut tracers: ResMut<Tracers>, mut gizmos: Gizmos) {
    let dt = time.delta_secs();
    tracers.0.retain_mut(|t| {
        t.age += dt;
        if t.rifle {
            // Balle : un trait jaune très bref.
            let alpha = (1.0 - t.age / 0.12).max(0.0);
            gizmos.line(t.origin, t.end, Color::srgba(1.0, 0.9, 0.4, alpha));
            t.age < 0.12
        } else {
            // Flèche : un segment qui file vers la cible.
            let duration = (t.origin.distance(t.end) / 45.0).max(0.05);
            let k = (t.age / duration).min(1.0);
            let dir = (t.end - t.origin).normalize_or_zero();
            let head = t.origin.lerp(t.end, k);
            gizmos.line(head - dir * 0.8, head, Color::srgb(0.45, 0.3, 0.15));
            t.age < duration + 0.05
        }
    });
}
