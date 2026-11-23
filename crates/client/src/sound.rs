//! Sons du jeu, synthétisés au démarrage (pas de fichiers audio à distribuer).
//!
//! Les sons du monde (zombies, tirs, blocs, eau, feux de camp, survivants au travail) sont
//! spatialisés autour de la caméra. Le corps du joueur s'entend aussi : manger, boire,
//! grelotter de froid, haleter de chaleur, tousser quand l'infection gagne.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::audio::{AudioSource, PlaybackSettings, SpatialListener, SpatialScale, Volume};
use bevy::prelude::*;
use shared::protocol::ZombieState;

use crate::AppState;
use crate::net::{ClientWorld, NetEvent};
use crate::player::LocalPlayer;
use crate::sky::GameClock;
use crate::survivors::SurvivorView;
use crate::zombies::ZombieView;

const RATE: u32 = 22_050;

pub struct SoundPlugin;

impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, build_sounds)
            .add_systems(Update, add_listener)
            .add_systems(
                Update,
                (
                    world_sounds,
                    footsteps,
                    zombie_sounds,
                    alarm,
                    body_sounds,
                    campfire_sounds,
                    work_sounds,
                )
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
    Parry,
    Explosion,
    Splash,
    Eat,
    Drink,
    Cough,
    Shiver,
    Pant,
    Infected,
    Heal,
    Crackle,
    Hammer,
    Zap,
    Rumble,
    Howl,
    Thunder,
    Fanfare,
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
        // Choc métallique : deux partiels aigus qui sonnent un instant.
        Sfx::Parry => samples(0.3, |t, k| {
            ((t * 1250.0 * TAU).sin() + 0.6 * (t * 1870.0 * TAU).sin()) * (1.0 - k).powi(3) * 0.4
        }),
        Sfx::Explosion => {
            // Détonation sourde : bruit filtré grave qui s'éteint lentement.
            let mut s = noise_burst(1.2, 0.08, 67, |k| (k * 60.0).min(1.0) * (1.0 - k).powi(3));
            for (i, v) in s.iter_mut().enumerate() {
                let t = i as f32 / RATE as f32;
                *v = (*v * 2.5 + (t * 45.0 * TAU).sin() * (1.0 - t / 1.2).max(0.0).powi(4) * 0.6).clamp(-1.0, 1.0);
            }
            s
        }
        // Plouf : bruit filtré qui monte puis retombe, avec un petit « bloup » grave.
        Sfx::Splash => {
            let mut s = noise_burst(0.5, 0.25, 71, |k| (k * 12.0).min(1.0) * (1.0 - k).powi(2) * 0.7);
            for (i, v) in s.iter_mut().enumerate() {
                let t = i as f32 / RATE as f32;
                *v += (t * (300.0 - 400.0 * t) * TAU).sin() * (1.0 - t / 0.2).max(0.0) * 0.3;
            }
            s
        }
        // Croc : trois petits craquements.
        Sfx::Eat => {
            let mut noise = Noise(83);
            samples(0.45, |_, k| {
                let bite = (k * 3.0).fract();
                noise.next() * (1.0 - bite).powi(6) * 0.5
            })
        }
        // Gorgées : deux « glouc » qui descendent.
        Sfx::Drink => samples(0.5, |t, k| {
            let gulp = (k * 2.0).fract();
            let f = 420.0 - 260.0 * gulp;
            (t * f * TAU).sin() * (gulp * 10.0).min(1.0) * (1.0 - gulp).powi(2) * 0.4
        }),
        Sfx::Cough => {
            let mut s = noise_burst(0.35, 0.45, 89, |k| (k * 30.0).min(1.0) * (1.0 - k).powi(3) * 0.7);
            for (i, v) in s.iter_mut().enumerate() {
                let t = i as f32 / RATE as f32;
                *v += (t * 140.0 * TAU).sin().signum() * 0.12 * (1.0 - t / 0.35).max(0.0);
            }
            s
        }
        // Dents qui claquent : petits clics rapides et irréguliers.
        Sfx::Shiver => {
            let mut noise = Noise(97);
            samples(0.6, |t, _| {
                let click = (t * 23.0).fract();
                noise.next() * (1.0 - click).powi(14) * 0.45
            })
        }
        // Halètement : deux souffles.
        Sfx::Pant => {
            let mut noise = Noise(101);
            let mut low = 0.0;
            samples(0.9, |_, k| {
                low += (noise.next() - low) * 0.3;
                let breath = (k * 2.0).fract();
                low * (breath * std::f32::consts::PI).sin() * 0.35
            })
        }
        // Battement sourd et inquiétant : l'infection vient de prendre.
        Sfx::Infected => samples(1.2, |t, _| {
            let beat = |start: f32| {
                let d = t - start;
                if d < 0.0 {
                    0.0
                } else {
                    (d * 55.0 * TAU).sin() * (-d * 18.0).exp()
                }
            };
            (beat(0.0) + 0.8 * beat(0.22) + beat(0.7) + 0.8 * beat(0.92)) * 0.7
        }),
        // Guérison : un accord clair qui monte.
        Sfx::Heal => samples(0.7, |t, k| {
            let f = if k < 0.33 {
                523.0
            } else if k < 0.66 {
                659.0
            } else {
                784.0
            };
            (t * f * TAU).sin() * (1.0 - (k * 3.0).fract()).powf(0.5) * 0.25
        }),
        // Crépitement du feu : petits claquements au hasard sur un souffle.
        Sfx::Crackle => {
            let mut noise = Noise(107);
            let mut low = 0.0;
            samples(1.5, |_, k| {
                low += (noise.next() - low) * 0.05;
                let pop = if noise.next() > 0.995 { noise.next() } else { 0.0 };
                (low * 0.3 + pop * 0.8) * (k * 10.0).min(1.0) * (1.0 - k).powf(0.3)
            })
        }
        // Coup de marteau sur du bois.
        Sfx::Hammer => {
            let mut s = noise_burst(0.12, 0.6, 113, |k| (1.0 - k).powi(5) * 0.6);
            for (i, v) in s.iter_mut().enumerate() {
                let t = i as f32 / RATE as f32;
                *v += (t * 620.0 * TAU).sin() * (-t * 40.0).exp() * 0.35;
            }
            s
        }
        // Arc électrique : bourdonnement grésillant.
        Sfx::Zap => {
            let mut noise = Noise(127);
            samples(0.3, |t, k| {
                let buzz = (t * 120.0 * TAU).sin().signum() * 0.3;
                (buzz + noise.next() * 0.4) * (1.0 - k).powi(2) * 0.7
            })
        }
        // Effondrement : grondement grave et fracas de débris.
        Sfx::Rumble => {
            let mut noise = Noise(131);
            let mut low = 0.0;
            samples(1.6, |_, k| {
                low += (noise.next() - low) * 0.02;
                let debris = if noise.next() > 0.99 {
                    noise.next() * (1.0 - k)
                } else {
                    0.0
                };
                (low * 2.5 + debris * 0.5).clamp(-1.0, 1.0) * (k * 20.0).min(1.0) * (1.0 - k).powf(0.7) * 0.8
            })
        }
        // Cri du hurleur : une plainte aiguë qui monte puis retombe, éraillée.
        Sfx::Howl => {
            let mut noise = Noise(149);
            let mut phase = 0.0f32;
            samples(1.8, |_, k| {
                let pitch = 380.0 + 520.0 * (k * std::f32::consts::PI).sin() - 120.0 * k;
                phase += pitch / RATE as f32;
                let rasp = 1.0 + 0.35 * noise.next();
                let env = (k * 10.0).min(1.0) * (1.0 - k).powf(0.8);
                ((phase * TAU).sin() * 0.6 + (phase * 2.0 * TAU).sin() * 0.25) * rasp * env * 0.7
            })
        }
        // Tonnerre : un claquement sec puis un long grondement qui roule.
        Sfx::Thunder => {
            let mut noise = Noise(173);
            let mut low = 0.0;
            let mut lower = 0.0;
            samples(3.5, |_, k| {
                let n = noise.next();
                low += (n - low) * 0.05;
                lower += (low - lower) * 0.03;
                let crack = n * (1.0 - k * 12.0).max(0.0);
                let roll = lower * 6.0 * (1.0 + 0.5 * (k * 9.0).sin());
                ((crack * 0.9 + roll) * (k * 40.0).min(1.0) * (1.0 - k).powf(1.2)).clamp(-1.0, 1.0)
            })
        }
        // Succès : trois notes montantes, claires, comme un petit carillon.
        Sfx::Fanfare => samples(1.4, |t, _| {
            let notes = [(0.0, 523.25), (0.14, 659.25), (0.28, 783.99), (0.42, 1046.5)];
            notes
                .iter()
                .filter(|(start, _)| t >= *start)
                .map(|(start, f)| {
                    let dt = t - start;
                    let env = (dt * 80.0).min(1.0) * (-dt * 3.5).exp();
                    ((dt * f * TAU).sin() + 0.3 * (dt * f * 2.0 * TAU).sin()) * env * 0.25
                })
                .sum()
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
        Sfx::Parry,
        Sfx::Explosion,
        Sfx::Splash,
        Sfx::Eat,
        Sfx::Drink,
        Sfx::Cough,
        Sfx::Shiver,
        Sfx::Pant,
        Sfx::Infected,
        Sfx::Heal,
        Sfx::Crackle,
        Sfx::Hammer,
        Sfx::Zap,
        Sfx::Rumble,
        Sfx::Howl,
        Sfx::Thunder,
        Sfx::Fanfare,
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
                let block = world.block(*pos);
                // L'eau courante qui avance ou se retire ne fait pas de bruit à chaque bloc.
                if block == shared::block::Block::FlowingWater {
                    continue;
                }
                let sfx = if block == shared::block::Block::Water {
                    Sfx::Splash
                } else if world.is_solid(*pos) {
                    Sfx::Place
                } else {
                    Sfx::Break
                };
                play(&mut commands, &sounds, sfx, Some(pos.as_vec3() + Vec3::splat(0.5)), 0.8);
            }
            NetEvent::Shot { origin, rifle, .. } => {
                let sfx = if *rifle { Sfx::Rifle } else { Sfx::Bow };
                play(&mut commands, &sounds, sfx, Some(*origin), 1.0);
            }
            NetEvent::Parried => play(&mut commands, &sounds, Sfx::Parry, None, 0.8),
            NetEvent::Zap { origin, .. } => play(&mut commands, &sounds, Sfx::Zap, Some(*origin), 0.8),
            NetEvent::Explosion { pos, .. } => play(&mut commands, &sounds, Sfx::Explosion, Some(*pos), 1.0),
            NetEvent::Collapse { pos, .. } => play(&mut commands, &sounds, Sfx::Rumble, Some(*pos), 1.0),
            NetEvent::Howl { pos } => play(&mut commands, &sounds, Sfx::Howl, Some(*pos + Vec3::Y * 1.8), 1.0),
            NetEvent::Bite => play(&mut commands, &sounds, Sfx::Splash, None, 0.5),
            NetEvent::Achievement(_) => play(&mut commands, &sounds, Sfx::Fanfare, None, 0.8),
            NetEvent::Lightning { pos } => play(&mut commands, &sounds, Sfx::Thunder, Some(*pos + Vec3::Y * 10.0), 1.0),
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

/// Ce qui arrive au corps du joueur : manger, boire, plonger, l'infection, le froid, la chaleur.
fn body_sounds(
    mut commands: Commands,
    sounds: Res<Sounds>,
    player: Res<LocalPlayer>,
    time: Res<Time>,
    mut last: Local<Option<(f32, f32, f32, bool)>>,
    mut next_symptom: Local<f32>,
) {
    use shared::climate::{HYPOTHERMIA, INFECTION_HARMFUL, OVERHEATING};
    if player.dead.is_some() {
        *last = None;
        return;
    }
    if let Some((hunger, thirst, infection, swimming)) = *last {
        if player.hunger > hunger + 1.0 {
            play(&mut commands, &sounds, Sfx::Eat, None, 0.6);
        }
        if player.thirst > thirst + 1.0 {
            play(&mut commands, &sounds, Sfx::Drink, None, 0.6);
        }
        if infection <= 0.0 && player.infection > 0.0 {
            play(&mut commands, &sounds, Sfx::Infected, None, 0.8);
        }
        if infection > 0.0 && player.infection <= 0.0 {
            play(&mut commands, &sounds, Sfx::Heal, None, 0.6);
        }
        if !swimming && player.swimming {
            play(&mut commands, &sounds, Sfx::Splash, None, 0.7);
        }
    }
    *last = Some((player.hunger, player.thirst, player.infection, player.swimming));

    // Symptômes qui reviennent tant que dure le mal.
    *next_symptom -= time.delta_secs();
    if *next_symptom > 0.0 {
        return;
    }
    let symptom = if player.body_temp < HYPOTHERMIA + 0.5 {
        Some((Sfx::Shiver, 3.0))
    } else if player.body_temp > OVERHEATING {
        Some((Sfx::Pant, 2.5))
    } else if player.infection >= INFECTION_HARMFUL {
        Some((Sfx::Cough, 8.0 + fastrand::f32() * 6.0))
    } else {
        None
    };
    if let Some((sfx, every)) = symptom {
        play(&mut commands, &sounds, sfx, None, 0.5);
        *next_symptom = every;
    } else {
        *next_symptom = 1.0;
    }
}

/// Le feu de camp le plus proche crépite.
fn campfire_sounds(
    mut commands: Commands,
    sounds: Res<Sounds>,
    world: Option<Res<ClientWorld>>,
    player: Res<LocalPlayer>,
    time: Res<Time>,
    mut timer: Local<f32>,
) {
    *timer -= time.delta_secs();
    if *timer > 0.0 {
        return;
    }
    *timer = 1.4;
    let Some(world) = world.as_deref() else { return };
    let me = player.position.floor().as_ivec3();
    let mut nearest: Option<(IVec3, i32)> = None;
    for dx in -6..=6 {
        for dz in -6..=6 {
            for dy in -3..=3 {
                let p = me + IVec3::new(dx, dy, dz);
                let d = dx * dx + dy * dy + dz * dz;
                if world.block(p) == shared::block::Block::Campfire && nearest.is_none_or(|(_, n)| d < n) {
                    nearest = Some((p, d));
                }
            }
        }
    }
    if let Some((fire, _)) = nearest {
        play(
            &mut commands,
            &sounds,
            Sfx::Crackle,
            Some(fire.as_vec3() + Vec3::splat(0.5)),
            0.5,
        );
    }
}

/// Coups de marteau des survivants qui travaillent près du joueur.
fn work_sounds(
    mut commands: Commands,
    sounds: Res<Sounds>,
    player: Res<LocalPlayer>,
    survivors: Query<(&SurvivorView, &Transform)>,
    time: Res<Time>,
    mut timer: Local<f32>,
) {
    *timer -= time.delta_secs();
    if *timer > 0.0 {
        return;
    }
    *timer = 1.1;
    for (view, transform) in &survivors {
        if view.working() && transform.translation.distance(player.position) < 24.0 {
            play(
                &mut commands,
                &sounds,
                Sfx::Hammer,
                Some(transform.translation + Vec3::Y * 1.0),
                0.5,
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
        for sfx in [
            Sfx::Splash,
            Sfx::Eat,
            Sfx::Drink,
            Sfx::Cough,
            Sfx::Shiver,
            Sfx::Pant,
            Sfx::Infected,
            Sfx::Heal,
            Sfx::Crackle,
            Sfx::Hammer,
            Sfx::Zap,
            Sfx::Rumble,
        ] {
            let s = synth(sfx);
            assert!(
                !s.is_empty() && s.iter().all(|v| v.is_finite() && v.abs() <= 1.0),
                "{sfx:?}"
            );
        }
    }
}
