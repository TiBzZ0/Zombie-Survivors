//! Survivants : PNJ humains à recruter. Ils suivent leur chef, gardent un poste,
//! récoltent des ressources ou réparent les murs, et se battent contre les zombies.

use std::collections::HashSet;

use bevy::prelude::*;
use bevy_renet::RenetServer;
use bevy_renet::renet::{ClientId, DefaultChannel};
use shared::block::Block;
use shared::item::{ArmorSlot, Item, armor_reduction};
use shared::physics::step_body;
use shared::protocol::{ServerMessage, SurvivorState, encode};
use shared::raycast::raycast;
use shared::survivor::{Order, RECRUIT_COST, SURVIVOR_SIZE, SURVIVOR_SPEED, SurvivorStats, TALK_RANGE, name_from_seed};
use shared::worldgen::RUIN_CELL;

use crate::daynight::Clock;
use crate::net::{Lobby, broadcast, send, send_chest_to_viewers, send_inventory};
use crate::save::{SavedSurvivor, SavedSurvivors};
use crate::world::{Chests, GameWorld};
use crate::zombies::{BlockDamage, Zombie, ZombieBody, ZombieSet, find_spawn_spot};

const JUMP_SPEED: f32 = 7.5;
/// Portée d'un coup de survivant, en mètres.
const ATTACK_RANGE: f32 = 1.7;
const ATTACK_COOLDOWN: f32 = 0.9;
/// Rayon défendu autour du survivant qui suit, ou autour de son poste.
const DEFEND_RADIUS: f32 = 12.0;
/// Un survivant sans chef ne se bat que si un zombie est tout près.
const WILD_DEFEND_RADIUS: f32 = 4.0;
/// Rayon de travail autour du poste (coffre, murs à réparer).
pub const WORK_RADIUS: f32 = 12.0;
/// Temps d'une récolte pour une habileté de 1 (divisé par la vitesse de travail).
pub const GATHER_SECS: f32 = 30.0;
/// Points de vie de bloc réparés par seconde, avant l'habileté.
const REPAIR_RATE: f32 = 15.0;
/// Au-delà, le survivant rejoint son chef d'un coup (il s'est perdu).
const FOLLOW_TELEPORT: f32 = 40.0;
const SPAWN_INTERVAL: f32 = 20.0;
const SPAWN_CHANCE: f32 = 0.4;
/// Rayon dans lequel un joueur reçoit les survivants.
const VIEW_DIST: f32 = 90.0;
/// Un survivant sans chef trop loin de tout joueur disparaît.
const WILD_DESPAWN: f32 = 160.0;

pub struct SurvivorPlugin;

impl Plugin for SurvivorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SurvivorTalk>()
            .init_resource::<NextSurvivorId>()
            .add_systems(Startup, restore_survivors)
            .add_systems(
                FixedUpdate,
                (
                    spawn_wanderers,
                    handle_talk,
                    survivor_ai,
                    survivor_deaths,
                    record_survivors,
                    broadcast_survivors,
                )
                    .chain()
                    .after(ZombieSet::Ai)
                    .before(ZombieSet::Deaths),
            );
    }
}

/// Demande d'un joueur à un survivant.
pub struct TalkRequest {
    pub client: ClientId,
    pub id: u32,
    /// Renvoie le survivant.
    pub dismiss: bool,
    /// Lui donne l'arme tenue en main.
    pub give: bool,
}

/// Demandes des joueurs aux survivants, traitées au tick suivant.
#[derive(Resource, Default)]
pub struct SurvivorTalk(pub Vec<TalkRequest>);

#[derive(Resource, Default)]
struct NextSurvivorId(u32);

/// Un survivant et son corps.
#[derive(Component, Debug)]
pub struct Survivor {
    pub id: u32,
    pub name: String,
    pub stats: SurvivorStats,
    pub health: f32,
    pub leader: Option<String>,
    pub order: Order,
    /// Arme confiée par son chef.
    pub weapon: Option<Item>,
    /// Casque et armure confiés par son chef.
    pub armor: [Option<Item>; 2],
    /// Endroit gardé, récolté ou réparé (son camp s'il n'a pas de chef).
    pub post: Vec3,
    pub position: Vec3,
    velocity: Vec3,
    yaw: f32,
    on_ground: bool,
    hit_wall: bool,
    attack_cooldown: f32,
    attacking: f32,
    hurt_timer: f32,
    since_hit: f32,
    /// Progression de la récolte en cours, en secondes.
    work: f32,
    working: bool,
    wander: Option<Vec3>,
    wander_timer: f32,
}

impl Survivor {
    fn new(id: u32, name: String, stats: SurvivorStats, position: Vec3) -> Self {
        Self {
            id,
            name,
            stats,
            health: stats.max_health(),
            leader: None,
            order: Order::Idle,
            weapon: None,
            armor: [None; 2],
            post: position,
            position,
            velocity: Vec3::ZERO,
            yaw: fastrand::f32() * std::f32::consts::TAU,
            on_ground: false,
            hit_wall: false,
            attack_cooldown: 0.0,
            attacking: 0.0,
            hurt_timer: 0.0,
            since_hit: 0.0,
            work: 0.0,
            working: false,
            wander: None,
            wander_timer: 0.0,
        }
    }

    /// Blessure, atténuée par l'armure portée.
    pub fn hurt(&mut self, amount: f32) {
        self.health -= amount * (1.0 - armor_reduction(self.armor));
        self.hurt_timer = 0.3;
        self.since_hit = 0.0;
    }

    /// Posté dans une base : la horde vient jusqu'à lui.
    pub fn is_posted(&self) -> bool {
        self.leader.is_some() && matches!(self.order, Order::Guard | Order::Gather | Order::Repair)
    }

    fn info(&self) -> ServerMessage {
        ServerMessage::SurvivorInfo {
            id: self.id,
            name: self.name.clone(),
            stats: self.stats,
            leader: self.leader.clone(),
            order: self.order,
            weapon: self.weapon,
            armor: self.armor,
        }
    }

    /// Rend au chef tout ce qu'il avait confié.
    fn return_gear(&mut self, inventory: &mut shared::item::Inventory) -> bool {
        let gear: Vec<Item> = self
            .weapon
            .take()
            .into_iter()
            .chain(self.armor.iter_mut().filter_map(Option::take))
            .collect();
        for item in &gear {
            inventory.add(*item, 1);
        }
        !gear.is_empty()
    }
}

fn next_id(ids: &mut NextSurvivorId) -> u32 {
    ids.0 = ids.0.wrapping_add(1);
    ids.0
}

fn restore_survivors(mut commands: Commands, saved: Res<SavedSurvivors>, mut ids: ResMut<NextSurvivorId>) {
    for s in &saved.0 {
        let mut survivor = Survivor::new(next_id(&mut ids), s.name.clone(), s.stats, s.position);
        survivor.health = s.health.min(s.stats.max_health());
        survivor.leader = s.leader.clone();
        survivor.order = s.order;
        survivor.weapon = s.weapon;
        survivor.armor = s.armor;
        survivor.post = s.post;
        commands.spawn(survivor);
    }
    if !saved.0.is_empty() {
        info!("{} survivants recrutés retrouvés", saved.0.len());
    }
}

/// Le jour, des survivants errants apparaissent de temps en temps à distance des joueurs.
#[allow(clippy::too_many_arguments)]
fn spawn_wanderers(
    mut commands: Commands,
    time: Res<Time>,
    clock: Res<Clock>,
    lobby: Res<Lobby>,
    world: Res<GameWorld>,
    mut ids: ResMut<NextSurvivorId>,
    mut server: Option<ResMut<RenetServer>>,
    survivors: Query<(Entity, &Survivor)>,
    mut timer: Local<f32>,
) {
    let players: Vec<Vec3> = lobby
        .players
        .values()
        .filter(|p| p.is_ready())
        .map(|p| p.state.position)
        .collect();
    // Les errants oubliés trop loin de tout le monde s'en vont.
    for (entity, s) in &survivors {
        let forgotten = !players.is_empty() && players.iter().all(|p| p.distance(s.position) > WILD_DESPAWN);
        if s.leader.is_none() && forgotten {
            commands.entity(entity).despawn();
        }
    }
    *timer -= time.delta_secs();
    if *timer > 0.0 || clock.is_night() || players.is_empty() {
        return;
    }
    *timer = SPAWN_INTERVAL;
    let wild: Vec<Vec3> = survivors
        .iter()
        .filter(|(_, s)| s.leader.is_none())
        .map(|(_, s)| s.position)
        .collect();
    if wild.len() >= players.len() + 2 || fastrand::f32() > SPAWN_CHANCE {
        return;
    }
    let around = players[fastrand::usize(0..players.len())];
    if wild.iter().any(|w| w.distance(around) < 100.0) {
        return;
    }
    // De préférence dans une ruine proche, sinon quelque part dans la nature.
    let Some(position) = ruin_near(&world, around).or_else(|| find_spawn_spot(&world, around, 25.0, 45.0)) else {
        return;
    };
    let seed = fastrand::u32(..);
    let survivor = Survivor::new(
        next_id(&mut ids),
        name_from_seed(seed).into(),
        SurvivorStats::from_seed(seed),
        position,
    );
    debug!("Un survivant apparaît : {} en {position}", survivor.name);
    if let Some(server) = server.as_deref_mut() {
        broadcast(server, &survivor.info());
    }
    commands.spawn(survivor);
}

/// Centre d'une ruine à bonne distance d'un joueur (ni sous ses yeux, ni trop loin).
fn ruin_near(world: &GameWorld, around: Vec3) -> Option<Vec3> {
    let generator = world.generator();
    let (cx, cz) = (
        (around.x as i32).div_euclid(RUIN_CELL),
        (around.z as i32).div_euclid(RUIN_CELL),
    );
    let ruins: Vec<Vec3> = (-1..=1)
        .flat_map(|dx| (-1..=1).map(move |dz| (cx + dx, cz + dz)))
        .filter_map(|(x, z)| generator.ruin_in_cell(x, z))
        .map(|(x, ground, z, _)| Vec3::new(x as f32 + 0.5, ground as f32, z as f32 + 0.5))
        .filter(|p| (20.0..90.0).contains(&p.distance(around)))
        .collect();
    (!ruins.is_empty()).then(|| ruins[fastrand::usize(0..ruins.len())])
}

/// Recrutement, changement d'ordre et renvoi.
fn handle_talk(
    mut talk: ResMut<SurvivorTalk>,
    mut lobby: ResMut<Lobby>,
    mut server: Option<ResMut<RenetServer>>,
    mut survivors: Query<&mut Survivor>,
) {
    for TalkRequest {
        client,
        id,
        dismiss,
        give,
    } in talk.0.drain(..)
    {
        let Some(player) = lobby.players.get_mut(&client) else {
            continue;
        };
        if !player.is_alive() {
            continue;
        }
        let Some(mut s) = survivors.iter_mut().find(|s| s.id == id) else {
            continue;
        };
        if s.position.distance(player.state.position) > TALK_RANGE + 1.0 {
            continue;
        }
        let mut changed = false;
        let reply = match s.leader.clone() {
            None => match RECRUIT_COST.iter().find(|(item, n)| player.inventory.has(*item, *n)) {
                Some((item, n)) => {
                    player.inventory.remove(*item, *n);
                    if let Some(server) = server.as_deref_mut() {
                        send_inventory(server, client, player);
                    }
                    s.leader = Some(player.name.clone());
                    s.order = Order::Follow;
                    s.post = s.position;
                    changed = true;
                    info!("{} recrute {}", player.name, s.name);
                    let food = if *n > 1 { "ces pommes" } else { "la ration" };
                    format!("Merci pour {food}. Je viens avec toi ! ({})", s.stats.describe())
                }
                None => "J'ai trop faim pour te suivre. Apporte-moi une ration ou 3 pommes.".into(),
            },
            Some(leader) if leader == player.name && give => {
                // Le chef confie l'arme ou l'armure qu'il tient ; l'ancienne pièce lui revient.
                let held = player.held_item().filter(|i| i.is_giftable());
                let slot = held.map(|item| match item.armor() {
                    Some((ArmorSlot::Head, _)) => &mut s.armor[0],
                    Some((ArmorSlot::Body, _)) => &mut s.armor[1],
                    None => &mut s.weapon,
                });
                match (held, slot) {
                    (Some(item), Some(slot)) if Some(item) != *slot => {
                        player.inventory.remove(item, 1);
                        if let Some(old) = slot.replace(item) {
                            player.inventory.add(old, 1);
                        }
                        if let Some(server) = server.as_deref_mut() {
                            send_inventory(server, client, player);
                        }
                        changed = true;
                        if let Some(ranged) = item.ranged() {
                            let ammo = ranged.ammo.name().to_lowercase();
                            format!(
                                "Merci ! Je prendrai les {ammo}s dans ton sac, ou dans le coffre près de mon poste."
                            )
                        } else if item.armor().is_some() {
                            "Merci, je me sens déjà mieux protégé.".into()
                        } else {
                            "Merci, ça va m'aider à les tenir à distance.".into()
                        }
                    }
                    (Some(_), _) => "J'ai déjà la même.".into(),
                    _ => "Tiens une arme ou une pièce d'armure en main pour me la donner.".into(),
                }
            }
            Some(leader) if leader == player.name => {
                if dismiss
                    && s.return_gear(&mut player.inventory)
                    && let Some(server) = server.as_deref_mut()
                {
                    send_inventory(server, client, player);
                }
                s.order = if dismiss { Order::Idle } else { s.order.next() };
                if dismiss {
                    s.leader = None;
                }
                s.post = s.position;
                s.work = 0.0;
                changed = true;
                s.order.reply().into()
            }
            Some(leader) => format!("Je suis avec {leader}."),
        };
        let Some(server) = server.as_deref_mut() else { continue };
        send(
            server,
            client,
            &ServerMessage::Chat {
                from: s.name.clone(),
                text: reply,
            },
        );
        if changed {
            broadcast(server, &s.info());
        }
    }
}

/// Ce que rapporte une récolte.
fn gathered_item() -> (Item, u32) {
    match fastrand::u32(0..100) {
        0..30 => (Item::Block(Block::Planks), 3),
        30..60 => (Item::Block(Block::Stone), 3),
        60..75 => (Item::Stick, 2),
        75..90 => (Item::Apple, 2),
        _ => (Item::Coal, 1),
    }
}

#[allow(clippy::too_many_arguments)]
fn survivor_ai(
    time: Res<Time>,
    clock: Res<Clock>,
    world: Res<GameWorld>,
    mut lobby: ResMut<Lobby>,
    mut server: Option<ResMut<RenetServer>>,
    mut chests: ResMut<Chests>,
    mut damage: ResMut<BlockDamage>,
    mut zombies: Query<(&mut Zombie, &ZombieBody)>,
    mut survivors: Query<&mut Survivor>,
) {
    let dt = time.delta_secs();
    for mut s in &mut survivors {
        let s = &mut *s;
        s.attack_cooldown -= dt;
        s.attacking -= dt;
        s.hurt_timer -= dt;
        s.since_hit += dt;
        s.working = false;
        let max_health = s.stats.max_health();
        if s.since_hit > 10.0 && s.health < max_health {
            s.health = (s.health + dt).min(max_health);
        }

        let leader = s.leader.as_ref().and_then(|name| {
            lobby
                .players
                .iter()
                .find(|(_, p)| &p.name == name && p.is_alive())
                .map(|(id, p)| (*id, p.state.position))
        });
        // Un survivant dont le chef est absent garde l'endroit où il se trouve.
        let order = match s.order {
            Order::Follow if leader.is_none() => Order::Guard,
            order => order,
        };
        if order == Order::Follow {
            s.post = s.position;
        }

        // Combat : le zombie le plus proche dans la zone défendue.
        let (center, radius) = match order {
            Order::Idle => (s.position, WILD_DEFEND_RADIUS),
            Order::Follow => (s.position, DEFEND_RADIUS),
            _ => (s.post, DEFEND_RADIUS),
        };
        let ranged = s.weapon.and_then(Item::ranged);
        let melee = Item::melee(s.weapon.filter(|w| w.is_melee_weapon()));
        // Une lance tient les zombies plus loin.
        let melee_range = ATTACK_RANGE + (melee.reach - Item::melee(None).reach).max(0.0);
        // Avec une arme à distance, il vise plus loin que sa zone.
        let radius = if ranged.is_some() { radius + 8.0 } else { radius };
        let threat = zombies
            .iter()
            .filter(|(z, b)| z.health > 0.0 && b.position.distance(center) < radius)
            .map(|(z, b)| (z.id, b.position, b.position.distance(s.position)))
            .min_by(|a, b| a.2.total_cmp(&b.2));

        let mut goal: Option<Vec3> = None;
        let mut face: Option<Vec3> = None;
        let mut speed = SURVIVOR_SPEED;
        if let Some((zombie_id, target, dist)) = threat {
            let flat = Vec2::new(target.x - s.position.x, target.z - s.position.z);
            let eye = s.position + Vec3::Y * 1.5;
            let aim = target + Vec3::Y * 1.0;
            // Tir si l'arme a des munitions à portée de main et que la cible est visible.
            let shot = ranged
                .filter(|w| dist <= w.range.min(SURVIVOR_RANGE) && clear_line(&world, eye, aim))
                .and_then(|w| {
                    ammo_source(s, w.ammo, leader.map(|(id, _)| id), &lobby, &chests, &world).map(|source| (w, source))
                });
            if let Some((weapon, source)) = shot {
                face = Some(target);
                if s.attack_cooldown <= 0.0 {
                    s.attack_cooldown = weapon.cooldown * 2.0;
                    s.attacking = 0.3;
                    take_ammo(source, weapon.ammo, &mut lobby, &mut chests, server.as_deref_mut());
                    if let Some((mut z, _)) = zombies.iter_mut().find(|(z, _)| z.id == zombie_id) {
                        z.health -= weapon.damage;
                        z.hurt_timer = 0.3;
                    }
                    if let Some(server) = server.as_deref_mut() {
                        broadcast(
                            server,
                            &ServerMessage::Shot {
                                from: 0,
                                origin: eye,
                                end: aim,
                                rifle: weapon.ammo == Item::Ammo,
                            },
                        );
                    }
                }
            } else if flat.length() < melee_range && (target.y - s.position.y).abs() < 1.6 {
                face = Some(target);
                if s.attack_cooldown <= 0.0 {
                    // L'arme de mêlée ajoute ses dégâts à la force du survivant et règle sa cadence.
                    s.attack_cooldown = ATTACK_COOLDOWN * (melee.cooldown / Item::melee(None).cooldown).sqrt();
                    s.attacking = 0.4;
                    let damage = s.stats.damage() + melee.damage - Item::melee_damage(None);
                    let dir = flat.normalize_or_zero();
                    for (mut z, body) in zombies.iter_mut() {
                        let to = Vec2::new(body.position.x - s.position.x, body.position.z - s.position.z);
                        // La masse balaie tout ce qui est devant ; sinon, seule la cible est touchée.
                        let hit = if melee.sweep {
                            z.health > 0.0 && to.length() < melee_range + 0.3 && to.normalize_or_zero().dot(dir) > 0.4
                        } else {
                            z.id == zombie_id
                        };
                        if hit {
                            z.health -= damage;
                            z.hurt_timer = 0.3;
                            let push = to.normalize_or_zero() * melee.knockback * 0.8;
                            z.knockback = Vec3::new(push.x, 2.5, push.y);
                        }
                    }
                }
            } else {
                goal = Some(target);
            }
        } else {
            match order {
                Order::Idle => {
                    // Un joueur approche : le survivant s'arrête et le regarde.
                    let visitor = lobby
                        .players
                        .values()
                        .filter(|p| p.is_alive())
                        .map(|p| p.state.position)
                        .find(|p| p.distance(s.position) < TALK_RANGE + 2.0);
                    match visitor {
                        Some(p) => face = Some(p),
                        None => goal = wander(s, 5.0, dt),
                    }
                }
                Order::Follow => {
                    if let Some((_, leader_pos)) = leader {
                        let dist = leader_pos.distance(s.position);
                        if dist > FOLLOW_TELEPORT || s.position.y < -10.0 {
                            s.position = leader_pos;
                            s.velocity = Vec3::ZERO;
                        } else if dist > 3.0 {
                            goal = Some(leader_pos);
                            if dist > 8.0 {
                                speed = SURVIVOR_SPEED * 1.5;
                            }
                        }
                    }
                }
                Order::Guard => {
                    if s.post.distance(s.position) > 1.5 {
                        goal = Some(s.post);
                    }
                }
                Order::Gather if clock.is_night() => {
                    // La nuit, on rentre au poste et on le défend.
                    if s.post.distance(s.position) > 1.5 {
                        goal = Some(s.post);
                    }
                }
                Order::Gather => {
                    goal = wander(s, 6.0, dt);
                    s.working = true;
                    s.work += dt * s.stats.work_speed();
                    if s.work >= GATHER_SECS {
                        s.work = 0.0;
                        let (item, n) = gathered_item();
                        deliver(
                            s,
                            item,
                            n,
                            leader.map(|(id, _)| id),
                            &world,
                            &mut chests,
                            &mut lobby,
                            server.as_deref_mut(),
                        );
                    }
                }
                Order::Repair => {
                    let post = s.post;
                    let job = damage
                        .0
                        .iter()
                        .filter(|(pos, (block, dealt))| {
                            *dealt > 0.0
                                && world.block(**pos) == *block
                                && (pos.as_vec3() + Vec3::splat(0.5)).distance(post) < WORK_RADIUS
                        })
                        .map(|(pos, _)| *pos)
                        .min_by(|a, b| {
                            let d = |p: &IVec3| p.as_vec3().distance(s.position);
                            d(a).total_cmp(&d(b))
                        });
                    match job {
                        Some(pos) => {
                            let center = pos.as_vec3() + Vec3::splat(0.5);
                            let flat = Vec2::new(center.x - s.position.x, center.z - s.position.z);
                            if flat.length() < 2.5 {
                                face = Some(center);
                                s.working = true;
                                repair(
                                    pos,
                                    REPAIR_RATE * s.stats.work_speed() * dt,
                                    &mut damage,
                                    server.as_deref_mut(),
                                );
                            } else {
                                goal = Some(center);
                            }
                        }
                        None if s.post.distance(s.position) > 1.5 => goal = Some(s.post),
                        None => {}
                    }
                }
            }
        }
        step(s, goal, face, speed, dt, &world);
    }
}

/// Portée maximale d'un survivant armé d'un arc ou d'un fusil.
const SURVIVOR_RANGE: f32 = 24.0;

/// Rien de solide entre deux points.
pub(crate) fn clear_line(world: &GameWorld, from: Vec3, to: Vec3) -> bool {
    let dir = (to - from).normalize_or_zero();
    let dist = from.distance(to);
    match raycast(from, dir, dist, |p| world.is_solid(p)) {
        Some(hit) => (hit.block.as_vec3() + Vec3::splat(0.5)).distance(from) > dist - 0.3,
        None => true,
    }
}

/// D'où un survivant tire ses munitions.
#[derive(Clone, Copy)]
enum AmmoSource {
    /// Le sac de son chef, s'il le suit.
    Leader(ClientId),
    /// Le coffre le plus proche de son poste.
    Chest(IVec3),
}

fn ammo_source(
    s: &Survivor,
    ammo: Item,
    leader: Option<ClientId>,
    lobby: &Lobby,
    chests: &Chests,
    world: &GameWorld,
) -> Option<AmmoSource> {
    if s.order == Order::Follow
        && let Some(id) = leader
        && lobby.players.get(&id).is_some_and(|p| p.inventory.has(ammo, 1))
    {
        return Some(AmmoSource::Leader(id));
    }
    chests
        .0
        .iter()
        .filter(|(pos, inv)| {
            inv.has(ammo, 1)
                && world.block(**pos) == Block::Chest
                && (pos.as_vec3() + Vec3::splat(0.5)).distance(s.post) < WORK_RADIUS
        })
        .map(|(pos, _)| AmmoSource::Chest(*pos))
        .next()
}

fn take_ammo(source: AmmoSource, ammo: Item, lobby: &mut Lobby, chests: &mut Chests, server: Option<&mut RenetServer>) {
    match source {
        AmmoSource::Leader(id) => {
            if let Some(player) = lobby.players.get_mut(&id) {
                player.inventory.remove(ammo, 1);
                if let Some(server) = server {
                    send_inventory(server, id, player);
                }
            }
        }
        AmmoSource::Chest(pos) => {
            if let Some(inv) = chests.0.get_mut(&pos) {
                inv.remove(ammo, 1);
            }
            if let Some(server) = server {
                send_chest_to_viewers(server, lobby, chests, pos);
            }
        }
    }
}

/// Choisit de temps en temps un point au hasard autour du poste.
fn wander(s: &mut Survivor, radius: f32, dt: f32) -> Option<Vec3> {
    s.wander_timer -= dt;
    if s.wander_timer <= 0.0 || s.wander.is_none() {
        s.wander_timer = 4.0 + fastrand::f32() * 5.0;
        let angle = fastrand::f32() * std::f32::consts::TAU;
        let dist = fastrand::f32() * radius;
        s.wander = Some(s.post + Vec3::new(angle.cos() * dist, 0.0, angle.sin() * dist));
    }
    s.wander
        .filter(|w| Vec2::new(w.x - s.position.x, w.z - s.position.z).length() > 0.8)
}

/// Range une récolte dans le coffre le plus proche du poste, sinon dans l'inventaire du chef.
#[allow(clippy::too_many_arguments)]
fn deliver(
    s: &Survivor,
    item: Item,
    n: u32,
    leader: Option<ClientId>,
    world: &GameWorld,
    chests: &mut Chests,
    lobby: &mut Lobby,
    server: Option<&mut RenetServer>,
) {
    let chest = chests
        .0
        .keys()
        .copied()
        .filter(|pos| world.block(*pos) == Block::Chest)
        .filter(|pos| (pos.as_vec3() + Vec3::splat(0.5)).distance(s.post) < WORK_RADIUS)
        .min_by(|a, b| {
            let d = |p: &IVec3| p.as_vec3().distance(s.post);
            d(a).total_cmp(&d(b))
        });
    if let Some(pos) = chest {
        if let Some(contents) = chests.0.get_mut(&pos) {
            contents.add(item, n);
        }
        if let Some(server) = server {
            send_chest_to_viewers(server, lobby, chests, pos);
        }
    } else if let Some(id) = leader
        && let Some(player) = lobby.players.get_mut(&id)
    {
        player.inventory.add(item, n);
        if let Some(server) = server {
            send_inventory(server, id, player);
        }
    }
}

/// Retire des dégâts à un bloc abîmé et prévient les clients par huitièmes.
fn repair(pos: IVec3, amount: f32, damage: &mut BlockDamage, server: Option<&mut RenetServer>) {
    let Some((block, dealt)) = damage.0.get_mut(&pos) else {
        return;
    };
    let hp = block.max_health() as f32;
    let step_before = (*dealt / hp * 8.0).ceil() as u32;
    *dealt = (*dealt - amount).max(0.0);
    let left = *dealt;
    if left <= 0.0 {
        damage.0.remove(&pos);
    }
    if (left / hp * 8.0).ceil() as u32 != step_before
        && let Some(server) = server
    {
        server.broadcast_message(
            DefaultChannel::ReliableOrdered,
            encode(&ServerMessage::BlockDamage {
                pos,
                damage: (left / hp * 255.0) as u8,
            }),
        );
    }
}

/// Déplace le survivant vers `goal` (en sautant les marches), tourné vers `face` s'il y en a un.
fn step(s: &mut Survivor, goal: Option<Vec3>, face: Option<Vec3>, speed: f32, dt: f32, world: &GameWorld) {
    let mut wish = goal.map_or(Vec3::ZERO, |g| Vec3::new(g.x - s.position.x, 0.0, g.z - s.position.z));
    if wish.length() < 0.3 {
        wish = Vec3::ZERO;
    }
    let wish = wish.normalize_or_zero() * speed;
    if let Some(target) = face {
        s.yaw = (s.position.x - target.x).atan2(s.position.z - target.z);
    } else if wish != Vec3::ZERO {
        s.yaw = (-wish.x).atan2(-wish.z);
    }
    let mut velocity = Vec3::new(wish.x, s.velocity.y, wish.z);
    if s.on_ground && s.hit_wall && wish != Vec3::ZERO {
        velocity.y = JUMP_SPEED;
    }
    let result = step_body(s.position, velocity, SURVIVOR_SIZE, dt, &|p| {
        world.is_solid(p) || p.y < 0
    });
    s.position = result.position;
    s.velocity = Vec3::new(0.0, result.velocity.y, 0.0);
    s.on_ground = result.on_ground;
    s.hit_wall = result.hit_wall;
    if s.position.y < -10.0 {
        s.position = s.post + Vec3::Y;
        s.velocity = Vec3::ZERO;
    }
}

fn survivor_deaths(
    mut commands: Commands,
    survivors: Query<(Entity, &Survivor)>,
    mut server: Option<ResMut<RenetServer>>,
) {
    for (entity, s) in &survivors {
        if s.health > 0.0 {
            continue;
        }
        commands.entity(entity).despawn();
        let Some(leader) = &s.leader else { continue };
        info!("Le survivant {} ({leader}) est mort", s.name);
        if let Some(server) = server.as_deref_mut() {
            broadcast(
                server,
                &ServerMessage::Chat {
                    from: "Serveur".into(),
                    text: format!("{}, survivant de {leader}, a été tué par les zombies", s.name),
                },
            );
        }
    }
}

/// Tient à jour la liste des survivants recrutés pour la sauvegarde.
fn record_survivors(survivors: Query<&Survivor>, mut saved: ResMut<SavedSurvivors>, mut tick: Local<u32>) {
    *tick += 1;
    if !tick.is_multiple_of(15) {
        return;
    }
    saved.0 = survivors
        .iter()
        .filter(|s| s.leader.is_some() && s.health > 0.0)
        .map(|s| SavedSurvivor {
            name: s.name.clone(),
            stats: s.stats,
            health: s.health,
            position: s.position,
            post: s.post,
            leader: s.leader.clone(),
            order: s.order,
            weapon: s.weapon,
            armor: s.armor,
        })
        .collect();
    saved.0.sort_by(|a, b| a.name.cmp(&b.name));
}

/// Envoie les survivants proches à chaque joueur, 10 fois par seconde, et leur identité aux nouveaux venus.
fn broadcast_survivors(
    mut server: Option<ResMut<RenetServer>>,
    lobby: Res<Lobby>,
    survivors: Query<&Survivor>,
    mut informed: Local<HashSet<ClientId>>,
    mut tick: Local<u32>,
) {
    let Some(server) = server.as_deref_mut() else { return };
    informed.retain(|id| lobby.players.get(id).is_some_and(|p| p.is_ready()));
    for (id, _) in lobby.players.iter().filter(|(_, p)| p.is_ready()) {
        if informed.insert(*id) {
            for s in &survivors {
                send(server, *id, &s.info());
            }
        }
    }
    *tick += 1;
    if !tick.is_multiple_of(3) {
        return;
    }
    for (id, player) in lobby.players.iter().filter(|(_, p)| p.is_ready()) {
        let states: Vec<SurvivorState> = survivors
            .iter()
            .filter(|s| s.position.distance(player.state.position) < VIEW_DIST)
            .map(|s| {
                let mut flags = 0;
                if s.attacking > 0.0 {
                    flags |= SurvivorState::ATTACKING;
                }
                if s.hurt_timer > 0.0 {
                    flags |= SurvivorState::HURT;
                }
                if s.working {
                    flags |= SurvivorState::WORKING;
                }
                SurvivorState::new(s.id, s.position, s.yaw, s.health / s.stats.max_health(), flags)
            })
            .collect();
        if !states.is_empty() {
            server.send_message(
                *id,
                DefaultChannel::Unreliable,
                encode(&ServerMessage::Survivors(states)),
            );
        }
    }
}
