//! Cycle jour/nuit côté client : soleil, couleur du ciel, lumière ambiante.

use bevy::prelude::*;
use shared::daytime::GameTime;

use crate::AppState;
use crate::net::NetEvent;

pub struct SkyPlugin;

impl Plugin for SkyPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(GameClock {
            time: GameTime::START,
            zombies_alive: 0,
            fog: false,
            fog_level: 0.0,
        })
        .add_systems(
            Update,
            (sync_clock, update_sky).chain().run_if(in_state(AppState::Playing)),
        );
    }
}

/// Heure de la partie, recalée sur le serveur chaque seconde et avancée localement entre deux.
#[derive(Resource, Debug, Clone, Copy)]
pub struct GameClock {
    pub time: GameTime,
    pub zombies_alive: u32,
    /// Brouillard annoncé par le serveur.
    pub fog: bool,
    /// Épaisseur du brouillard affichée, de 0 à 1 (monte et descend en douceur).
    pub fog_level: f32,
}

const DAY_SKY: Vec3 = Vec3::new(0.53, 0.72, 0.90);
const DUSK_SKY: Vec3 = Vec3::new(0.85, 0.45, 0.30);
const NIGHT_SKY: Vec3 = Vec3::new(0.02, 0.03, 0.07);
const FOG_SKY: Vec3 = Vec3::new(0.68, 0.70, 0.72);

fn sync_clock(mut events: MessageReader<NetEvent>, mut clock: ResMut<GameClock>, time: Res<Time>) {
    clock.time.advance(time.delta_secs());
    let target = if clock.fog { 1.0 } else { 0.0 };
    let step = time.delta_secs() / 8.0;
    clock.fog_level += (target - clock.fog_level).clamp(-step, step);
    for event in events.read() {
        if let NetEvent::Weather { fog } = event {
            clock.fog = *fog;
        }
        if let NetEvent::Time { time, zombies_alive } = event {
            clock.time = *time;
            clock.zombies_alive = *zombies_alive;
        }
    }
}

fn update_sky(
    clock: Res<GameClock>,
    mut clear: ResMut<ClearColor>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut sun: Single<(&mut DirectionalLight, &mut Transform)>,
    mut fog: Single<&mut DistanceFog>,
    settings: Res<crate::settings::Settings>,
) {
    // Le brouillard cache le bord du terrain chargé, quelle que soit la distance d'affichage.
    let end = settings.view_distance as f32 * 32.0 + 26.0;
    // Jour de brouillard : on n'y voit plus qu'à une trentaine de mètres.
    let thick = clock.fog_level;
    let end = end + (32.0 - end) * thick;
    fog.falloff = FogFalloff::Linear {
        start: end * (0.5 - 0.4 * thick),
        end,
    };
    let h = clock.time.sun_height();
    let day = h.clamp(0.0, 1.0);
    // Teinte orangée quand le soleil est bas.
    let low = (1.0 - (h.abs() * 4.0).min(1.0)) * if h > -0.25 { 1.0 } else { 0.0 };
    let base = NIGHT_SKY.lerp(DAY_SKY, (h * 2.0 + 0.3).clamp(0.0, 1.0));
    let sky = base.lerp(DUSK_SKY, low * 0.6);
    let sky = sky.lerp(FOG_SKY * (0.25 + 0.75 * day), thick * 0.85);
    let color = Color::srgb(sky.x, sky.y, sky.z);
    clear.0 = color;
    fog.color = color;

    let (light, transform) = &mut *sun;
    light.illuminance = (300.0 + 9_000.0 * day) * (1.0 - 0.5 * thick);
    // Le soleil tourne autour de l'axe X ; la nuit, la « lune » éclaire faiblement.
    let angle = clock.time.phase * std::f32::consts::TAU;
    let dir = Vec3::new(-0.3, -angle.sin().abs().max(0.2), -angle.cos());
    **transform = Transform::default().looking_to(dir, Vec3::Y);
    ambient.brightness = 120.0 + 500.0 * day;
}
