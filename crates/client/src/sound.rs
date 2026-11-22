//! Sons du jeu, synthétisés au démarrage (pas de fichiers audio à distribuer).
//!
//! Les sons du monde (zombies, tirs, blocs) sont spatialisés autour de la caméra.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::audio::{AudioSource, PlaybackSettings, SpatialListener, SpatialScale, Volume};
use bevy::prelude::*;
use shared::protocol::ZombieState;

use crate::AppState;
use crate::net::{ClientWorld, NetEvent};
use crate::player::LocalPlayer;
use crate::sky::GameClock;
use crate::zombies::ZombieView;

const RATE: u32 = 22_050;

pub struct SoundPlugin;

impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, build_sounds)
            .add_systems(Update, add_listener)
            .add_systems(
                Update,
                (world_sounds, footsteps, zombie_sounds, alarm)
                    .run_if(in_state(AppState::Playing))
                    .run_if(resource_exists::<Sounds>),
            );
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Sfx {
    Step,
    Break,
    Place,
    Rifle,
    Bow,
    Groan(u8),
    ZombieHit,
    ZombieBite,
    Hurt,
    Horn,
}

#[derive(Resource)]
struct Sounds(HashMap<Sfx, Handle<AudioSource>>);

// ---------- Synthèse ----------

/// Petit générateur pseudo-aléatoire déterministe (bruit blanc).
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

fn samples(duration: f32, mut f: impl FnMut(f32, f32) -> f32) -> Vec<f32> {
    let n = (duration * RATE as f32) as usize;
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            f(t, t / duration)
        })
        .collect()
}

/// Bruit filtré (passe-bas simple) avec une enveloppe.
fn noise_burst(duration: f32, smooth: f32, seed: u32, env: impl Fn(f32) -> f32) -> Vec<f32> {
    let mut noise = Noise(seed);
    let mut low = 0.0;
    samples(duration, |_, k| {
        low += (noise.next() - low) * smooth;
        low * env(k)
    })
}

fn groan(seed: u32) -> Vec<f32> {
    let mut noise = Noise(seed);
    let base = 70.0 + (seed % 5) as f32 * 9.0;
    let mut phase = 0.0f32;
    samples(1.1, |t, k| {
        let pitch = base * (1.0 + 0.25 * (t * 3.0 + seed as f32).sin()) * (1.0 - 0.2 * k);
        phase += pitch / RATE as f32;
        let saw = (phase.fract() * 2.0 - 1.0) * 0.6;
        let rasp = noise.next() * 0.25;
        let env = (k * 8.0).min(1.0) * (1.0 - k).powf(0.7);
        (saw + rasp) * env * 0.7
    })
}

fn wav(samples: &[f32]) -> Vec<u8> {
    let data_len = samples.len() as u32 * 2;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).to_le_bytes());
    }
    out
}

fn synth(sfx: Sfx) -> Vec<f32> {
    use std::f32::consts::TAU;
    match sfx {
        Sfx::Step => noise_burst(0.09, 0.15, 11, |k| (1.0 - k).powi(3) * 0.5),
        Sfx::Break => noise_burst(0.22, 0.35, 23, |k| (1.0 - k).powi(2) * 0.8),
        Sfx::Place => {
            let mut s = noise_burst(0.07, 0.2, 31, |k| (1.0 - k).powi(4) * 0.6);
            for (i, v) in s.iter_mut().enumerate() {
                *v += (i as f32 / RATE as f32 * 180.0 * TAU).sin() * 0.3 * (1.0 - i as f32 / 1544.0).max(0.0);
            }
            s
        }
        Sfx::Rifle => noise_burst(0.35, 0.8, 41, |k| (1.0 - k).powi(6)),
        Sfx::Bow => samples(0.18, |t, k| {
            ((t * 220.0 * TAU).sin() * 0.5 + (t * 330.0 * TAU).sin() * 0.2) * (1.0 - k).powi(2)
        }),
        Sfx::Groan(i) => groan(100 + i as u32),
        Sfx::ZombieHit => samples(0.14, |t, k| (t * 90.0 * TAU).sin() * (1.0 - k).powi(2)),
        Sfx::ZombieBite => noise_burst(0.25, 0.5, 53, |k| (k * 20.0).min(1.0) * (1.0 - k).powi(2) * 0.8),
        Sfx::Hurt => samples(0.25, |t, k| {
            let f = 260.0 - 120.0 * k;
            (t * f * TAU).sin().signum() * 0.25 * (1.0 - k)
        }),
        Sfx::Horn => samples(2.5, |t, k| {
            let env = (k * 6.0).min(1.0) * (1.0 - k).powf(0.5);
            ((t * 82.0 * TAU).sin() + 0.5 * (t * 123.0 * TAU).sin() + 0.25 * (t * 164.0 * TAU).sin()) * env * 0.4
        }),
    }
}

fn build_sounds(mut commands: Commands, mut sources: ResMut<Assets<AudioSource>>) {
    let mut all = vec![
        Sfx::Step,
        Sfx::Break,
        Sfx::Place,
        Sfx::Rifle,
        Sfx::Bow,
        Sfx::ZombieHit,
        Sfx::ZombieBite,
        Sfx::Hurt,
        Sfx::Horn,
    ];
    all.extend((0..3).map(Sfx::Groan));
    let map = all
        .into_iter()
        .map(|sfx| {
            let bytes: Arc<[u8]> = wav(&synth(sfx)).into();
            (sfx, sources.add(AudioSource { bytes }))
        })
        .collect();
    commands.insert_resource(Sounds(map));
}

fn add_listener(mut commands: Commands, cameras: Query<Entity, (With<Camera3d>, Without<SpatialListener>)>) {
    for camera in &cameras {
        commands.entity(camera).insert(SpatialListener::new(0.3));
    }
}

// ---------- Lecture ----------

fn play(commands: &mut Commands, sounds: &Sounds, sfx: Sfx, at: Option<Vec3>, volume: f32) {
    let Some(handle) = sounds.0.get(&sfx) else { return };
    let mut settings = PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume));
    let mut entity = commands.spawn(AudioPlayer::new(handle.clone()));
    if let Some(pos) = at {
        // Une unité de distance sonore pour 6 m : on entend une horde de loin.
        settings = settings
            .with_spatial(true)
            .with_spatial_scale(SpatialScale::new(1.0 / 6.0));
        entity.insert(Transform::from_translation(pos));
    }
    entity.insert(settings);
}

fn world_sounds(
    mut commands: Commands,
    sounds: Res<Sounds>,
    mut events: MessageReader<NetEvent>,
    world: Option<Res<ClientWorld>>,
    player: Res<LocalPlayer>,
    time: Res<Time>,
    mut recent: Local<HashMap<IVec3, f32>>,
    mut last_health: Local<f32>,
) {
    let now = time.elapsed_secs();
    recent.retain(|_, at| now - *at < 0.4);
    for event in events.read() {
        match event {
            NetEvent::BlockChanged(pos) => {
                // Le client anticipe puis le serveur confirme : un seul son par bloc.
                if recent.insert(*pos, now).is_some() {
                    continue;
                }
                let Some(world) = world.as_deref() else { continue };
                let sfx = if world.is_solid(*pos) { Sfx::Place } else { Sfx::Break };
                play(&mut commands, &sounds, sfx, Some(pos.as_vec3() + Vec3::splat(0.5)), 0.8);
            }
            NetEvent::Shot { origin, rifle, .. } => {
                let sfx = if *rifle { Sfx::Rifle } else { Sfx::Bow };
                play(&mut commands, &sounds, sfx, Some(*origin), 1.0);
            }
            _ => {}
        }
    }
    if player.health < *last_health - 0.5 {
        play(&mut commands, &sounds, Sfx::Hurt, None, 0.6);
    }
    *last_health = player.health;
}

fn footsteps(
    mut commands: Commands,
    sounds: Res<Sounds>,
    player: Res<LocalPlayer>,
    time: Res<Time>,
    mut travelled: Local<f32>,
) {
    let speed = Vec2::new(player.velocity.x, player.velocity.z).length();
    if !player.on_ground || speed < 0.5 || player.dead.is_some() {
        return;
    }
    *travelled += speed * time.delta_secs();
    let stride = if speed > 5.5 { 2.2 } else { 1.8 };
    if *travelled >= stride {
        *travelled = 0.0;
        play(&mut commands, &sounds, Sfx::Step, None, 0.35);
    }
}

/// Grognements au hasard, coups et morsures des zombies proches.
fn zombie_sounds(
    mut commands: Commands,
    sounds: Res<Sounds>,
    time: Res<Time>,
    zombies: Query<(&ZombieView, &Transform)>,
    mut previous: Local<HashMap<u32, u8>>,
    mut next_groan: Local<f32>,
) {
    let mut seen = HashMap::new();
    for (view, transform) in &zombies {
        let before = previous.get(&view.id).copied().unwrap_or(0);
        let rising = view.flags & !before;
        let head = transform.translation + Vec3::Y * 1.5;
        if rising & ZombieState::ATTACKING != 0 {
            play(&mut commands, &sounds, Sfx::ZombieBite, Some(head), 0.9);
        }
        if rising & ZombieState::HURT != 0 {
            play(&mut commands, &sounds, Sfx::ZombieHit, Some(head), 0.9);
        }
        seen.insert(view.id, view.flags);
    }
    *previous = seen;

    // Un grognement de temps en temps, plus souvent quand la horde est nombreuse.
    *next_groan -= time.delta_secs();
    let count = zombies.iter().len();
    if *next_groan <= 0.0 && count > 0 {
        *next_groan = (6.0 / count as f32).clamp(0.4, 4.0) * (0.6 + fastrand::f32() * 0.8);
        if let Some((view, transform)) = zombies.iter().nth(fastrand::usize(..count)) {
            let variant = (view.id % 3) as u8;
            play(
                &mut commands,
                &sounds,
                Sfx::Groan(variant),
                Some(transform.translation + Vec3::Y * 1.5),
                0.8,
            );
        }
    }
}

/// Corne grave à la tombée de la nuit.
fn alarm(mut commands: Commands, sounds: Res<Sounds>, clock: Res<GameClock>, mut was_night: Local<Option<bool>>) {
    let night = clock.time.is_night();
    if *was_night == Some(false) && night {
        play(&mut commands, &sounds, Sfx::Horn, None, 0.7);
    }
    *was_night = Some(night);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthesized_sounds_are_valid_wav() {
        let bytes = wav(&synth(Sfx::Rifle));
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        let data_len = u32::from_le_bytes(bytes[40..44].try_into().unwrap()) as usize;
        assert_eq!(bytes.len(), 44 + data_len);
        assert!(synth(Sfx::Groan(1)).iter().all(|s| s.is_finite() && s.abs() <= 1.0));
    }
}
