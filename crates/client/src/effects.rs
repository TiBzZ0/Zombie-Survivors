//! Effets visuels : trajectoires des tirs, fissures des blocs attaqués, vie des zombies.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::AppState;
use crate::net::{ClientWorld, NetEvent};
use crate::survivors::SurvivorView;
use crate::zombies::ZombieView;
use shared::block::Block;
use shared::item::Item;
use shared::physics::{PROJECTILE_GRAVITY, step_projectile};

pub struct EffectsPlugin;

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Tracers>()
            .init_resource::<Cracks>()
            .init_resource::<Thrown>()
            .add_systems(
                OnExit(AppState::Playing),
                |mut cracks: ResMut<Cracks>, mut thrown: ResMut<Thrown>| {
                    cracks.0.clear();
                    thrown.0.clear();
                },
            )
            .add_systems(
                Update,
                (
                    collect_shots,
                    draw_tracers,
                    draw_thrown,
                    draw_cracks,
                    draw_zombie_health,
                )
                    .chain()
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

struct Tracer {
    origin: Vec3,
    end: Vec3,
    rifle: bool,
    /// Crachat d'acide d'un cracheur.
    acid: bool,
    /// Arc électrique d'une bobine.
    zap: bool,
    age: f32,
}

#[derive(Resource, Default)]
struct Tracers(Vec<Tracer>);

/// Explosif en vol (trajectoire simulée comme sur le serveur) ou explosion qui s'étend.
enum Blast {
    Flying {
        pos: Vec3,
        vel: Vec3,
        fuse: f32,
        item: Item,
    },
    Boom {
        pos: Vec3,
        radius: f32,
        age: f32,
    },
}

#[derive(Resource, Default)]
struct Thrown(Vec<Blast>);

/// Dégâts subis par les blocs attaqués par les zombies (0 à 255).
#[derive(Resource, Default)]
struct Cracks(HashMap<IVec3, u8>);

fn collect_shots(
    mut events: MessageReader<NetEvent>,
    mut tracers: ResMut<Tracers>,
    mut cracks: ResMut<Cracks>,
    mut thrown: ResMut<Thrown>,
) {
    for event in events.read() {
        match event {
            NetEvent::Thrown {
                origin,
                velocity,
                fuse,
                item,
            } => thrown.0.push(Blast::Flying {
                pos: *origin,
                vel: *velocity,
                fuse: *fuse,
                item: *item,
            }),
            NetEvent::Explosion { pos, radius } => {
                // L'explosion remplace l'explosif le plus proche encore affiché.
                let nearest = thrown
                    .0
                    .iter()
                    .enumerate()
                    .filter_map(|(i, b)| match b {
                        Blast::Flying { pos: p, .. } => Some((i, p.distance(*pos))),
                        Blast::Boom { .. } => None,
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                if let Some((i, _)) = nearest {
                    thrown.0.swap_remove(i);
                }
                thrown.0.push(Blast::Boom {
                    pos: *pos,
                    radius: *radius,
                    age: 0.0,
                });
            }
            NetEvent::Shot { origin, end, rifle } => tracers.0.push(Tracer {
                // Part un peu sous l'œil, comme depuis l'arme.
                origin: *origin - Vec3::Y * 0.25,
                end: *end,
                rifle: *rifle,
                acid: false,
                zap: false,
                age: 0.0,
            }),
            NetEvent::Acid { origin, end } => tracers.0.push(Tracer {
                origin: *origin,
                end: *end,
                rifle: false,
                acid: true,
                zap: false,
                age: 0.0,
            }),
            NetEvent::Zap { origin, end } => tracers.0.push(Tracer {
                origin: *origin,
                end: *end,
                rifle: false,
                acid: false,
                zap: true,
                age: 0.0,
            }),
            NetEvent::BlockDamage { pos, damage: 0 } => {
                cracks.0.remove(pos);
            }
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
    survivors: Query<(&SurvivorView, &Transform)>,
    mut gizmos: Gizmos,
) {
    let right = camera.right().as_vec3();
    let bars = zombies
        .iter()
        .map(|(v, t)| {
            (
                v.health,
                t.translation + Vec3::Y * (v.kind.stats().size.height + 0.3),
                false,
            )
        })
        .chain(survivors.iter().map(|(v, t)| {
            let height = shared::survivor::SURVIVOR_SIZE.height;
            (v.health, t.translation + Vec3::Y * (height + 0.2), true)
        }));
    for (health, top, friend) in bars {
        if health >= 1.0 || health <= 0.0 {
            continue;
        }
        let half = 0.4;
        let left = top - right * half;
        let filled = left + right * (2.0 * half * health);
        let color = if friend {
            Color::srgb(0.3, 0.6, 1.0)
        } else {
            Color::srgb(0.2, 0.9, 0.2)
        };
        gizmos.line(left, top + right * half, Color::srgb(0.25, 0.05, 0.05));
        gizmos.line(left, filled, color);
    }
}

fn draw_tracers(time: Res<Time>, mut tracers: ResMut<Tracers>, mut gizmos: Gizmos) {
    let dt = time.delta_secs();
    tracers.0.retain_mut(|t| {
        t.age += dt;
        if t.zap {
            // Éclair : une ligne brisée bleutée qui change de forme à chaque image.
            let alpha = (1.0 - t.age / 0.2).max(0.0);
            let color = Color::srgba(0.6, 0.8, 1.0, alpha);
            let mut last = t.origin;
            for i in 1..=6 {
                let k = i as f32 / 6.0;
                let jitter = if i < 6 {
                    Vec3::new(fastrand::f32() - 0.5, fastrand::f32() - 0.5, fastrand::f32() - 0.5) * 0.6
                } else {
                    Vec3::ZERO
                };
                let next = t.origin.lerp(t.end, k) + jitter;
                gizmos.line(last, next, color);
                last = next;
            }
            t.age < 0.2
        } else if t.acid {
            // Acide : une boule verte qui file en cloche.
            let duration = (t.origin.distance(t.end) / 18.0).max(0.05);
            let k = (t.age / duration).min(1.0);
            let arc = Vec3::Y * (k * (1.0 - k) * 2.0);
            let head = t.origin.lerp(t.end, k) + arc;
            gizmos.sphere(Isometry3d::from_translation(head), 0.15, Color::srgb(0.5, 0.95, 0.15));
            t.age < duration + 0.05
        } else if t.rifle {
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

/// Gravité d'un projectile d'arme fixe (qui explose à l'impact), d'après sa munition.
fn shell_gravity(item: Item) -> Option<f32> {
    [Block::Cannon, Block::Catapult]
        .into_iter()
        .filter_map(Block::emplacement)
        .find(|g| g.ammo == item)
        .map(|g| g.gravity)
}

fn draw_thrown(time: Res<Time>, world: Res<ClientWorld>, mut thrown: ResMut<Thrown>, mut gizmos: Gizmos) {
    let dt = time.delta_secs().min(0.05);
    let solid = |p: IVec3| world.is_solid(p);
    thrown.0.retain_mut(|blast| match blast {
        Blast::Flying { pos, vel, fuse, item } => {
            // Boulet et pierre volent droit jusqu'à l'impact ; grenade et dynamite roulent.
            let shell = shell_gravity(*item);
            if let Some(gravity) = shell {
                vel.y -= PROJECTILE_GRAVITY * gravity * dt;
                let next = *pos + *vel * dt;
                if solid(next.floor().as_ivec3()) {
                    *vel = Vec3::ZERO;
                } else {
                    *pos = next;
                }
            } else {
                (*pos, *vel) = step_projectile(*pos, *vel, dt, &solid);
            }
            *fuse -= dt;
            let (body, size) = match item {
                Item::Dynamite => (Color::srgb(0.75, 0.15, 0.1), 0.18),
                Item::CannonBall => (Color::srgb(0.12, 0.12, 0.14), 0.22),
                Item::Block(_) => (Color::srgb(0.55, 0.55, 0.55), 0.3),
                _ => (Color::srgb(0.25, 0.35, 0.2), 0.14),
            };
            gizmos.sphere(Isometry3d::from_translation(*pos), size, body);
            if shell.is_none() {
                // La mèche clignote.
                let blink = ((*fuse * 8.0).sin() > 0.0) as u8 as f32;
                gizmos.sphere(
                    Isometry3d::from_translation(*pos + Vec3::Y * size),
                    0.04,
                    Color::srgba(1.0, 0.8, 0.2, blink),
                );
            }
            // Filet de sécurité si le message d'explosion se perd.
            *fuse > -1.0
        }
        Blast::Boom { pos, radius, age } => {
            *age += dt;
            let k = (*age / 0.45).min(1.0);
            let alpha = 1.0 - k;
            gizmos.sphere(
                Isometry3d::from_translation(*pos),
                *radius * (0.3 + 0.7 * k),
                Color::srgba(1.0, 0.55, 0.1, alpha),
            );
            gizmos.sphere(
                Isometry3d::from_translation(*pos),
                *radius * 0.5 * (0.3 + 0.7 * k),
                Color::srgba(1.0, 0.9, 0.5, alpha),
            );
            *age < 0.45
        }
    });
}
