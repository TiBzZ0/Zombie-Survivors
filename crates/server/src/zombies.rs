//! Les zombies : apparition par vagues la nuit, déplacement en horde par champ de
//! flux, attaques des joueurs et des murs.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use bevy::prelude::*;
use bevy_renet::RenetServer;
use bevy_renet::renet::{ClientId, DefaultChannel};
use shared::block::Block;
use shared::chunk::{Chunk, ChunkPos};
use shared::pathfinding::{FlowField, build_flow_field};
use shared::physics::step_body;
use shared::protocol::{ServerMessage, ZombieState, encode};
use shared::world::{block_containing, chunk_of};
use shared::zombie::{
    SPIT_COOLDOWN, SPIT_DAMAGE, SPIT_RANGE, ZOMBIE_ATTACK_COOLDOWN, ZOMBIE_REACH, ZombieKind, stacking,
};

use crate::daynight::Clock;
use crate::net::{Lobby, MAX_HEALTH, MAX_HUNGER, MAX_THIRST, WorldVersion, send_vitals};
use crate::survivors::Survivor;
use crate::world::{Chests, GameWorld, SpawnPoint};

/// Rayon exploré par le champ de flux autour des joueurs, en blocs.
const FLOW_RADIUS: i32 = 48;
const FLOW_MAX_NODES: usize = 40_000;
/// Intervalle minimal entre deux recalculs du champ.
const FLOW_INTERVAL: f32 = 1.0;
/// Distance d'apparition autour d'un joueur.
const SPAWN_MIN_DIST: f32 = 24.0;
const SPAWN_MAX_DIST: f32 = 40.0;
/// Au-delà, un zombie est trop loin de tout joueur et disparaît.
const DESPAWN_DIST: f32 = 120.0;
/// Rayon dans lequel un joueur reçoit les zombies.
const VIEW_DIST: f32 = 90.0;
/// En deçà, les zombies sont envoyés à chaque fois (15 fois par seconde).
const NEAR_DIST: f32 = 40.0;
/// Dégâts du soleil par seconde.
const SUN_DPS: f32 = 6.0;
/// Assez pour franchir une marche d'un bloc avec de la marge.
const JUMP_SPEED: f32 = 8.5;

pub struct ZombiePlugin;

impl Plugin for ZombiePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NextZombieId>()
            .init_resource::<Flow>()
            .init_resource::<BlockDamage>()
            .init_resource::<Waves>()
            .init_resource::<TickStats>()
            .add_systems(Startup, stress_test)
            .add_systems(FixedFirst, start_tick_timer)
            .add_systems(
                FixedUpdate,
                (
                    update_flow_field,
                    spawn_waves,
                    zombie_ai.in_set(ZombieSet::Ai),
                    separate_zombies,
                    sun_and_cleanup,
                    kill_dead_zombies.in_set(ZombieSet::Deaths),
                    player_survival,
                    broadcast_zombies,
                )
                    .chain(),
            )
            .add_systems(FixedLast, end_tick_timer)
            .add_systems(Update, report_tick_stats);
    }
}

/// Étapes de la simulation des zombies, pour ordonner les autres systèmes autour.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ZombieSet {
    Ai,
    Deaths,
}

/// Un zombie. Sa position est dans `ZombieBody`.
#[derive(Component, Debug)]
pub struct Zombie {
    pub id: u32,
    pub kind: ZombieKind,
    pub health: f32,
    pub attack_cooldown: f32,
    /// Durée restante de l'animation d'attaque.
    pub attacking: f32,
    /// Durée restante de l'effet « touché ».
    pub hurt_timer: f32,
    /// Recul à appliquer au prochain pas.
    pub knockback: Vec3,
    pub last_attacker: Option<ClientId>,
    /// Hauteur où il a commencé à grimper sur les autres zombies (pour borner la pile).
    pub climb_from: Option<f32>,
    /// Depuis combien de temps il pousse contre un obstacle sans avancer.
    pub stuck: f32,
}

/// Corps physique d'un zombie.
#[derive(Component, Debug, Clone, Copy)]
pub struct ZombieBody {
    pub position: Vec3,
    pub velocity: Vec3,
    pub yaw: f32,
    pub on_ground: bool,
    pub hit_wall: bool,
}

#[derive(Resource, Default)]
struct NextZombieId(u32);

#[derive(Resource, Default)]
struct Flow {
    field: FlowField,
    timer: f32,
    version: u64,
}

/// Dégâts accumulés sur les blocs que les zombies frappent (avec le type du bloc :
/// un bloc remplacé repart à zéro).
#[derive(Resource, Default)]
pub(crate) struct BlockDamage(pub HashMap<IVec3, (Block, f32)>);

#[derive(Resource, Default)]
struct Waves {
    spawn_timer: f32,
    /// Jour de la dernière grande horde menée par un colosse.
    colossus_day: Option<u32>,
    /// En mode test de charge, les zombies ne brûlent pas le jour.
    stress: bool,
    /// Nuit en cours (son jour) et zombies déjà envoyés cette nuit-là.
    night: u32,
    spawned: u32,
}

/// Nombre total de zombies envoyés pendant une nuit : il grandit avec les nuits et les joueurs.
/// Sans ce plafond, un joueur qui tue vite verrait arriver un flot sans fin.
pub fn night_budget(day: u32, players: u32, horde: bool) -> u32 {
    let extra = players.saturating_sub(1);
    if horde {
        90 + 15 * day + 50 * extra
    } else {
        25 + 11 * day + 15 * extra
    }
}

#[derive(Resource, Default)]
struct TickStats {
    started: Option<Instant>,
    total_ms: f64,
    max_ms: f64,
    count: u32,
    flow_ms: f64,
}

fn start_tick_timer(mut stats: ResMut<TickStats>) {
    stats.started = Some(Instant::now());
}

fn end_tick_timer(mut stats: ResMut<TickStats>) {
    if let Some(start) = stats.started.take() {
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        stats.total_ms += ms;
        stats.max_ms = stats.max_ms.max(ms);
        stats.count += 1;
    }
}

fn report_tick_stats(
    mut stats: ResMut<TickStats>,
    zombies: Query<(), With<Zombie>>,
    flow: Res<Flow>,
    time: Res<Time>,
    mut last: Local<f32>,
) {
    if time.elapsed_secs() - *last < 30.0 || stats.count == 0 {
        return;
    }
    *last = time.elapsed_secs();
    info!(
        "Tick : {:.2} ms en moyenne, {:.2} ms au pire · {} zombies · champ de flux {} cases ({:.1} ms)",
        stats.total_ms / stats.count as f64,
        stats.max_ms,
        zombies.iter().count(),
        flow.field.len(),
        stats.flow_ms
    );
    let flow_ms = stats.flow_ms;
    *stats = TickStats { flow_ms, ..default() };
}

fn spawn_zombie(commands: &mut Commands, ids: &mut NextZombieId, kind: ZombieKind, position: Vec3) {
    ids.0 = ids.0.wrapping_add(1);
    commands.spawn((
        Zombie {
            id: ids.0,
            kind,
            health: kind.stats().health,
            attack_cooldown: 0.0,
            attacking: 0.0,
            hurt_timer: 0.0,
            knockback: Vec3::ZERO,
            last_attacker: None,
            climb_from: None,
            stuck: 0.0,
        },
        ZombieBody {
            position,
            velocity: Vec3::ZERO,
            yaw: fastrand::f32() * std::f32::consts::TAU,
            on_ground: false,
            hit_wall: false,
        },
    ));
}

/// Cherche une case libre sur le sol à une distance donnée d'un point.
pub(crate) fn find_spawn_spot(world: &GameWorld, around: Vec3, min: f32, max: f32) -> Option<Vec3> {
    for _ in 0..12 {
        let angle = fastrand::f32() * std::f32::consts::TAU;
        let dist = min + fastrand::f32() * (max - min);
        let x = (around.x + angle.cos() * dist) as i32;
        let z = (around.z + angle.sin() * dist) as i32;
        let feet = IVec3::new(x, world.generator().surface_height(x, z), z);
        if !world.in_bounds(feet) {
            continue;
        }
        let dry = !world.block(feet).is_water();
        if dry && world.is_solid(feet - IVec3::Y) && !world.is_solid(feet) && !world.is_solid(feet + IVec3::Y) {
            return Some(Vec3::new(x as f32 + 0.5, feet.y as f32, z as f32 + 0.5));
        }
    }
    None
}

fn random_kind(day: u32) -> ZombieKind {
    let r = fastrand::f32();
    let brute = (0.04 + day as f32 * 0.01).min(0.2);
    let runner = (0.15 + day as f32 * 0.015).min(0.35);
    // Les cracheurs arrivent à partir de la troisième nuit.
    let spitter = if day >= 3 {
        (0.04 + day as f32 * 0.01).min(0.15)
    } else {
        0.0
    };
    if r < brute {
        ZombieKind::Brute
    } else if r < brute + runner {
        ZombieKind::Runner
    } else if r < brute + runner + spitter {
        ZombieKind::Spitter
    } else {
        ZombieKind::Walker
    }
}

/// `STRESS_ZOMBIES=500` fait apparaître des zombies dès le démarrage, pour mesurer les performances.
fn stress_test(
    mut commands: Commands,
    world: Res<GameWorld>,
    spawn: Res<SpawnPoint>,
    mut ids: ResMut<NextZombieId>,
    mut waves: ResMut<Waves>,
) {
    let Some(count) = std::env::var("STRESS_ZOMBIES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
    else {
        return;
    };
    waves.stress = true;
    let mut spawned = 0;
    for _ in 0..count * 3 {
        if spawned == count {
            break;
        }
        if let Some(pos) = find_spawn_spot(&world, spawn.0, 12.0, 60.0) {
            spawn_zombie(&mut commands, &mut ids, random_kind(5), pos);
            spawned += 1;
        }
    }
    info!("Test de charge : {spawned} zombies créés");
}

fn update_flow_field(
    time: Res<Time>,
    mut flow: ResMut<Flow>,
    version: Res<WorldVersion>,
    lobby: Res<Lobby>,
    world: Res<GameWorld>,
    mut stats: ResMut<TickStats>,
    zombies: Query<(), With<Zombie>>,
    survivors: Query<&Survivor>,
) {
    flow.timer -= time.delta_secs();
    let world_changed = version.0 != flow.version && flow.timer < FLOW_INTERVAL - 0.25;
    if (flow.timer > 0.0 && !world_changed) || zombies.is_empty() {
        return;
    }
    // Recalcule à intervalle fixe (les joueurs bougent), et vite si des blocs ont changé.
    flow.timer = FLOW_INTERVAL;
    flow.version = version.0;
    // Les survivants postés dans une base attirent aussi la horde.
    let targets: Vec<IVec3> = lobby
        .players
        .values()
        .filter(|p| p.is_alive())
        .map(|p| p.state.position)
        .chain(survivors.iter().filter(|s| s.is_posted()).map(|s| s.position))
        .map(|p| block_containing(p + Vec3::Y * 0.1))
        .collect();
    if targets.is_empty() {
        flow.field = FlowField::default();
        return;
    }
    let start = Instant::now();
    // Cache local des chunks : évite le verrou du monde à chaque lecture de bloc.
    let chunks: RefCell<HashMap<ChunkPos, Arc<Chunk>>> = RefCell::default();
    let block = |p: IVec3| -> Block {
        if !world.in_bounds(p) {
            return Block::Air;
        }
        let cp = chunk_of(p);
        let chunk = chunks.borrow_mut().entry(cp).or_insert_with(|| world.chunk(cp)).clone();
        let l = p - IVec3::from(cp.origin());
        chunk.get(l.x as usize, l.y as usize, l.z as usize)
    };
    flow.field = build_flow_field(
        &targets,
        FLOW_RADIUS,
        FLOW_MAX_NODES,
        |p| block(p).is_solid(),
        |p| (p.y > 0).then(|| block(p).max_health() as f32),
    );
    stats.flow_ms = start.elapsed().as_secs_f64() * 1000.0;
}

fn spawn_waves(
    mut commands: Commands,
    time: Res<Time>,
    clock: Res<Clock>,
    lobby: Res<Lobby>,
    world: Res<GameWorld>,
    mut waves: ResMut<Waves>,
    mut ids: ResMut<NextZombieId>,
    zombies: Query<(), With<Zombie>>,
    mut server: Option<ResMut<RenetServer>>,
    weather: Option<Res<crate::events::Weather>>,
) {
    let fog = weather.is_some_and(|w| w.fog);
    if !clock.is_night() && !fog {
        return;
    }
    waves.spawn_timer -= time.delta_secs();
    if waves.spawn_timer > 0.0 {
        return;
    }
    let players: Vec<Vec3> = lobby
        .players
        .values()
        .filter(|p| p.is_alive())
        .map(|p| p.state.position)
        .collect();
    if players.is_empty() {
        return;
    }
    let horde = clock.is_night() && clock.is_horde_night();
    let n = players.len() as u32;
    if !clock.is_night() {
        // Dans le brouillard, quelques zombies rôdent en plein jour.
        let alive = zombies.iter().count() as u32;
        waves.spawn_timer = 8.0;
        if alive < (6 + 4 * n).min(30) {
            let around = players[fastrand::usize(0..players.len())];
            for _ in 0..2 {
                if let Some(pos) = find_spawn_spot(&world, around, SPAWN_MIN_DIST, SPAWN_MAX_DIST) {
                    spawn_zombie(&mut commands, &mut ids, random_kind(clock.day), pos);
                }
            }
        }
        return;
    }
    let cap = if horde {
        (100 + 40 * n).min(320)
    } else {
        (20 + 10 * n + 4 * clock.day).min(150)
    };
    let alive = zombies.iter().count() as u32;
    waves.spawn_timer = if horde { 1.5 } else { 4.0 };
    if waves.night != clock.day {
        waves.night = clock.day;
        waves.spawned = 0;
    }
    let left = night_budget(clock.day, n, horde).saturating_sub(waves.spawned);
    if alive >= cap || left == 0 {
        return;
    }
    let group = (if horde { 8 } else { 3 } + fastrand::u32(0..4))
        .min(cap - alive)
        .min(left);
    waves.spawned += group;
    let around = players[fastrand::usize(0..players.len())];
    // Chaque grande horde est menée par un colosse.
    if horde
        && waves.colossus_day != Some(clock.day)
        && let Some(pos) = find_spawn_spot(&world, around, SPAWN_MIN_DIST, SPAWN_MAX_DIST)
    {
        waves.colossus_day = Some(clock.day);
        spawn_zombie(&mut commands, &mut ids, ZombieKind::Colossus, pos);
        if let Some(server) = server.as_deref_mut() {
            server.broadcast_message(
                DefaultChannel::ReliableOrdered,
                encode(&ServerMessage::Chat {
                    from: "Serveur".into(),
                    text: "Un colosse mène la horde !".into(),
                }),
            );
        }
    }
    for _ in 0..group {
        if let Some(pos) = find_spawn_spot(&world, around, SPAWN_MIN_DIST, SPAWN_MAX_DIST) {
            spawn_zombie(&mut commands, &mut ids, random_kind(clock.day), pos);
        }
    }
}

fn zombie_ai(
    time: Res<Time>,
    clock: Res<Clock>,
    flow: Res<Flow>,
    mut world: ResMut<GameWorld>,
    mut version: ResMut<WorldVersion>,
    mut damage: ResMut<BlockDamage>,
    mut lobby: ResMut<Lobby>,
    mut server: Option<ResMut<RenetServer>>,
    mut zombies: Query<(&mut Zombie, &mut ZombieBody)>,
    mut chests: ResMut<Chests>,
    mut survivors: Query<(Entity, &mut Survivor)>,
) {
    let dt = time.delta_secs();
    let players: Vec<(ClientId, Vec3)> = lobby
        .players
        .iter()
        .filter(|(_, p)| p.is_alive())
        .map(|(id, p)| (*id, p.state.position))
        .collect();
    let survivor_spots: Vec<(Entity, Vec3)> = survivors.iter().map(|(e, s)| (e, s.position)).collect();
    // Positions de tous les zombies, pour savoir qui se presse contre un mur.
    let crowd: Vec<(u32, Vec3)> = zombies.iter().map(|(z, b)| (z.id, b.position)).collect();
    let stack = stacking(clock.day);
    let mut survivor_hits: Vec<(Entity, f32)> = Vec::new();

    for (mut zombie, mut body) in &mut zombies {
        let stats = zombie.kind.stats();
        zombie.attack_cooldown -= dt;
        zombie.attacking -= dt;
        zombie.hurt_timer -= dt;

        // Debout sur des piques : le zombie se blesse.
        let below = block_containing(body.position - Vec3::Y * 0.1);
        if body.on_ground && world.block(below) == Block::Spikes {
            zombie.health -= SPIKE_DPS * dt;
            zombie.hurt_timer = zombie.hurt_timer.max(0.1);
        }

        // Dans les barbelés : le zombie se blesse, ralentit et use le fil.
        let feet_block = block_containing(body.position + Vec3::Y * 0.1);
        let in_wire = world.block(feet_block) == Block::BarbedWire;
        if in_wire {
            zombie.health -= WIRE_DPS * dt;
            zombie.hurt_timer = zombie.hurt_timer.max(0.1);
            damage_block(
                feet_block,
                WIRE_WEAR * dt,
                &mut damage,
                &mut world,
                &mut version,
                server.as_deref_mut(),
                &mut lobby,
                &mut chests,
            );
        }

        let nearest = players
            .iter()
            .map(|(id, p)| (*id, *p, p.distance(body.position)))
            .min_by(|a, b| a.2.total_cmp(&b.2));

        let mut wish = Vec3::ZERO;
        let mut face: Option<Vec3> = None;
        let mut climbing = false;
        let mut at_wall = false;
        // Un survivant proche, plus proche que tout joueur, est pris pour cible.
        let survivor = survivor_spots
            .iter()
            .map(|(e, p)| (*e, *p, p.distance(body.position)))
            .filter(|(_, _, d)| *d < SURVIVOR_AGGRO)
            .min_by(|a, b| a.2.total_cmp(&b.2))
            .filter(|(_, _, d)| nearest.is_none_or(|n| *d < n.2));
        if let Some((entity, target, dist)) = survivor {
            let vertical_ok = (target.y - body.position.y).abs() < 1.6;
            if dist < ZOMBIE_REACH + stats.size.half_width && vertical_ok {
                face = Some(target);
                if zombie.attack_cooldown <= 0.0 {
                    zombie.attack_cooldown = ZOMBIE_ATTACK_COOLDOWN;
                    zombie.attacking = 0.4;
                    survivor_hits.push((entity, stats.damage));
                }
            } else {
                wish = Vec3::new(target.x - body.position.x, 0.0, target.z - body.position.z);
            }
        } else if let Some((player_id, player_pos, dist)) = nearest {
            let feet = block_containing(body.position + Vec3::Y * 0.1);
            let vertical_ok = (player_pos.y - body.position.y).abs() < 1.6;
            let mouth = body.position + Vec3::Y * 1.5;
            let aim = player_pos + Vec3::Y * 1.2;
            let spits = zombie.kind == ZombieKind::Spitter
                && dist > 3.0
                && dist < SPIT_RANGE
                && crate::survivors::clear_line(&world, mouth, aim);
            if spits {
                // Le cracheur s'arrête à distance et crache.
                face = Some(player_pos);
                if zombie.attack_cooldown <= 0.0 {
                    zombie.attack_cooldown = SPIT_COOLDOWN;
                    zombie.attacking = 0.4;
                    if let Some(player) = lobby.players.get_mut(&player_id) {
                        let (amount, _) = player.incoming_damage(SPIT_DAMAGE, body.position);
                        hurt_player(
                            player_id,
                            player,
                            amount,
                            death_cause(zombie.kind),
                            server.as_deref_mut(),
                        );
                    }
                    if let Some(server) = server.as_deref_mut() {
                        server.broadcast_message(
                            DefaultChannel::ReliableOrdered,
                            encode(&ServerMessage::Acid {
                                origin: mouth,
                                end: aim,
                            }),
                        );
                    }
                }
            } else if dist < ZOMBIE_REACH + stats.size.half_width && vertical_ok {
                // Au contact : frappe le joueur.
                face = Some(player_pos);
                if zombie.attack_cooldown <= 0.0 {
                    zombie.attack_cooldown = ZOMBIE_ATTACK_COOLDOWN;
                    zombie.attacking = 0.4;
                    if let Some(player) = lobby.players.get_mut(&player_id) {
                        let (amount, parried) = player.incoming_damage(stats.damage, body.position);
                        if parried {
                            // Parade : le zombie est repoussé et met du temps à revenir.
                            let back = (body.position - player_pos).with_y(0.0).normalize_or_zero();
                            zombie.knockback = back * 8.0 + Vec3::Y * 3.0;
                            zombie.attack_cooldown += PARRY_STUN;
                            if let Some(server) = server.as_deref_mut() {
                                server.send_message(
                                    player_id,
                                    DefaultChannel::ReliableOrdered,
                                    encode(&ServerMessage::Parried),
                                );
                            }
                        } else {
                            // Une morsure qui traverse l'armure peut infecter.
                            if amount > 0.0
                                && player.infection <= 0.0
                                && fastrand::f32() < shared::climate::INFECTION_CHANCE
                            {
                                player.infection = 1.0;
                            }
                            hurt_player(
                                player_id,
                                player,
                                amount,
                                death_cause(zombie.kind),
                                server.as_deref_mut(),
                            );
                        }
                    }
                }
            } else if let Some(node) = flow.field.get(match zombie.climb_from {
                // En pleine escalade, il suit toujours le chemin du sol d'où il est parti.
                Some(y0) if y0.is_finite() => IVec3::new(feet.x, y0.floor() as i32, feet.z),
                _ => feet,
            }) {
                let wall = node.breaks.filter(|b| world.is_solid(*b));
                let wall_close = wall.is_some_and(|b| {
                    let c = b.as_vec3() + Vec3::splat(0.5);
                    Vec2::new(c.x - body.position.x, c.z - body.position.z).length() < 1.3
                });
                // Serrés contre le mur, les zombies grimpent les uns sur les autres (de plus en plus haut au fil des nuits).
                let pressed = crowd
                    .iter()
                    .filter(|(id, p)| *id != zombie.id && p.distance(body.position) < 1.8)
                    .count();
                at_wall = wall.is_some() && wall_close;
                let climbed = body.position.y - zombie.climb_from.unwrap_or(body.position.y);
                let can_climb = stack.is_some_and(|(needed, height)| {
                    if climbed >= height {
                        // Pile trop basse pour ce mur : il redescend et tape dedans.
                        zombie.climb_from = Some(f32::NEG_INFINITY);
                    }
                    pressed >= needed && climbed < height
                });
                if let (Some(block_pos), true, true) = (wall, wall_close, can_climb) {
                    climbing = true;
                    zombie.climb_from.get_or_insert(body.position.y);
                    let c = block_pos.as_vec3() + Vec3::splat(0.5);
                    wish = Vec3::new(c.x - body.position.x, 0.0, c.z - body.position.z);
                } else if let (Some(block_pos), true) = (wall, wall_close) {
                    // Contre le mur à casser : on tape dedans.
                    face = Some(block_pos.as_vec3() + Vec3::splat(0.5));
                    zombie.attacking = 0.4;
                    let wall_block = world.block(block_pos);
                    if wall_block == Block::Spikes {
                        zombie.health -= SPIKE_DPS * dt;
                        zombie.hurt_timer = zombie.hurt_timer.max(0.1);
                    }
                    damage_block(
                        block_pos,
                        stats.block_dps * dt,
                        &mut damage,
                        &mut world,
                        &mut version,
                        server.as_deref_mut(),
                        &mut lobby,
                        &mut chests,
                    );
                } else {
                    let target = node.next.as_vec3() + Vec3::new(0.5, 0.0, 0.5);
                    wish = Vec3::new(target.x - body.position.x, 0.0, target.z - body.position.z);
                    if node.next == feet {
                        wish = Vec3::new(player_pos.x - body.position.x, 0.0, player_pos.z - body.position.z);
                    }
                }
            } else if dist < 64.0 {
                // Hors du champ : droit sur le joueur.
                wish = Vec3::new(player_pos.x - body.position.x, 0.0, player_pos.z - body.position.z);
            }
        }

        let slow = if in_wire { WIRE_SLOW } else { 1.0 };
        let mut wish = wish.normalize_or_zero() * stats.speed * slow;
        // Coincé (fosse, marche trop haute, branches basses) : il escalade d'abord, puis casse ce qui le bloque.
        let mut scrambling = false;
        if zombie.stuck > STUCK_SECS && !climbing && face.is_none() {
            if wish.length() < 0.1
                && let Some((_, player_pos, _)) = nearest
            {
                wish = (player_pos - body.position).with_y(0.0).normalize_or_zero() * stats.speed;
            }
            // On n'escalade que contre une paroi, jamais dans le vide.
            let against = [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z].into_iter().any(|d| {
                world.is_solid(block_containing(
                    body.position + d * (stats.size.half_width + 0.3) + Vec3::Y * 0.5,
                ))
            });
            scrambling = against && zombie.stuck < STUCK_SECS * 3.0;
        }
        if zombie.stuck > STUCK_SECS * 3.0 && wish != Vec3::ZERO && !climbing {
            let dir = wish.normalize();
            let reach = body.position + dir * (stats.size.half_width + 0.45);
            let blocker = [0.2, 1.2, 1.9]
                .into_iter()
                .map(|dy| block_containing(reach + Vec3::Y * dy))
                .find(|p| p.y > 0 && world.is_solid(*p) && world.block(*p).max_health() > 0);
            if let Some(block_pos) = blocker {
                face = Some(block_pos.as_vec3() + Vec3::splat(0.5));
                zombie.attacking = 0.4;
                damage_block(
                    block_pos,
                    stats.block_dps * dt,
                    &mut damage,
                    &mut world,
                    &mut version,
                    server.as_deref_mut(),
                    &mut lobby,
                    &mut chests,
                );
            }
        }
        if let Some(target) = face {
            body.yaw = (body.position.x - target.x).atan2(body.position.z - target.z);
        } else if wish != Vec3::ZERO {
            body.yaw = (-wish.x).atan2(-wish.z);
        }
        let mut velocity = Vec3::new(wish.x, body.velocity.y, wish.z) + zombie.knockback;
        zombie.knockback = Vec3::ZERO;
        if body.on_ground && body.hit_wall && wish != Vec3::ZERO {
            velocity.y = JUMP_SPEED;
        }
        if climbing || scrambling {
            velocity.y = velocity.y.max(CLIMB_SPEED);
        } else if body.on_ground && !at_wall {
            zombie.climb_from = None;
        }
        let result = step_body(body.position, velocity, stats.size, dt, &|p| {
            world.is_solid(p) || p.y < 0
        });
        // Avance réelle comparée à l'avance voulue.
        let moved = (result.position - body.position).with_y(0.0).length();
        let wanted = wish.with_y(0.0).length() * dt;
        let blocked = wanted > 0.0 && moved < wanted * 0.2;
        // Rester sur place en voulant monter (fond d'une fosse) compte aussi.
        let pinned = face.is_none()
            && nearest.is_some_and(|n| n.2 > ZOMBIE_REACH + 1.0 && n.2 < 64.0)
            && moved < 0.01
            && wanted == 0.0;
        if (blocked || pinned) && !climbing {
            zombie.stuck += dt;
        } else {
            zombie.stuck = (zombie.stuck - dt * 2.0).max(0.0);
        }
        body.position = result.position;
        body.velocity = Vec3::new(0.0, result.velocity.y, 0.0);
        body.on_ground = result.on_ground;
        body.hit_wall = result.hit_wall;
    }
    for (entity, amount) in survivor_hits {
        if let Ok((_, mut survivor)) = survivors.get_mut(entity) {
            survivor.hurt(amount);
        }
    }
}

/// Temps passé à pousser sans avancer avant qu'un zombie ne casse l'obstacle.
const STUCK_SECS: f32 = 1.5;

/// Distance à laquelle un zombie s'en prend à un survivant plutôt qu'au joueur le plus proche.
const SURVIVOR_AGGRO: f32 = 12.0;

/// Dégâts par seconde des piques aux zombies qui les touchent.
const SPIKE_DPS: f32 = 12.0;
/// Vitesse à laquelle un zombie grimpe sur les autres.
const CLIMB_SPEED: f32 = 2.5;
/// Temps d'étourdissement d'un zombie dont le coup a été paré.
const PARRY_STUN: f32 = 1.2;
/// Dégâts par seconde des barbelés, usure du fil et ralentissement.
const WIRE_DPS: f32 = 8.0;
const WIRE_WEAR: f32 = 6.0;
const WIRE_SLOW: f32 = 0.35;

/// Inflige `amount` points de dégâts à un bloc ; renvoie vrai s'il est détruit.
/// Les fissures sont diffusées par huitièmes, pour ne pas inonder le réseau.
#[allow(clippy::too_many_arguments)]
pub(crate) fn damage_block(
    pos: IVec3,
    amount: f32,
    damage: &mut BlockDamage,
    world: &mut GameWorld,
    version: &mut WorldVersion,
    mut server: Option<&mut RenetServer>,
    lobby: &mut Lobby,
    chests: &mut Chests,
) -> bool {
    let block = world.block(pos);
    if block == Block::Air {
        damage.0.remove(&pos);
        return false;
    }
    let hp = block.max_health() as f32;
    let entry = damage.0.entry(pos).or_insert((block, 0.0));
    if entry.0 != block {
        *entry = (block, 0.0);
    }
    let dealt = &mut entry.1;
    let step_before = (*dealt / hp * 8.0) as u32;
    *dealt += amount;
    if *dealt < hp {
        if (*dealt / hp * 8.0) as u32 != step_before
            && let Some(server) = server.as_deref_mut()
        {
            server.broadcast_message(
                DefaultChannel::ReliableOrdered,
                encode(&ServerMessage::BlockDamage {
                    pos,
                    damage: (*dealt / hp * 255.0) as u8,
                }),
            );
        }
        return false;
    }
    damage.0.remove(&pos);
    if !world.set_block(pos, Block::Air) {
        return false;
    }
    version.0 += 1;
    if let Some(server) = server {
        if block.is_storage() {
            // Le coffre (ou la tourelle) est détruit avec son contenu.
            crate::net::chest_destroyed(server, lobby, chests, pos, None);
        }
        server.broadcast_message(
            DefaultChannel::ReliableOrdered,
            encode(&ServerMessage::BlockChanged { pos, block: Block::Air }),
        );
    } else if block.is_storage() {
        chests.0.remove(&pos);
    }
    true
}

/// Retire de la vie à un joueur et annonce sa mort si besoin.
pub fn hurt_player(
    id: ClientId,
    player: &mut crate::net::ConnectedPlayer,
    amount: f32,
    cause: &str,
    server: Option<&mut RenetServer>,
) {
    if player.health <= 0.0 {
        return; // déjà mort (plusieurs zombies peuvent frapper dans le même tick)
    }
    let before = player.health.ceil();
    player.health = (player.health - amount).max(0.0);
    player.since_hit = 0.0;
    let Some(server) = server else { return };
    if player.health.ceil() != before || player.health <= 0.0 {
        send_vitals(server, id, player);
    }
    if player.health <= 0.0 {
        info!("{} est mort : {cause}", player.name);
        player.mining = None;
        server.send_message(
            id,
            DefaultChannel::ReliableOrdered,
            encode(&ServerMessage::Died { cause: cause.into() }),
        );
        server.broadcast_message(
            DefaultChannel::ReliableOrdered,
            encode(&ServerMessage::Chat {
                from: "Serveur".into(),
                text: format!("{} : {cause}", player.name),
            }),
        );
    }
}

fn death_cause(kind: ZombieKind) -> &'static str {
    match kind {
        ZombieKind::Walker => "Dévoré par un zombie",
        ZombieKind::Runner => "Rattrapé par un coureur",
        ZombieKind::Brute => "Écrasé par une brute",
        ZombieKind::Spitter => "Rongé par l'acide d'un cracheur",
        ZombieKind::Colossus => "Piétiné par le colosse",
    }
}

/// Écarte les zombies trop proches les uns des autres (grille spatiale, pas de comparaison tous contre tous).
fn separate_zombies(world: Res<GameWorld>, mut bodies: Query<(Entity, &Zombie, &mut ZombieBody)>) {
    let mut grid: HashMap<(i32, i32), Vec<(Entity, Vec3, f32)>> = HashMap::new();
    for (entity, zombie, body) in &bodies {
        let key = (body.position.x.floor() as i32, body.position.z.floor() as i32);
        grid.entry(key)
            .or_default()
            .push((entity, body.position, zombie.kind.stats().size.half_width));
    }
    for (entity, zombie, mut body) in &mut bodies {
        let r = zombie.kind.stats().size.half_width;
        let (cx, cz) = (body.position.x.floor() as i32, body.position.z.floor() as i32);
        let mut push = Vec3::ZERO;
        for dx in -1..=1 {
            for dz in -1..=1 {
                let Some(cell) = grid.get(&(cx + dx, cz + dz)) else {
                    continue;
                };
                for (other, pos, other_r) in cell {
                    if *other == entity {
                        continue;
                    }
                    let d = Vec3::new(body.position.x - pos.x, 0.0, body.position.z - pos.z);
                    let min = r + other_r;
                    let len = d.length();
                    if len < min && (pos.y - body.position.y).abs() < 1.5 {
                        let dir = if len > 1e-3 {
                            d / len
                        } else {
                            Vec3::new(entity.index_u32() as f32 % 2.0 - 0.5, 0.0, 0.5).normalize()
                        };
                        push += dir * (min - len) * 0.5;
                    }
                }
            }
        }
        if push != Vec3::ZERO {
            let size = zombie.kind.stats().size;
            let next = body.position + push.clamp_length_max(0.2);
            if !shared::physics::overlaps_solid(next, size, &|p| world.is_solid(p)) {
                body.position = next;
            }
        }
    }
}

fn sun_and_cleanup(
    mut commands: Commands,
    time: Res<Time>,
    clock: Res<Clock>,
    waves: Res<Waves>,
    lobby: Res<Lobby>,
    mut zombies: Query<(Entity, &mut Zombie, &ZombieBody)>,
    weather: Option<Res<crate::events::Weather>>,
) {
    let fog = weather.is_some_and(|w| w.fog);
    let players: Vec<Vec3> = lobby
        .players
        .values()
        .filter(|p| p.is_ready())
        .map(|p| p.state.position)
        .collect();
    for (entity, mut zombie, body) in &mut zombies {
        // Le brouillard cache le soleil : les zombies ne brûlent pas.
        if !clock.is_night() && !waves.stress && !fog {
            zombie.health -= SUN_DPS * time.delta_secs();
        }
        let far = players.iter().all(|p| p.distance(body.position) > DESPAWN_DIST);
        if (far && !waves.stress) || body.position.y < -10.0 {
            commands.entity(entity).despawn();
        }
    }
}

fn kill_dead_zombies(
    mut commands: Commands,
    zombies: Query<(Entity, &Zombie)>,
    mut server: Option<ResMut<RenetServer>>,
) {
    for (entity, zombie) in &zombies {
        if zombie.health <= 0.0 {
            commands.entity(entity).despawn();
            if let Some(server) = server.as_deref_mut() {
                server.broadcast_message(
                    DefaultChannel::ReliableOrdered,
                    encode(&ServerMessage::ZombieKilled {
                        id: zombie.id,
                        by: zombie.last_attacker,
                    }),
                );
            }
        }
    }
}

/// Faim perdue par seconde : un joueur rassasié tient environ 15 minutes.
const HUNGER_PER_SEC: f32 = MAX_HUNGER / 900.0;
/// Vie perdue par seconde quand la faim est à zéro.
const STARVATION_DPS: f32 = 1.0;
/// En dessous de cette faim, la vie ne remonte plus.
const REGEN_MIN_HUNGER: f32 = 30.0;
/// Soif perdue par seconde : un joueur désaltéré tient environ 10 minutes.
const THIRST_PER_SEC: f32 = MAX_THIRST / 600.0;
/// Vie perdue par seconde quand la soif est à zéro.
const DEHYDRATION_DPS: f32 = 1.0;
/// En dessous de cette soif, la vie ne remonte plus.
const REGEN_MIN_THIRST: f32 = 20.0;

/// Faim, soif et régénération : 2 points de vie par seconde après 8 secondes sans coup,
/// si le joueur a mangé et bu. Les anciens mondes, sans lacs, ne donnent pas soif.
fn player_survival(
    time: Res<Time>,
    clock: Res<Clock>,
    world: Res<GameWorld>,
    mut lobby: ResMut<Lobby>,
    mut server: Option<ResMut<RenetServer>>,
) {
    use shared::climate::*;
    let dt = time.delta_secs();
    let thirsty = world.generator().water_level().is_some();
    for (id, player) in lobby.players.iter_mut() {
        player.attack_cooldown -= dt;
        if let Some(t) = player.guarding.as_mut() {
            *t += dt;
        }
        if !player.is_alive() {
            continue;
        }
        let hunger_before = player.hunger.ceil();
        let thirst_before = player.thirst.ceil();
        let temp_before = (player.body_temp * 10.0).round();
        let felt_before = player.felt.round();
        let infection_before = player.infection.ceil();

        // Température : l'air autour, les vêtements, puis le corps qui suit lentement.
        let around = surroundings(&world, player.state.position, clock.0.is_night());
        let air = ambient(
            world.generator().biome(
                player.state.position.x.floor() as i32,
                player.state.position.z.floor() as i32,
            ),
            around,
        );
        let (warmth, cooling) = player.inventory.clothing();
        player.felt = felt(air, warmth, cooling);
        player.body_temp = shared::climate::step_body(player.body_temp, player.felt, dt);

        if player.infection > 0.0 {
            player.infection = (player.infection + INFECTION_PER_SEC * dt).min(100.0);
        }

        player.hunger = (player.hunger - HUNGER_PER_SEC * dt).max(0.0);
        if thirsty {
            let heat = if player.body_temp > OVERHEATING { 2.0 } else { 1.0 };
            player.thirst = (player.thirst - THIRST_PER_SEC * heat * dt).max(0.0);
        }
        let infection_dps = if player.infection >= 100.0 {
            INFECTION_DPS * 2.0
        } else if player.infection >= INFECTION_HARMFUL {
            INFECTION_DPS
        } else {
            0.0
        };
        let deprived = if player.hunger <= 0.0 {
            Some((STARVATION_DPS, "Mort de faim"))
        } else if player.thirst <= 0.0 {
            Some((DEHYDRATION_DPS, "Mort de soif"))
        } else if player.body_temp < HYPOTHERMIA {
            Some((EXPOSURE_DPS, "Mort de froid"))
        } else if player.body_temp > HEATSTROKE {
            Some((EXPOSURE_DPS, "Terrassé par la chaleur"))
        } else if infection_dps > 0.0 {
            Some((infection_dps, "Emporté par l'infection"))
        } else {
            None
        };
        let vitals_changed = |p: &crate::net::ConnectedPlayer| {
            p.hunger.ceil() != hunger_before
                || p.thirst.ceil() != thirst_before
                || (p.body_temp * 10.0).round() != temp_before
                || p.felt.round() != felt_before
                || p.infection.ceil() != infection_before
        };
        if let Some((dps, cause)) = deprived {
            let since_hit = player.since_hit;
            hurt_player(*id, player, dps * dt, cause, server.as_deref_mut());
            player.since_hit = since_hit + dt;
            if vitals_changed(player)
                && let Some(server) = server.as_deref_mut()
            {
                send_vitals(server, *id, player);
            }
            continue;
        }
        player.since_hit += dt;
        let health_before = player.health.ceil();
        if player.since_hit > 8.0
            && player.health < MAX_HEALTH
            && player.hunger > REGEN_MIN_HUNGER
            && player.thirst > REGEN_MIN_THIRST
            && player.infection <= 0.0
        {
            player.health = (player.health + 2.0 * dt).min(MAX_HEALTH);
        }
        if (player.health.ceil() != health_before || vitals_changed(player))
            && let Some(server) = server.as_deref_mut()
        {
            send_vitals(server, *id, player);
        }
    }
}

/// Ce qui réchauffe ou refroidit un joueur à cette position.
fn surroundings(world: &GameWorld, position: Vec3, night: bool) -> shared::climate::Surroundings {
    let feet = block_containing(position + Vec3::Y * 0.1);
    let head = feet + IVec3::Y;
    let near_fire = (-3..=3)
        .any(|dx| (-2..=2).any(|dy| (-3..=3).any(|dz| world.block(feet + IVec3::new(dx, dy, dz)) == Block::Campfire)));
    let sheltered = (1..=6).any(|dy| world.block(head + IVec3::Y * dy).is_opaque());
    let in_water = world.block(feet).is_water();
    shared::climate::Surroundings {
        night,
        near_fire,
        sheltered,
        in_water,
    }
}

/// Envoie à chaque joueur les zombies proches de lui, 15 fois par seconde.
fn broadcast_zombies(
    mut server: Option<ResMut<RenetServer>>,
    lobby: Res<Lobby>,
    zombies: Query<(&Zombie, &ZombieBody)>,
    mut tick: Local<u32>,
) {
    *tick += 1;
    if !tick.is_multiple_of(2) {
        return;
    }
    // Les zombies lointains ne sont envoyés que 5 fois par seconde : de loin, ça ne se voit pas.
    let far_too = tick.is_multiple_of(6);
    let Some(server) = server.as_deref_mut() else { return };
    for (id, player) in lobby.players.iter().filter(|(_, p)| p.is_ready()) {
        let states: Vec<ZombieState> = zombies
            .iter()
            .filter(|(_, b)| {
                let d = b.position.distance(player.state.position);
                d < NEAR_DIST || (far_too && d < VIEW_DIST)
            })
            .map(|(z, b)| {
                let mut flags = 0;
                if z.attacking > 0.0 {
                    flags |= ZombieState::ATTACKING;
                }
                if z.hurt_timer > 0.0 {
                    flags |= ZombieState::HURT;
                }
                if z.climb_from.is_some_and(f32::is_finite) {
                    flags |= ZombieState::CLIMBING;
                }
                ZombieState::new(z.id, b.position, b.yaw, z.kind, flags, z.health)
            })
            .collect();
        // Découpe en paquets raisonnables pour le canal non fiable.
        if states.is_empty() {
            server.send_message(
                *id,
                DefaultChannel::Unreliable,
                encode(&ServerMessage::Zombies(Vec::new())),
            );
        }
        for batch in states.chunks(80) {
            server.send_message(
                *id,
                DefaultChannel::Unreliable,
                encode(&ServerMessage::Zombies(batch.to_vec())),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use shared::config::WorldConfig;
    use shared::daytime::GameTime;
    use shared::world::WorldState;

    use super::*;
    use crate::net::ConnectedPlayer;

    /// Une plateforme dans le ciel coupée par un mur renforcé de deux blocs ; le joueur est derrière.
    fn arena(night: u32, zombies: usize) -> (App, i32) {
        let mut world = GameWorld(WorldState::new(WorldConfig::default()));
        let y = world.generator().surface_height(100, 100) + 25;
        let wall_z = 110;
        for x in 95..=105 {
            for z in 95..=125 {
                world.set_block(IVec3::new(x, y - 1, z), Block::Stone);
            }
            for dy in 0..2 {
                world.set_block(IVec3::new(x, y + dy, wall_z), Block::ReinforcedWall);
            }
        }
        let mut lobby = Lobby::default();
        let mut player = ConnectedPlayer::new(Vec3::new(100.5, y as f32, 120.5));
        player.name = "Zoé".into();
        lobby.players.insert(1, player);
        let mut app = App::new();
        app.insert_resource(world)
            .insert_resource(lobby)
            .insert_resource(Clock(GameTime { day: night, phase: 0.9 }))
            .init_resource::<Time>()
            .init_resource::<Flow>()
            .init_resource::<BlockDamage>()
            .init_resource::<WorldVersion>()
            .init_resource::<Chests>()
            .init_resource::<TickStats>()
            .add_systems(Update, (update_flow_field, zombie_ai, separate_zombies).chain());
        for i in 0..zombies {
            let pos = Vec3::new(97.5 + (i % 6) as f32, y as f32, 107.0 + (i / 6) as f32 * 0.6);
            let id = i as u32 + 1;
            app.world_mut().spawn((
                Zombie {
                    id,
                    kind: ZombieKind::Walker,
                    health: 1000.0,
                    attack_cooldown: 0.0,
                    attacking: 0.0,
                    hurt_timer: 0.0,
                    knockback: Vec3::ZERO,
                    last_attacker: None,
                    climb_from: None,
                    stuck: 0.0,
                },
                ZombieBody {
                    position: pos,
                    velocity: Vec3::ZERO,
                    yaw: 0.0,
                    on_ground: false,
                    hit_wall: false,
                },
            ));
        }
        (app, wall_z)
    }

    fn crossed_after(app: &mut App, wall_z: i32, secs: f32) -> usize {
        for _ in 0..(secs * 30.0) as usize {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::from_secs_f32(1.0 / 30.0));
            app.update();
        }
        app.world_mut()
            .query::<&ZombieBody>()
            .iter(app.world())
            .filter(|b| b.position.z > wall_z as f32 + 1.0)
            .count()
    }

    #[test]
    fn nights_send_a_bounded_but_growing_number_of_zombies() {
        assert!(night_budget(1, 1, false) < night_budget(6, 1, false));
        assert!(night_budget(6, 1, false) < night_budget(6, 4, false));
        assert!(night_budget(7, 1, true) > 2 * night_budget(6, 1, false));
        assert!(night_budget(1, 1, false) <= 40, "première nuit trop chargée");
    }

    #[test]
    fn zombies_reach_a_player_across_natural_terrain() {
        // Sur le vrai terrain autour du point de départ (arbres, marches, rives), aucun zombie
        // ne doit rester coincé : ils cassent ce qui les bloque.
        let world = GameWorld(WorldState::new(WorldConfig::default()));
        let spawn = crate::world::find_spawn(&world);
        let mut lobby = Lobby::default();
        let mut player = ConnectedPlayer::new(spawn);
        player.name = "Zoé".into();
        player.health = f32::MAX;
        lobby.players.insert(1, player);
        let mut app = App::new();
        app.insert_resource(Clock(GameTime { day: 1, phase: 0.9 }))
            .insert_resource(lobby)
            .init_resource::<Time>()
            .init_resource::<Flow>()
            .init_resource::<BlockDamage>()
            .init_resource::<WorldVersion>()
            .init_resource::<Chests>()
            .init_resource::<TickStats>()
            .add_systems(Update, (update_flow_field, zombie_ai, separate_zombies).chain());
        let mut ids = NextZombieId::default();
        fastrand::seed(7);
        let mut spawned = 0;
        for _ in 0..200 {
            if spawned == 16 {
                break;
            }
            if let Some(pos) = find_spawn_spot(&world, spawn, 20.0, 30.0) {
                let mut commands = app.world_mut().commands();
                spawn_zombie(&mut commands, &mut ids, ZombieKind::Walker, pos);
                spawned += 1;
            }
        }
        app.insert_resource(world);
        app.world_mut().flush();
        for _ in 0..(90.0 * 30.0) as usize {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::from_secs_f32(1.0 / 30.0));
            app.update();
        }
        let far: Vec<Vec3> = app
            .world_mut()
            .query::<&ZombieBody>()
            .iter(app.world())
            .map(|b| b.position)
            .filter(|p| p.distance(spawn) > 4.0)
            .collect();
        assert!(
            far.len() <= 2,
            "{} zombies sur {spawned} sont restés loin : {far:?}",
            far.len()
        );
    }

    #[test]
    fn crowded_zombies_climb_over_walls_on_later_nights() {
        // Nuit 6 : une dizaine de zombies serrés passent par-dessus le mur.
        let (mut app, wall_z) = arena(6, 12);
        assert!(crossed_after(&mut app, wall_z, 20.0) > 0, "personne n'a franchi le mur");
        // Première nuit : ils restent bloqués à taper dedans.
        let (mut app, wall_z) = arena(1, 12);
        assert_eq!(crossed_after(&mut app, wall_z, 20.0), 0);
    }
}

#[cfg(test)]
mod climate_tests {
    use shared::config::WorldConfig;
    use shared::daytime::GameTime;
    use shared::item::Item;
    use shared::world::WorldState;
    use shared::worldgen::Biome;

    use super::*;
    use crate::net::ConnectedPlayer;

    /// Un joueur seul, la nuit, à l'air libre dans la neige.
    fn snowy_night() -> (App, Vec3) {
        let world = GameWorld(WorldState::new(WorldConfig::default()));
        let g = world.generator();
        let size = g.config().size_blocks();
        let (x, z) = (8..size - 8)
            .step_by(4)
            .flat_map(|x| (8..size - 8).step_by(4).map(move |z| (x, z)))
            .find(|(x, z)| g.biome(*x, *z) == Biome::Snow)
            .expect("le monde par défaut a de la neige");
        let position = Vec3::new(x as f32 + 0.5, g.surface_height(x, z) as f32 + 1.0, z as f32 + 0.5);
        let mut player = ConnectedPlayer::new(position);
        player.name = "Alice".into();
        let mut lobby = Lobby::default();
        lobby.players.insert(1, player);
        let mut app = App::new();
        app.insert_resource(world)
            .insert_resource(lobby)
            .insert_resource(Clock(GameTime { day: 1, phase: 0.8 }))
            .init_resource::<Time>()
            .add_systems(Update, player_survival);
        (app, position)
    }

    fn run(app: &mut App, secs: f32) -> ConnectedPlayer {
        for _ in 0..(secs * 10.0) as usize {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::from_secs_f32(0.1));
            app.update();
        }
        app.world().resource::<Lobby>().players[&1].clone()
    }

    #[test]
    fn a_snowy_night_freezes_an_unprotected_player() {
        let (mut app, _) = snowy_night();
        let player = run(&mut app, 90.0);
        assert!(
            player.body_temp < shared::climate::HYPOTHERMIA,
            "{} °C",
            player.body_temp
        );
        assert!(player.health < MAX_HEALTH);
    }

    #[test]
    fn a_campfire_keeps_the_cold_away() {
        let (mut app, position) = snowy_night();
        let fire = block_containing(position) + IVec3::new(2, 0, 0);
        app.world_mut()
            .resource_mut::<GameWorld>()
            .set_block(fire, Block::Campfire);
        let player = run(&mut app, 90.0);
        assert_eq!(player.body_temp, shared::climate::BODY_NORMAL);
    }

    #[test]
    fn infection_spreads_blocks_healing_and_then_hurts() {
        let (mut app, position) = snowy_night();
        app.insert_resource(Clock(GameTime { day: 1, phase: 0.1 }));
        {
            let mut lobby = app.world_mut().resource_mut::<Lobby>();
            let player = lobby.players.get_mut(&1).unwrap();
            player.inventory.add(Item::FurCoat, 1);
            player.health = 50.0;
            player.infection = 55.0;
            player.since_hit = 100.0;
        }
        // Un abri contre le froid de la journée enneigée.
        app.world_mut()
            .resource_mut::<GameWorld>()
            .set_block(block_containing(position) + IVec3::new(0, 4, 0), Block::Stone);
        let player = run(&mut app, 30.0);
        assert!(player.health <= 50.0, "pas de guérison quand on est infecté");
        assert!(player.infection > 58.0);
        let player = run(&mut app, 60.0);
        assert!(
            player.health < 49.0,
            "l'infection avancée ronge la vie : {}",
            player.health
        );
    }
}

#[cfg(test)]
mod fog_tests {
    use shared::config::WorldConfig;
    use shared::daytime::GameTime;
    use shared::world::WorldState;

    use super::*;

    fn daylight_zombie(fog: bool) -> f32 {
        let mut app = App::new();
        app.insert_resource(GameWorld(WorldState::new(WorldConfig::default())))
            .insert_resource(Clock(GameTime { day: 3, phase: 0.2 }))
            .insert_resource(crate::events::Weather { fog })
            .init_resource::<Time>()
            .init_resource::<Waves>()
            .init_resource::<Lobby>()
            .add_systems(Update, sun_and_cleanup);
        // Un joueur tout près, pour que le zombie ne soit pas retiré.
        let mut player = crate::net::ConnectedPlayer::new(Vec3::new(52.0, 40.0, 50.0));
        player.name = "Alice".into();
        app.world_mut().resource_mut::<Lobby>().players.insert(1, player);
        let entity = app
            .world_mut()
            .spawn((
                Zombie {
                    id: 1,
                    kind: ZombieKind::Walker,
                    health: 100.0,
                    attack_cooldown: 0.0,
                    attacking: 0.0,
                    hurt_timer: 0.0,
                    knockback: Vec3::ZERO,
                    last_attacker: None,
                    climb_from: None,
                    stuck: 0.0,
                },
                ZombieBody {
                    position: Vec3::new(50.0, 40.0, 50.0),
                    velocity: Vec3::ZERO,
                    yaw: 0.0,
                    on_ground: true,
                    hit_wall: false,
                },
            ))
            .id();
        for _ in 0..30 {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::from_secs_f32(0.1));
            app.update();
        }
        app.world().get::<Zombie>(entity).unwrap().health
    }

    #[test]
    fn fog_hides_the_sun_from_zombies() {
        assert!(daylight_zombie(false) < 90.0);
        assert_eq!(daylight_zombie(true), 100.0);
    }
}
