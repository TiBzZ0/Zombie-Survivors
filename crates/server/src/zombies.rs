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
use shared::zombie::{ZOMBIE_ATTACK_COOLDOWN, ZOMBIE_REACH, ZombieKind};

use crate::daynight::Clock;
use crate::net::{Lobby, MAX_HEALTH, MAX_HUNGER, WorldVersion, send_vitals};
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
/// Dégâts du soleil par seconde.
const SUN_DPS: f32 = 6.0;
const JUMP_SPEED: f32 = 7.5;

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
    /// En mode test de charge, les zombies ne brûlent pas le jour.
    stress: bool,
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
        if world.is_solid(feet - IVec3::Y) && !world.is_solid(feet) && !world.is_solid(feet + IVec3::Y) {
            return Some(Vec3::new(x as f32 + 0.5, feet.y as f32, z as f32 + 0.5));
        }
    }
    None
}

fn random_kind(day: u32) -> ZombieKind {
    let r = fastrand::f32();
    let brute = (0.04 + day as f32 * 0.01).min(0.2);
    let runner = (0.15 + day as f32 * 0.015).min(0.35);
    if r < brute {
        ZombieKind::Brute
    } else if r < brute + runner {
        ZombieKind::Runner
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
) {
    if !clock.is_night() {
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
    let horde = clock.is_horde_night();
    let n = players.len() as u32;
    let cap = if horde {
        (150 + 50 * n).min(400)
    } else {
        (20 + 10 * n + 4 * clock.day).min(150)
    };
    let alive = zombies.iter().count() as u32;
    waves.spawn_timer = if horde { 1.5 } else { 4.0 };
    if alive >= cap {
        return;
    }
    let group = (if horde { 8 } else { 3 } + fastrand::u32(0..4)).min(cap - alive);
    let around = players[fastrand::usize(0..players.len())];
    for _ in 0..group {
        if let Some(pos) = find_spawn_spot(&world, around, SPAWN_MIN_DIST, SPAWN_MAX_DIST) {
            spawn_zombie(&mut commands, &mut ids, random_kind(clock.day), pos);
        }
    }
}

fn zombie_ai(
    time: Res<Time>,
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
            if dist < ZOMBIE_REACH + stats.size.half_width && vertical_ok {
                // Au contact : frappe le joueur.
                face = Some(player_pos);
                if zombie.attack_cooldown <= 0.0 {
                    zombie.attack_cooldown = ZOMBIE_ATTACK_COOLDOWN;
                    zombie.attacking = 0.4;
                    if let Some(player) = lobby.players.get_mut(&player_id) {
                        hurt_player(
                            player_id,
                            player,
                            stats.damage,
                            death_cause(zombie.kind),
                            server.as_deref_mut(),
                        );
                    }
                }
            } else if let Some(node) = flow.field.get(feet) {
                let wall = node.breaks.filter(|b| world.is_solid(*b));
                let wall_close = wall.is_some_and(|b| {
                    let c = b.as_vec3() + Vec3::splat(0.5);
                    Vec2::new(c.x - body.position.x, c.z - body.position.z).length() < 1.3
                });
                if let (Some(block_pos), true) = (wall, wall_close) {
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
        let wish = wish.normalize_or_zero() * stats.speed * slow;
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
        let result = step_body(body.position, velocity, stats.size, dt, &|p| {
            world.is_solid(p) || p.y < 0
        });
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

/// Distance à laquelle un zombie s'en prend à un survivant plutôt qu'au joueur le plus proche.
const SURVIVOR_AGGRO: f32 = 12.0;

/// Dégâts par seconde des piques aux zombies qui les touchent.
const SPIKE_DPS: f32 = 12.0;
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
) {
    let players: Vec<Vec3> = lobby
        .players
        .values()
        .filter(|p| p.is_ready())
        .map(|p| p.state.position)
        .collect();
    for (entity, mut zombie, body) in &mut zombies {
        if !clock.is_night() && !waves.stress {
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

/// Faim et régénération : 2 points de vie par seconde après 8 secondes sans coup, si le joueur a mangé.
fn player_survival(time: Res<Time>, mut lobby: ResMut<Lobby>, mut server: Option<ResMut<RenetServer>>) {
    let dt = time.delta_secs();
    for (id, player) in lobby.players.iter_mut() {
        player.attack_cooldown -= dt;
        if !player.is_alive() {
            continue;
        }
        let hunger_before = player.hunger.ceil();
        player.hunger = (player.hunger - HUNGER_PER_SEC * dt).max(0.0);
        if player.hunger <= 0.0 {
            let since_hit = player.since_hit;
            hurt_player(*id, player, STARVATION_DPS * dt, "Mort de faim", server.as_deref_mut());
            player.since_hit = since_hit + dt;
            continue;
        }
        player.since_hit += dt;
        let health_before = player.health.ceil();
        if player.since_hit > 8.0 && player.health < MAX_HEALTH && player.hunger > REGEN_MIN_HUNGER {
            player.health = (player.health + 2.0 * dt).min(MAX_HEALTH);
        }
        if (player.health.ceil() != health_before || player.hunger.ceil() != hunger_before)
            && let Some(server) = server.as_deref_mut()
        {
            send_vitals(server, *id, player);
        }
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
    let Some(server) = server.as_deref_mut() else { return };
    for (id, player) in lobby.players.iter().filter(|(_, p)| p.is_ready()) {
        let states: Vec<ZombieState> = zombies
            .iter()
            .filter(|(_, b)| b.position.distance(player.state.position) < VIEW_DIST)
            .map(|(z, b)| {
                let mut flags = 0;
                if z.attacking > 0.0 {
                    flags |= ZombieState::ATTACKING;
                }
                if z.hurt_timer > 0.0 {
                    flags |= ZombieState::HURT;
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
