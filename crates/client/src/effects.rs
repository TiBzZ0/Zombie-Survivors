//! Effets visuels éphémères : trajectoires des tirs.

use bevy::prelude::*;

use crate::AppState;
use crate::net::NetEvent;

pub struct EffectsPlugin;

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Tracers>().add_systems(
            Update,
            (collect_shots, draw_tracers)
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

fn collect_shots(mut events: MessageReader<NetEvent>, mut tracers: ResMut<Tracers>) {
    for event in events.read() {
        if let NetEvent::Shot { origin, end, rifle } = event {
            tracers.0.push(Tracer {
                // Part un peu sous l'œil, comme depuis l'arme.
                origin: *origin - Vec3::Y * 0.25,
                end: *end,
                rifle: *rifle,
                age: 0.0,
            });
        }
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
