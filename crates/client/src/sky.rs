//! Cycle jour/nuit côté client : soleil et lune qui traversent le ciel, étoiles, couleur du ciel,
//! lumière (et ombres) venant de l'astre visible, lumière ambiante.

use bevy::asset::RenderAssetUsages;
use bevy::image::Image;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::transform::TransformSystems;
use shared::daytime::{DAYLIGHT_FRACTION, GameTime};

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
        .add_systems(Startup, spawn_sky)
        .add_systems(
            Update,
            (sync_clock, update_sky).chain().run_if(in_state(AppState::Playing)),
        )
        // Après le déplacement de la caméra, pour que les astres ne tremblent pas.
        .add_systems(PostUpdate, place_sky.before(TransformSystems::Propagate));
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

/// Distance des astres à la caméra : derrière le terrain, devant le plan lointain.
const SKY_DISTANCE: f32 = 800.0;
const STAR_DISTANCE: f32 = 850.0;
/// Taille apparente (côté du carré, halo compris) du soleil et de la lune.
const SUN_SIZE: f32 = 150.0;
const MOON_SIZE: f32 = 90.0;
const STAR_COUNT: usize = 600;

/// Lumière directe du soleil en plein midi, et de la pleine lune.
const SUN_LUX: f32 = 10_000.0;
const MOON_LUX: f32 = 450.0;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum SkyBody {
    Sun,
    Moon,
}

#[derive(Component)]
struct Stars;

/// Où sont les astres et comment ils éclairent, à un instant de la journée.
#[derive(Debug, Clone, Copy)]
struct Sky {
    /// Direction (depuis le joueur) du soleil et de la lune.
    sun: Vec3,
    moon: Vec3,
    /// 0 en plein jour, 1 en pleine nuit.
    night: f32,
}

impl Sky {
    fn at(time: GameTime) -> Self {
        // Le soleil se lève à l'est (+X) à l'aube, culmine puis se couche à l'ouest au crépuscule ;
        // la nuit, il fait le même chemin sous l'horizon pendant que la lune passe au-dessus.
        let angle = if time.is_night() {
            let t = (time.phase - DAYLIGHT_FRACTION) / (1.0 - DAYLIGHT_FRACTION);
            std::f32::consts::PI * (1.0 + t)
        } else {
            std::f32::consts::PI * time.phase / DAYLIGHT_FRACTION
        };
        // Un peu incliné vers le sud, comme sous nos latitudes.
        let sun = Vec3::new(angle.cos(), angle.sin(), 0.35).normalize();
        let night = (1.0 - (time.sun_height() * 3.0 + 0.5)).clamp(0.0, 1.0);
        Self { sun, moon: -sun, night }
    }

    /// Astre qui éclaire la scène et direction de sa lumière (jamais rasante, pour garder des ombres
    /// lisibles), puis sa force de 0 à 1.
    fn light(&self) -> (SkyBody, Vec3, f32) {
        let (body, dir) = if self.sun.y >= 0.0 {
            (SkyBody::Sun, self.sun)
        } else {
            (SkyBody::Moon, self.moon)
        };
        // La lumière faiblit près de l'horizon, et s'éteint juste avant que l'astre ne disparaisse.
        let strength = (dir.y * 4.0).clamp(0.0, 1.0);
        let lifted = Vec3::new(dir.x, dir.y.max(0.35), dir.z).normalize();
        (body, -lifted, strength)
    }
}

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

#[allow(clippy::too_many_arguments)]
fn update_sky(
    clock: Res<GameClock>,
    mut clear: ResMut<ClearColor>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut light: Single<(&mut DirectionalLight, &mut Transform)>,
    mut fog: Single<&mut DistanceFog>,
    bodies: Query<(&SkyBody, &MeshMaterial3d<StandardMaterial>)>,
    stars: Query<&MeshMaterial3d<StandardMaterial>, With<Stars>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
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
    let sky_color = base.lerp(DUSK_SKY, low * 0.6);
    let sky_color = sky_color.lerp(FOG_SKY * (0.25 + 0.75 * day), thick * 0.85);
    let color = Color::srgb(sky_color.x, sky_color.y, sky_color.z);
    clear.0 = color;
    fog.color = color;

    let sky = Sky::at(clock.time);
    let (body, toward, strength) = sky.light();
    let (light, transform) = &mut *light;
    // Le brouillard étouffe la lumière directe : les ombres s'estompent.
    let veil = 1.0 - 0.7 * thick;
    light.illuminance = match body {
        SkyBody::Sun => SUN_LUX * strength * veil,
        SkyBody::Moon => MOON_LUX * strength * veil,
    };
    light.color = match body {
        // Soleil blanc à midi, doré puis orangé quand il descend.
        SkyBody::Sun => {
            let warm = 1.0 - (sky.sun.y * 2.5).clamp(0.0, 1.0);
            Color::srgb(1.0, 0.95 - 0.3 * warm, 0.88 - 0.5 * warm)
        }
        // Clair de lune bleuté.
        SkyBody::Moon => Color::srgb(0.62, 0.72, 1.0),
    };
    light.shadow_maps_enabled = settings.shadows && strength > 0.0;
    **transform = Transform::default().looking_to(toward, Vec3::Y);
    // Sans lumière directe, l'ambiante seule dessine les reliefs : un peu plus forte la nuit
    // qu'avant, pour qu'on y voie quand même.
    ambient.brightness = 220.0 + 380.0 * day;
    ambient.color = Color::srgb(0.85 + 0.15 * day, 0.88 + 0.12 * day, 1.0);

    // Astres et étoiles pâlissent dans le brouillard.
    for (body, material) in &bodies {
        let Some(mut material) = materials.get_mut(&material.0) else {
            continue;
        };
        let dir = match body {
            SkyBody::Sun => sky.sun,
            SkyBody::Moon => sky.moon,
        };
        // Fond sous l'horizon plutôt que de disparaître d'un coup.
        let shown = ((dir.y + 0.08) * 8.0).clamp(0.0, 1.0) * (1.0 - thick);
        material.base_color = match body {
            SkyBody::Sun => {
                // Plus rouge et moins éblouissant au lever et au coucher.
                let warm = 1.0 - (sky.sun.y * 3.0).clamp(0.0, 1.0);
                Color::LinearRgba(LinearRgba::new(6.0, 5.0 - 2.5 * warm, 3.6 - 3.0 * warm, 1.0) * shown)
            }
            SkyBody::Moon => Color::LinearRgba(LinearRgba::new(1.6, 1.65, 1.8, shown * sky.night.max(0.35))),
        };
    }
    for material in &stars {
        if let Some(mut material) = materials.get_mut(&material.0) {
            let shown = sky.night * sky.night * (1.0 - thick);
            material.base_color = Color::LinearRgba(LinearRgba::new(1.4, 1.4, 1.6, 1.0) * shown);
        }
    }
}

/// Garde les astres autour de la caméra, à leur place dans le ciel, tournés vers elle.
fn place_sky(
    clock: Res<GameClock>,
    camera: Single<&Transform, (With<Camera3d>, Without<SkyBody>, Without<Stars>)>,
    mut bodies: Query<(&SkyBody, &mut Transform), Without<Stars>>,
    mut stars: Query<&mut Transform, (With<Stars>, Without<SkyBody>)>,
) {
    let eye = camera.translation;
    let sky = Sky::at(clock.time);
    for (body, mut transform) in &mut bodies {
        let (dir, size) = match body {
            SkyBody::Sun => (sky.sun, SUN_SIZE),
            SkyBody::Moon => (sky.moon, MOON_SIZE),
        };
        *transform = Transform::from_translation(eye + dir * SKY_DISTANCE)
            .looking_at(eye, Vec3::Y)
            .with_scale(Vec3::splat(size));
    }
    // Les étoiles tournent lentement avec la nuit, autour de l'axe des astres.
    let axis = Vec3::new(0.0, -0.35, 1.0).normalize();
    let turn = Quat::from_axis_angle(axis, clock.time.phase * std::f32::consts::TAU);
    for mut transform in &mut stars {
        *transform = Transform::from_translation(eye).with_rotation(turn);
    }
}

fn spawn_sky(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    // Un carré de côté 1, face à -Z (le côté que `looking_at` tourne vers la caméra).
    let quad = meshes.add(Rectangle::new(1.0, 1.0));
    let sky_material = |texture: Handle<Image>, alpha_mode: AlphaMode| StandardMaterial {
        base_color_texture: Some(texture),
        unlit: true,
        fog_enabled: false,
        alpha_mode,
        double_sided: true,
        cull_mode: None,
        ..default()
    };
    let sun = images.add(disc_image(64, sun_pixel));
    let moon = images.add(disc_image(64, moon_pixel));
    commands.spawn((
        SkyBody::Sun,
        Mesh3d(quad.clone()),
        MeshMaterial3d(materials.add(sky_material(sun, AlphaMode::Add))),
        Transform::default(),
        NotShadowCaster,
        bevy::camera::visibility::NoFrustumCulling,
    ));
    commands.spawn((
        SkyBody::Moon,
        Mesh3d(quad),
        MeshMaterial3d(materials.add(sky_material(moon, AlphaMode::Blend))),
        Transform::default(),
        NotShadowCaster,
        bevy::camera::visibility::NoFrustumCulling,
    ));
    commands.spawn((
        Stars,
        Mesh3d(meshes.add(star_mesh(STAR_COUNT, STAR_DISTANCE))),
        MeshMaterial3d(materials.add(StandardMaterial {
            unlit: true,
            fog_enabled: false,
            alpha_mode: AlphaMode::Add,
            cull_mode: None,
            ..default()
        })),
        Transform::default(),
        NotShadowCaster,
        bevy::camera::visibility::NoFrustumCulling,
    ));
}

/// Image carrée RGBA dessinée pixel par pixel ; `pixel` reçoit la position depuis le centre (-1 à 1).
fn disc_image(size: u32, pixel: impl Fn(f32, f32) -> [f32; 4]) -> Image {
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let u = (x as f32 + 0.5) / size as f32 * 2.0 - 1.0;
            let v = (y as f32 + 0.5) / size as f32 * 2.0 - 1.0;
            for c in pixel(u, v) {
                data.push((c.clamp(0.0, 1.0) * 255.0).round() as u8);
            }
        }
    }
    Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        // Valeurs linéaires : un halo faible reste visible au lieu d'être écrasé par la conversion sRGB.
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// Soleil : un disque franc au centre, entouré d'un halo qui s'efface.
fn sun_pixel(u: f32, v: f32) -> [f32; 4] {
    let r = (u * u + v * v).sqrt();
    let core = if r < 0.28 { 1.0 } else { 0.0 };
    let halo = (1.0 - r).max(0.0).powi(2) * 0.35;
    let a = (core + halo).min(1.0);
    [a, a, a, 1.0]
}

/// Lune : un disque pâle, quelques mers plus sombres, bord adouci.
fn moon_pixel(u: f32, v: f32) -> [f32; 4] {
    let r = (u * u + v * v).sqrt();
    let edge = ((0.62 - r) * 30.0).clamp(0.0, 1.0);
    let maria = [
        (-0.18, -0.12, 0.17),
        (0.2, 0.1, 0.13),
        (0.02, 0.25, 0.1),
        (-0.25, 0.22, 0.07),
    ];
    let dark = maria
        .iter()
        .map(|&(x, y, s)| {
            if (u - x).powi(2) + (v - y).powi(2) < s * s {
                0.22
            } else {
                0.0
            }
        })
        .sum::<f32>();
    let shade = 0.92 - dark - 0.12 * r;
    // Une lueur discrète autour du disque.
    let glow = (1.0 - r).max(0.0).powi(4) * 0.25;
    [shade, shade, shade * 1.03, (edge + glow).min(1.0)]
}

/// Étoiles : de petits carrés semés sur une sphère, tournés vers le centre, plus ou moins brillants.
fn star_mesh(count: usize, radius: f32) -> Mesh {
    let mut rng = fastrand::Rng::with_seed(0x5747_4152);
    let mut positions = Vec::with_capacity(count * 4);
    let mut colors = Vec::with_capacity(count * 4);
    let mut indices = Vec::with_capacity(count * 6);
    for i in 0..count {
        // Point uniforme sur la sphère.
        let z = rng.f32() * 2.0 - 1.0;
        let a = rng.f32() * std::f32::consts::TAU;
        let s = (1.0 - z * z).sqrt();
        let dir = Vec3::new(s * a.cos(), z, s * a.sin());
        let center = dir * radius;
        let (t, b) = dir.any_orthonormal_pair();
        let size = 0.9 + rng.f32() * rng.f32() * 2.6;
        let bright = 0.35 + rng.f32() * 0.65;
        // Quelques étoiles bleutées ou dorées.
        let tint = match rng.u8(0..10) {
            0 => [0.75, 0.85, 1.0],
            1 => [1.0, 0.9, 0.7],
            _ => [1.0, 1.0, 1.0],
        };
        for (dx, dy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            positions.push((center + (t * dx + b * dy) * size * 0.5).to_array());
            colors.push([tint[0] * bright, tint[1] * bright, tint[2] * bright, 1.0]);
        }
        let k = (i * 4) as u32;
        indices.extend_from_slice(&[k, k + 1, k + 2, k, k + 2, k + 3]);
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
        .with_inserted_indices(Indices::U32(indices))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(phase: f32) -> Sky {
        Sky::at(GameTime { day: 1, phase })
    }

    #[test]
    fn the_sun_crosses_the_sky_by_day_and_the_moon_by_night() {
        let noon = at(DAYLIGHT_FRACTION / 2.0);
        assert!(noon.sun.y > 0.9, "soleil haut à midi");
        assert_eq!(noon.night, 0.0);
        assert!(at(0.02).sun.x > 0.8, "levé à l'est");
        assert!(at(DAYLIGHT_FRACTION - 0.02).sun.x < -0.8, "couché à l'ouest");
        let midnight = at((1.0 + DAYLIGHT_FRACTION) / 2.0);
        assert!(midnight.moon.y > 0.9, "lune haute à minuit");
        assert_eq!(midnight.night, 1.0);
    }

    #[test]
    fn the_light_comes_from_whichever_body_is_up_and_never_grazes() {
        let (body, toward, strength) = at(0.2).light();
        assert!(body == SkyBody::Sun && strength > 0.9);
        assert!(toward.y < -0.2, "la lumière descend vers le sol");
        let (body, toward, _) = at(0.85).light();
        assert!(body == SkyBody::Moon);
        assert!(toward.y <= -0.32, "même au ras de l'horizon, la lumière garde un angle");
        // À l'aube pile, l'astre est sur l'horizon : il n'éclaire presque plus.
        let (_, _, strength) = at(0.0).light();
        assert!(strength < 0.05);
    }
}
