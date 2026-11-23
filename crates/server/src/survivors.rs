//! Survivants : PNJ humains à recruter. Ils suivent leur chef, gardent un poste,
//! récoltent des ressources, réparent les murs, cultivent, cuisinent ou chassent, et se
//! battent contre les zombies. Leur moral (faim, feu de camp, équipement, deuils) règle
//! leur ardeur au travail ; au plus bas, ils abandonnent leur chef.

use std::collections::HashSet;

use bevy::prelude::*;
use bevy_renet::RenetServer;
use bevy_renet::renet::{ClientId, DefaultChannel};
use shared::block::Block;
use shared::item::{ArmorSlot, Item, armor_reduction};
use shared::physics::step_body;
use shared::protocol::{ServerMessage, SurvivorState, encode};
use shared::raycast::raycast;
use shared::survivor::{
    MORALE_BREAKING, MORALE_START, MoraleFactors, Offer, Order, RECRUIT_COST, SURVIVOR_SIZE, SURVIVOR_SPEED,
    SurvivorStats, TALK_RANGE, TRADES, morale_multiplier, name_from_seed,
};
use shared::worldgen::RUIN_CELL;

use crate::animals::Animal;
use crate::daynight::Clock;
use crate::net::{Lobby, WorldVersion, broadcast, send, send_chest_to_viewers, send_inventory};
use crate::save::{SavedSurvivor, SavedSurvivors};
use crate::world::{Chests, GameWorld};
use crate::zombies::{BlockDamage, Zombie, ZombieBody, ZombieSet, find_spawn_spot};

const JUMP_SPEED: f32 = 8.5;
/// Portée d'un coup de survivant, en mètres.
const ATTACK_RANGE: f32 = 1.7;
const ATTACK_COOLDOWN: f32 = 0.9;
/// Sous cette part de sa vie, un survivant au corps-à-corps se replie.
const RETREAT_HEALTH: f32 = 0.3;
/// Un tireur recule quand un zombie s'approche à moins de cette distance.
const KITE_DIST: f32 = 4.0;
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
                    spawn_traders,
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
#[derive(Component, Debug, Clone)]
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
    /// Faim : 100 = rassasié. Seuls les survivants recrutés ont faim.
    pub hunger: f32,
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
    /// Moral, de 0 à 100 (seuls les survivants recrutés en ont l'usage).
    pub morale: f32,
    /// Chagrin après la mort d'un compagnon : pèse sur le moral, s'estompe avec le temps.
    pub grief: f32,
    /// Secondes passées au bord de l'abandon.
    despair: f32,
    /// Case visée par le travail en cours (blé à récolter, terre à semer).
    job_target: Option<IVec3>,
    /// Feu de camp près du poste, cherché de temps en temps.
    campfire: Option<IVec3>,
    scan_timer: f32,
    /// Marchés d'un marchand de passage (vide pour un survivant).
    pub offers: Vec<Offer>,
    /// Chemin en cours (centres de cases), vers `path_goal`.
    path: Vec<Vec3>,
    path_goal: Vec3,
    path_timer: f32,
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
            hunger: SURVIVOR_MAX_HUNGER,
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
            morale: MORALE_START,
            grief: 0.0,
            despair: 0.0,
            job_target: None,
            campfire: None,
            scan_timer: 0.0,
            offers: Vec::new(),
            path: Vec::new(),
            path_goal: Vec3::ZERO,
            path_timer: 0.0,
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
        self.leader.is_some() && (self.order == Order::Guard || self.order.is_job())
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
            offers: self.offers.clone(),
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
        .map(|r| Vec3::new(r.x as f32 + 0.5, r.ground as f32, r.z as f32 + 0.5))
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
            // Un marchand ne se recrute pas : il échange.
            None if !s.offers.is_empty() => {
                match s.offers.iter().find(|o| player.inventory.has(o.want, o.give)).copied() {
                    Some(o) => {
                        player.inventory.remove(o.want, o.give);
                        player.inventory.add(o.offer, o.get);
                        if let Some(server) = server.as_deref_mut() {
                            send_inventory(server, client, player);
                        }
                        format!("Marché conclu : {}.", o.describe())
                    }
                    None => {
                        let list: Vec<String> = s.offers.iter().map(|o| o.describe()).collect();
                        format!("Je prends : {}. Reviens avec ce qu'il faut.", list.join(", "))
                    }
                }
            }
            None => match RECRUIT_COST.iter().find(|(item, n)| player.inventory.has(*item, *n)) {
                Some((item, n)) => {
                    player.inventory.remove(*item, *n);
                    if let Some(server) = server.as_deref_mut() {
                        send_inventory(server, client, player);
                    }
                    s.leader = Some(player.name.clone());
                    s.order = Order::Follow;
                    s.post = s.position;
                    player.record.recruited += 1;
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
    mut world: ResMut<GameWorld>,
    mut version: ResMut<WorldVersion>,
    mut lobby: ResMut<Lobby>,
    mut server: Option<ResMut<RenetServer>>,
    mut chests: ResMut<Chests>,
    mut damage: ResMut<BlockDamage>,
    mut zombies: Query<(&mut Zombie, &ZombieBody)>,
    mut survivors: Query<&mut Survivor>,
    mut animals: Query<&mut Animal>,
) {
    let dt = time.delta_secs();
    let horde = clock.is_night() && clock.is_horde_night();
    let mut medics: Vec<(Vec3, f32)> = Vec::new();
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
        // Un survivant recruté a faim : il mange ce que son chef ou le coffre du poste lui offre.
        if s.leader.is_some() {
            s.hunger = (s.hunger - SURVIVOR_HUNGER_PER_SEC * dt).max(0.0);
            if s.hunger < EAT_BELOW {
                eat(
                    s,
                    leader.map(|(id, _)| id),
                    &world,
                    &mut lobby,
                    &mut chests,
                    server.as_deref_mut(),
                );
            }
            if s.hunger <= 0.0 {
                s.health -= STARVATION_DPS * dt;
                s.since_hit = 0.0;
            }
        }
        let starving = s.leader.is_some() && s.hunger <= 0.0;
        if s.leader.is_some() {
            s.scan_timer -= dt;
            if s.scan_timer <= 0.0 {
                s.scan_timer = 3.0;
                s.campfire = find_near(&world, s.post, Block::Campfire, WORK_RADIUS as i32);
            }
            let factors = MoraleFactors {
                fed: s.hunger > EAT_BELOW,
                starving,
                campfire: s.campfire.is_some(),
                armed: s.weapon.is_some(),
                armored: s.armor.iter().any(Option::is_some),
                hurt: s.since_hit < 10.0,
                horde,
                grief: s.grief,
            };
            s.grief = (s.grief - GRIEF_FADE * dt).max(0.0);
            let target = factors.target();
            s.morale += (target - s.morale).clamp(-dt / MORALE_SECS_PER_POINT, dt / MORALE_SECS_PER_POINT);
            if s.morale < MORALE_BREAKING {
                s.despair += dt;
            } else {
                s.despair = 0.0;
            }
            if s.despair > DESERT_AFTER {
                desert(s, &mut lobby, server.as_deref_mut());
                continue;
            }
        }
        let morale = morale_multiplier(s.morale);
        let work_speed = s.stats.work_speed() * morale;
        // Un survivant dont le chef est absent garde l'endroit où il se trouve ; affamé, il ne travaille plus.
        let order = match s.order {
            Order::Follow if leader.is_none() => Order::Guard,
            order if starving && order.is_job() => Order::Guard,
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
        // Priorité au zombie qui menace le chef, puis au plus proche, puis au plus blessé.
        let leader_pos = leader.map(|(_, p)| p);
        let threat = zombies
            .iter()
            .filter(|(z, b)| z.health > 0.0 && b.position.distance(center) < radius)
            .map(|(z, b)| {
                let dist = b.position.distance(s.position);
                let on_leader = leader_pos.is_some_and(|l| l.distance(b.position) < 3.0);
                let score = dist - if on_leader { 4.0 } else { 0.0 } - (1.0 - z.health / z.kind.stats().health) * 2.0;
                (z.id, b.position, dist, score)
            })
            .min_by(|a, b| a.3.total_cmp(&b.3))
            .map(|(id, pos, dist, _)| (id, pos, dist));
        let leader_id = leader.map(|(id, _)| id);
        let hurt = s.health < s.stats.max_health() * RETREAT_HEALTH;

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
                // Trop près : il recule en tirant.
                if dist < KITE_DIST {
                    let away = (s.position - target).with_y(0.0).normalize_or_zero();
                    goal = Some(s.position + away * 3.0);
                    speed = SURVIVOR_SPEED * 0.9;
                }
                if s.attack_cooldown <= 0.0 {
                    s.attack_cooldown = weapon.cooldown * 2.0;
                    s.attacking = 0.3;
                    take_one(source, weapon.ammo, &mut lobby, &mut chests, server.as_deref_mut());
                    if let Some((mut z, _)) = zombies.iter_mut().find(|(z, _)| z.id == zombie_id) {
                        z.health -= weapon.damage * z.kind.armor_factor(false);
                        z.hurt_timer = 0.3;
                        // L'expérience des zombies abattus revient au chef.
                        z.last_attacker = leader_id.or(z.last_attacker);
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
            } else if hurt && ranged.is_none() && dist < 6.0 {
                // Blessé : il rompt le combat et se replie vers son chef ou son poste.
                let away = (s.position - target).with_y(0.0).normalize_or_zero();
                let refuge = leader_pos.unwrap_or(s.post);
                goal = Some(if refuge.distance(target) > dist {
                    refuge
                } else {
                    s.position + away * 4.0
                });
                speed = SURVIVOR_SPEED * 1.3;
            } else if flat.length() < melee_range && (target.y - s.position.y).abs() < 1.6 {
                face = Some(target);
                // Entre deux coups, il recule d'un pas pour esquiver la riposte, puis revient.
                if s.attack_cooldown > ATTACK_COOLDOWN * 0.35 && flat.length() < melee_range * 0.85 {
                    let away = (s.position - target).with_y(0.0).normalize_or_zero();
                    goal = Some(s.position + away * 1.5);
                    speed = SURVIVOR_SPEED * 0.8;
                }
                if s.attack_cooldown <= 0.0 {
                    // L'arme de mêlée ajoute ses dégâts à la force du survivant et règle sa cadence.
                    s.attack_cooldown = ATTACK_COOLDOWN * (melee.cooldown / Item::melee(None).cooldown).sqrt();
                    s.attacking = 0.4;
                    let damage =
                        (s.stats.damage() + melee.damage - Item::melee_damage(None)) * (1.0 + (morale - 1.0) * 0.4);
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
                            z.health -= damage * z.kind.armor_factor(melee.sweep);
                            z.hurt_timer = 0.3;
                            z.last_attacker = leader_id.or(z.last_attacker);
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
                Order::Gather | Order::Farm | Order::Hunt if clock.is_night() => {
                    // La nuit, on rentre au poste et on le défend.
                    if s.post.distance(s.position) > 1.5 {
                        goal = Some(s.post);
                    }
                }
                Order::Gather => {
                    goal = wander(s, 6.0, dt);
                    s.working = true;
                    s.work += dt * work_speed;
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
                                repair(pos, REPAIR_RATE * work_speed * dt, &mut damage, server.as_deref_mut());
                            } else {
                                goal = Some(center);
                            }
                        }
                        None if s.post.distance(s.position) > 1.5 => goal = Some(s.post),
                        None => {}
                    }
                }
                Order::Farm => {
                    let jobs = Jobs {
                        world: &mut world,
                        version: &mut version,
                        chests: &mut chests,
                        lobby: &mut lobby,
                        server: server.as_deref_mut(),
                    };
                    goal = farm(s, leader.map(|(id, _)| id), work_speed, dt, jobs, &mut face);
                }
                Order::Cook => {
                    let jobs = Jobs {
                        world: &mut world,
                        version: &mut version,
                        chests: &mut chests,
                        lobby: &mut lobby,
                        server: server.as_deref_mut(),
                    };
                    goal = cook(s, leader.map(|(id, _)| id), work_speed, dt, jobs, &mut face);
                }
                Order::Hunt => {
                    let prey = animals
                        .iter()
                        .filter(|a| a.health > 0.0 && a.position.distance(s.post) < HUNT_RADIUS)
                        .map(|a| (a.id, a.position, a.position.distance(s.position)))
                        .min_by(|a, b| a.2.total_cmp(&b.2));
                    match prey {
                        Some((prey_id, target, dist)) => {
                            goal = Some(target);
                            speed = SURVIVOR_SPEED * 1.3;
                            if dist < melee_range + 0.6 && (target.y - s.position.y).abs() < 1.6 {
                                face = Some(target);
                                goal = None;
                                if s.attack_cooldown <= 0.0 {
                                    s.attack_cooldown = ATTACK_COOLDOWN;
                                    s.attacking = 0.4;
                                    let damage = s.stats.damage() + melee.damage - Item::melee_damage(None);
                                    if let Some(mut animal) = animals.iter_mut().find(|a| a.id == prey_id) {
                                        let push = (target - s.position).with_y(0.0).normalize_or_zero() * 4.0;
                                        animal.hurt(damage, None, push);
                                        if animal.health <= 0.0 {
                                            // Le chasseur rapporte sa prise au coffre du poste (ou à son chef).
                                            let (min, max) = animal.kind.stats().meat;
                                            let leader_id = leader.map(|(id, _)| id);
                                            let meat = fastrand::u32(min..=max);
                                            for (item, n) in [(Item::RawMeat, meat), (Item::Hide, 1)] {
                                                deliver(
                                                    s,
                                                    item,
                                                    n,
                                                    leader_id,
                                                    &world,
                                                    &mut chests,
                                                    &mut lobby,
                                                    server.as_deref_mut(),
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        None => goal = wander(s, 10.0, dt),
                    }
                }
                Order::Heal => {
                    if s.post.distance(s.position) > 1.5 {
                        goal = Some(s.post);
                    }
                    medics.push((s.post, work_speed));
                    let mut jobs = Jobs {
                        world: &mut world,
                        version: &mut version,
                        chests: &mut chests,
                        lobby: &mut lobby,
                        server: server.as_deref_mut(),
                    };
                    // Entre deux soins, il roule des bandages avec les herbes du coffre.
                    if let Some(chest) = jobs.chest_with(s, Item::Herb, 2) {
                        s.working = true;
                        s.work += dt * work_speed;
                        if s.work >= BANDAGE_SECS {
                            s.work = 0.0;
                            jobs.take(chest, Item::Herb, 2);
                            jobs.deliver(s, leader.map(|(id, _)| id), Item::Bandage, 1);
                        }
                    }
                }
                Order::Engineer => {
                    let jobs = Jobs {
                        world: &mut world,
                        version: &mut version,
                        chests: &mut chests,
                        lobby: &mut lobby,
                        server: server.as_deref_mut(),
                    };
                    goal = engineer(s, work_speed, dt, jobs, &mut face);
                }
            }
        }
        step(s, goal, face, speed, dt, &world);
    }
    // Les médecins soignent les joueurs et les survivants proches de leur poste.
    for (post, work_speed) in medics {
        let heal = MEDIC_HEAL * work_speed * dt;
        for (id, player) in lobby.players.iter_mut() {
            if !player.is_alive() || player.state.position.distance(post) > MEDIC_RADIUS {
                continue;
            }
            let before = (player.health.ceil(), player.bleeding > 0.0);
            player.health = (player.health + heal).min(player.max_health());
            player.bleeding = 0.0;
            if before != (player.health.ceil(), false)
                && let Some(server) = server.as_deref_mut()
            {
                crate::net::send_vitals(server, *id, player);
            }
        }
        for mut s in &mut survivors {
            if s.position.distance(post) < MEDIC_RADIUS {
                s.health = (s.health + heal).min(s.stats.max_health());
            }
        }
    }
}

/// Le médecin soigne dans ce rayon autour de son poste, de tant de points par seconde.
const MEDIC_RADIUS: f32 = 8.0;
const MEDIC_HEAL: f32 = 2.5;
/// Temps pour rouler un bandage.
const BANDAGE_SECS: f32 = 20.0;
/// Temps pour recharger une machine, et ce qu'il y met.
const ENGINEER_SECS: f32 = 3.0;
const TURRET_LOAD: u32 = 6;
const TURRET_FULL: u32 = 12;
const GENERATOR_LOAD: u32 = 2;
const GENERATOR_FULL: u32 = 4;

/// Ingénieur : recharge les tourelles (cartouches ou flèches) et les générateurs (charbon)
/// proches de son poste avec le contenu des coffres. Renvoie où il veut aller.
fn engineer(s: &mut Survivor, work_speed: f32, dt: f32, mut jobs: Jobs, face: &mut Option<Vec3>) -> Option<Vec3> {
    // Machine à recharger et ce qu'il faut y mettre.
    let post = s.post;
    let need = |pos: IVec3, jobs: &Jobs| -> Option<(Item, u32)> {
        let inv = jobs.chests.0.get(&pos);
        let count = |item: Item| inv.map_or(0, |i| i.count(item));
        match jobs.world.block(pos) {
            Block::Turret if count(Item::Ammo) + count(Item::Arrow) < TURRET_FULL => [Item::Ammo, Item::Arrow]
                .into_iter()
                .find(|item| jobs.chest_near(post, *item, 1).is_some())
                .map(|item| (item, TURRET_LOAD)),
            Block::Generator if count(Item::Coal) < GENERATOR_FULL => jobs
                .chest_near(post, Item::Coal, 1)
                .map(|_| (Item::Coal, GENERATOR_LOAD)),
            _ => None,
        }
    };
    if s.job_target.is_some_and(|p| need(p, &jobs).is_none()) {
        s.job_target = None;
    }
    if s.job_target.is_none() {
        s.job_target = jobs
            .chests
            .0
            .keys()
            .copied()
            .filter(|pos| (pos.as_vec3() + Vec3::splat(0.5)).distance(s.post) < WORK_RADIUS)
            .find(|pos| need(*pos, &jobs).is_some());
    }
    let Some(target) = s.job_target else {
        return (s.post.distance(s.position) > 1.5).then_some(s.post);
    };
    let center = target.as_vec3() + Vec3::new(0.5, 0.0, 0.5);
    if Vec2::new(center.x - s.position.x, center.z - s.position.z).length() > 2.2 {
        return Some(center);
    }
    *face = Some(center);
    s.working = true;
    s.work += dt * work_speed;
    if s.work < ENGINEER_SECS {
        return None;
    }
    s.work = 0.0;
    if let Some((item, n)) = need(target, &jobs)
        && let Some(chest) = jobs.chest_with(s, item, 1)
    {
        let n = n.min(jobs.chests.0.get(&chest).map_or(0, |i| i.count(item)));
        jobs.take(chest, item, n);
        jobs.chests.0.entry(target).or_default().add(item, n);
    }
    None
}

/// Portée maximale d'un survivant armé d'un arc ou d'un fusil.
const SURVIVOR_RANGE: f32 = 24.0;

/// Rien de solide entre deux points.
pub fn clear_line(world: &GameWorld, from: Vec3, to: Vec3) -> bool {
    let dir = (to - from).normalize_or_zero();
    let dist = from.distance(to);
    match raycast(from, dir, dist, |p| world.is_solid(p)) {
        Some(hit) => (hit.block.as_vec3() + Vec3::splat(0.5)).distance(from) > dist - 0.3,
        None => true,
    }
}

/// D'où un survivant tire ses munitions ou sa nourriture.
#[derive(Clone, Copy)]
enum Supply {
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
) -> Option<Supply> {
    if s.order == Order::Follow
        && let Some(id) = leader
        && lobby.players.get(&id).is_some_and(|p| p.inventory.has(ammo, 1))
    {
        return Some(Supply::Leader(id));
    }
    chests
        .0
        .iter()
        .filter(|(pos, inv)| {
            inv.has(ammo, 1)
                && world.block(**pos) == Block::Chest
                && (pos.as_vec3() + Vec3::splat(0.5)).distance(s.post) < WORK_RADIUS
        })
        .map(|(pos, _)| Supply::Chest(*pos))
        .next()
}

fn take_one(source: Supply, item: Item, lobby: &mut Lobby, chests: &mut Chests, server: Option<&mut RenetServer>) {
    match source {
        Supply::Leader(id) => {
            if let Some(player) = lobby.players.get_mut(&id) {
                player.inventory.remove(item, 1);
                if let Some(server) = server {
                    send_inventory(server, id, player);
                }
            }
        }
        Supply::Chest(pos) => {
            if let Some(inv) = chests.0.get_mut(&pos) {
                inv.remove(item, 1);
            }
            if let Some(server) = server {
                send_chest_to_viewers(server, lobby, chests, pos);
            }
        }
    }
}

/// Faim maximale d'un survivant.
pub const SURVIVOR_MAX_HUNGER: f32 = 100.0;
/// Un survivant rassasié tient environ 20 minutes.
const SURVIVOR_HUNGER_PER_SEC: f32 = SURVIVOR_MAX_HUNGER / 1200.0;
/// En dessous, il cherche à manger.
const EAT_BELOW: f32 = 50.0;
/// Vie perdue par seconde quand il meurt de faim.
const STARVATION_DPS: f32 = 0.5;

/// Le survivant mange un aliment du sac de son chef (s'il le suit) ou du coffre près de son poste.
fn eat(
    s: &mut Survivor,
    leader: Option<ClientId>,
    world: &GameWorld,
    lobby: &mut Lobby,
    chests: &mut Chests,
    server: Option<&mut RenetServer>,
) {
    let is_food = |item: &Item| item.food().is_some_and(|(hunger, _)| hunger > 0.0);
    let from_leader = leader.filter(|_| s.order == Order::Follow).and_then(|id| {
        let food = lobby.players.get(&id)?.inventory.iter().map(|(i, _)| i).find(is_food)?;
        Some((id, food))
    });
    let (food, source) = if let Some((id, food)) = from_leader {
        (food, Supply::Leader(id))
    } else {
        let Some((pos, food)) = chests
            .0
            .iter()
            .filter(|(pos, _)| {
                world.block(**pos) == Block::Chest && (pos.as_vec3() + Vec3::splat(0.5)).distance(s.post) < WORK_RADIUS
            })
            .find_map(|(pos, inv)| inv.iter().map(|(i, _)| i).find(is_food).map(|f| (*pos, f)))
        else {
            return;
        };
        (food, Supply::Chest(pos))
    };
    take_one(source, food, lobby, chests, server);
    if let Some((hunger, heal)) = food.food() {
        s.hunger = (s.hunger + hunger).min(SURVIVOR_MAX_HUNGER);
        s.health = (s.health + heal).min(s.stats.max_health());
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

/// Délai entre deux recherches de chemin vers le même but.
const REPATH_SECS: f32 = 0.8;

/// Prochain point où aller pour rejoindre `goal` : le chemin trouvé par A* (marches, détours),
/// ou tout droit si le but est proche ou introuvable.
fn steer(s: &mut Survivor, goal: Vec3, dt: f32, world: &GameWorld) -> Vec3 {
    s.path_timer -= dt;
    let flat = |a: Vec3, b: Vec3| Vec2::new(a.x - b.x, a.z - b.z).length();
    if flat(goal, s.position) < 1.5 && (goal.y - s.position.y).abs() < 1.0 {
        s.path.clear();
        return goal;
    }
    if s.path_timer <= 0.0 || s.path_goal.distance(goal) > 3.0 {
        s.path_timer = REPATH_SECS * (0.8 + fastrand::f32() * 0.4);
        s.path_goal = goal;
        s.path = crate::pathfind::find_path(s.position, goal, |p| world.block(p)).unwrap_or_default();
    }
    // Les points déjà atteints sont retirés.
    while let Some(next) = s.path.first() {
        if flat(*next, s.position) < 0.45 && (next.y - s.position.y).abs() < 1.2 {
            s.path.remove(0);
        } else {
            break;
        }
    }
    s.path.first().copied().unwrap_or(goal)
}

/// Déplace le survivant vers `goal` (en sautant les marches), tourné vers `face` s'il y en a un.
fn step(s: &mut Survivor, goal: Option<Vec3>, face: Option<Vec3>, speed: f32, dt: f32, world: &GameWorld) {
    let target = goal.map(|g| steer(s, g, dt, world));
    let mut wish = target.map_or(Vec3::ZERO, |g| Vec3::new(g.x - s.position.x, 0.0, g.z - s.position.z));
    if wish.length() < 0.15 {
        wish = Vec3::ZERO;
    }
    let wish = wish.normalize_or_zero() * speed;
    if let Some(target) = face {
        s.yaw = (s.position.x - target.x).atan2(s.position.z - target.z);
    } else if wish != Vec3::ZERO {
        s.yaw = (-wish.x).atan2(-wish.z);
    }
    let mut velocity = Vec3::new(wish.x, s.velocity.y, wish.z);
    // Saute contre un mur, ou quand la prochaine case du chemin est une marche.
    let step_up = target
        .is_some_and(|t| t.y > s.position.y + 0.5 && Vec2::new(t.x - s.position.x, t.z - s.position.z).length() < 1.4);
    if s.on_ground && wish != Vec3::ZERO && (s.hit_wall || step_up) {
        velocity.y = JUMP_SPEED;
    }
    // Dans l'eau, il nage vers la surface.
    let feet = s.position.floor().as_ivec3();
    if world.block(feet + IVec3::Y).is_water() {
        velocity.y = velocity.y.max(2.5);
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
    mut survivors: Query<(Entity, &mut Survivor)>,
    mut server: Option<ResMut<RenetServer>>,
) {
    // Les compagnons proches d'un survivant tombé en sont affectés.
    let fallen: Vec<(String, Vec3)> = survivors
        .iter()
        .filter(|(_, s)| s.health <= 0.0)
        .filter_map(|(_, s)| s.leader.clone().map(|l| (l, s.position)))
        .collect();
    for (_, mut s) in &mut survivors {
        if s.health <= 0.0 {
            continue;
        }
        for (leader, at) in &fallen {
            if s.leader.as_ref() == Some(leader) && s.position.distance(*at) < GRIEF_RADIUS {
                s.grief += GRIEF_PER_DEATH;
            }
        }
    }
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
                if s.leader.is_some() && s.hunger <= 0.0 {
                    flags |= SurvivorState::HUNGRY;
                }
                SurvivorState::new(
                    s.id,
                    s.position,
                    s.yaw,
                    s.health / s.stats.max_health(),
                    flags,
                    s.morale,
                )
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

/// Ce qu'un travail a le droit de modifier.
struct Jobs<'a> {
    world: &'a mut GameWorld,
    version: &'a mut WorldVersion,
    chests: &'a mut Chests,
    lobby: &'a mut Lobby,
    server: Option<&'a mut RenetServer>,
}

impl Jobs<'_> {
    fn set_block(&mut self, pos: IVec3, block: Block) {
        if self.world.set_block(pos, block) {
            self.version.0 += 1;
            if let Some(server) = self.server.as_deref_mut() {
                broadcast(server, &ServerMessage::BlockChanged { pos, block });
            }
        }
    }

    /// Coffre près du poste qui contient au moins `n` exemplaires de l'objet.
    fn chest_with(&self, s: &Survivor, item: Item, n: u32) -> Option<IVec3> {
        self.chest_near(s.post, item, n)
    }

    fn chest_near(&self, post: Vec3, item: Item, n: u32) -> Option<IVec3> {
        self.chests
            .0
            .iter()
            .filter(|(pos, inv)| {
                inv.has(item, n)
                    && self.world.block(**pos) == Block::Chest
                    && (pos.as_vec3() + Vec3::splat(0.5)).distance(post) < WORK_RADIUS
            })
            .map(|(pos, _)| *pos)
            .next()
    }

    fn take(&mut self, chest: IVec3, item: Item, n: u32) {
        for _ in 0..n {
            take_one(
                Supply::Chest(chest),
                item,
                self.lobby,
                self.chests,
                self.server.as_deref_mut(),
            );
        }
    }

    fn deliver(&mut self, s: &Survivor, leader: Option<ClientId>, item: Item, n: u32) {
        deliver(
            s,
            item,
            n,
            leader,
            self.world,
            self.chests,
            self.lobby,
            self.server.as_deref_mut(),
        );
    }
}

/// Rayon du champ d'un fermier autour de son poste.
const FARM_RADIUS: i32 = 6;
/// Durée d'une récolte ou d'un semis.
const FARM_SECS: f32 = 5.0;
/// Durée d'une cuisson.
const COOK_SECS: f32 = 10.0;
/// Distance de chasse autour du poste.
const HUNT_RADIUS: f32 = 40.0;
/// Le moral bouge d'un point toutes les `MORALE_SECS_PER_POINT` secondes.
const MORALE_SECS_PER_POINT: f32 = 3.0;
/// Chagrin causé par la mort d'un compagnon proche, et rayon où il se fait sentir.
const GRIEF_PER_DEATH: f32 = 25.0;
const GRIEF_RADIUS: f32 = 24.0;
/// Le chagrin s'estompe de 25 points en 5 minutes.
const GRIEF_FADE: f32 = 25.0 / 300.0;
/// Secondes au plus bas du moral avant d'abandonner son chef.
const DESERT_AFTER: f32 = 60.0;

/// Bloc de ce type le plus proche de `center`, dans un rayon donné (sur 4 blocs de haut et de bas).
fn find_near(world: &GameWorld, center: Vec3, block: Block, radius: i32) -> Option<IVec3> {
    let c = center.floor().as_ivec3();
    let mut best: Option<(IVec3, i32)> = None;
    for dx in -radius..=radius {
        for dz in -radius..=radius {
            for dy in -4..=4 {
                let p = c + IVec3::new(dx, dy, dz);
                let d = dx * dx + dy * dy + dz * dz;
                if world.block(p) == block && best.is_none_or(|(_, bd)| d < bd) {
                    best = Some((p, d));
                }
            }
        }
    }
    best.map(|(p, _)| p)
}

/// Fermier : récolte le blé mûr (et le ressème), puis sème sur la terre nue avec les graines du coffre.
/// Renvoie où il veut aller.
fn farm(
    s: &mut Survivor,
    leader: Option<ClientId>,
    work_speed: f32,
    dt: f32,
    mut jobs: Jobs,
    face: &mut Option<Vec3>,
) -> Option<Vec3> {
    let still_valid = |p: IVec3, world: &GameWorld| {
        let b = world.block(p);
        b == Block::RipeWheat || (b == Block::Air && world.block(p - IVec3::Y) == Block::Dirt)
    };
    if s.job_target.is_some_and(|p| !still_valid(p, jobs.world)) {
        s.job_target = None;
    }
    if s.job_target.is_none() {
        let post = s.post.floor().as_ivec3();
        let mut ripe = None;
        let mut bare = None;
        for dx in -FARM_RADIUS..=FARM_RADIUS {
            for dz in -FARM_RADIUS..=FARM_RADIUS {
                for dy in -3..=3 {
                    let p = post + IVec3::new(dx, dy, dz);
                    match jobs.world.block(p) {
                        Block::RipeWheat if ripe.is_none() => ripe = Some(p),
                        Block::Air if bare.is_none() && jobs.world.block(p - IVec3::Y) == Block::Dirt => bare = Some(p),
                        _ => {}
                    }
                }
            }
        }
        let has_seeds = jobs.chest_with(s, Item::Seeds, 1).is_some();
        s.job_target = ripe.or(bare.filter(|_| has_seeds));
    }
    let Some(target) = s.job_target else {
        return wander(s, FARM_RADIUS as f32, dt);
    };
    let center = target.as_vec3() + Vec3::new(0.5, 0.0, 0.5);
    if Vec2::new(center.x - s.position.x, center.z - s.position.z).length() > 1.8 {
        return Some(center);
    }
    *face = Some(center);
    s.working = true;
    s.work += dt * work_speed;
    if s.work < FARM_SECS {
        return None;
    }
    s.work = 0.0;
    s.job_target = None;
    if jobs.world.block(target) == Block::RipeWheat {
        // Récolte, et une graine remise en terre aussitôt.
        jobs.set_block(target, Block::Sprout);
        jobs.deliver(s, leader, Item::Wheat, 2);
        if fastrand::bool() {
            jobs.deliver(s, leader, Item::Seeds, 1);
        }
    } else if let Some(chest) = jobs.chest_with(s, Item::Seeds, 1) {
        jobs.take(chest, Item::Seeds, 1);
        jobs.set_block(target, Block::Sprout);
    }
    None
}

/// Cuisinier : au feu de camp près du poste, cuit la viande crue et fait le pain avec le blé du coffre.
fn cook(
    s: &mut Survivor,
    leader: Option<ClientId>,
    work_speed: f32,
    dt: f32,
    mut jobs: Jobs,
    face: &mut Option<Vec3>,
) -> Option<Vec3> {
    let Some(fire) = s.campfire else {
        return (s.post.distance(s.position) > 1.5).then_some(s.post);
    };
    let center = fire.as_vec3() + Vec3::new(0.5, 0.0, 0.5);
    if Vec2::new(center.x - s.position.x, center.z - s.position.z).length() > 2.2 {
        return Some(center);
    }
    *face = Some(center);
    let recipe = if let Some(chest) = jobs.chest_with(s, Item::RawMeat, 1) {
        Some((chest, Item::RawMeat, 1, Item::CookedMeat))
    } else {
        jobs.chest_with(s, Item::Wheat, 3)
            .map(|chest| (chest, Item::Wheat, 3, Item::Bread))
    };
    let (chest, input, n, output) = recipe?;
    s.working = true;
    s.work += dt * work_speed;
    if s.work >= COOK_SECS {
        s.work = 0.0;
        jobs.take(chest, input, n);
        jobs.deliver(s, leader, output, 1);
    }
    None
}

/// Le survivant perd courage et quitte son chef ; il lui rend ce qu'il lui avait confié s'il est là.
fn desert(s: &mut Survivor, lobby: &mut Lobby, mut server: Option<&mut RenetServer>) {
    let Some(leader) = s.leader.take() else { return };
    if let Some((id, player)) = lobby.players.iter_mut().find(|(_, p)| p.name == leader)
        && s.return_gear(&mut player.inventory)
        && let Some(server) = server.as_deref_mut()
    {
        send_inventory(server, *id, player);
    }
    info!("Le survivant {} abandonne {leader} (moral au plus bas)", s.name);
    s.order = Order::Idle;
    s.post = s.position;
    s.despair = 0.0;
    s.morale = MORALE_START;
    s.grief = 0.0;
    if let Some(server) = server {
        broadcast(
            server,
            &ServerMessage::Chat {
                from: "Serveur".into(),
                text: format!("{} a perdu courage et quitte {leader}", s.name),
            },
        );
        broadcast(server, &s.info());
    }
}

#[cfg(test)]
mod tests {
    use shared::config::WorldConfig;
    use shared::daytime::GameTime;
    use shared::world::WorldState;

    use super::*;

    #[test]
    fn a_survivor_climbs_steps_and_walks_around_walls_to_reach_its_goal() {
        let mut world = GameWorld(WorldState::new(WorldConfig::default()));
        let y = world.generator().surface_height(100, 100) + 25;
        for x in 85..=115 {
            for z in 85..=115 {
                world.set_block(IVec3::new(x, y - 1, z), Block::Stone);
            }
        }
        // Un mur trop haut pour sauter, puis un escalier de deux marches vers une estrade.
        for z in 95..=103 {
            for dy in 0..3 {
                world.set_block(IVec3::new(103, y + dy, z), Block::Stone);
            }
        }
        for x in 106..=112 {
            for z in 98..=102 {
                world.set_block(IVec3::new(x, y, z), Block::Planks);
                if x >= 108 {
                    world.set_block(IVec3::new(x, y + 1, z), Block::Planks);
                }
            }
        }
        let start = Vec3::new(100.5, y as f32, 100.5);
        let goal = Vec3::new(110.5, y as f32 + 2.0, 100.5);
        let mut s = Survivor::new(1, "Marie".into(), SurvivorStats::from_seed(3), start);
        for _ in 0..(20.0 * 30.0) as usize {
            step(&mut s, Some(goal), None, SURVIVOR_SPEED, 1.0 / 30.0, &world);
        }
        assert!(s.position.distance(goal) < 1.5, "{:?}", s.position);
    }

    /// Une dalle dans le ciel, un coffre près du poste, un survivant de Alice au travail.
    fn camp(order: Order) -> (App, IVec3, IVec3) {
        let mut world = GameWorld(WorldState::new(WorldConfig::default()));
        let y = world.generator().surface_height(100, 100) + 25;
        for x in 85..=115 {
            for z in 85..=115 {
                world.set_block(IVec3::new(x, y - 1, z), Block::Stone);
            }
        }
        let post = IVec3::new(100, y, 100);
        let chest = post + IVec3::new(3, 0, 0);
        world.set_block(chest, Block::Chest);
        let mut chests = Chests::default();
        chests.0.insert(chest, shared::item::Inventory::default());
        let mut app = App::new();
        app.insert_resource(world)
            .insert_resource(chests)
            .insert_resource(Clock(GameTime { day: 2, phase: 0.2 }))
            .init_resource::<Time>()
            .init_resource::<WorldVersion>()
            .init_resource::<Lobby>()
            .init_resource::<BlockDamage>()
            .add_systems(Update, (survivor_ai, survivor_deaths).chain());
        let mut s = Survivor::new(
            1,
            "Marie".into(),
            SurvivorStats::from_seed(3),
            post.as_vec3() + Vec3::new(0.5, 0.0, 0.5),
        );
        s.leader = Some("Alice".into());
        s.order = order;
        app.world_mut().spawn(s);
        (app, post, chest)
    }

    fn run(app: &mut App, secs: f32) {
        for _ in 0..(secs * 10.0) as usize {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::from_secs_f32(0.1));
            app.update();
        }
    }

    fn chest(app: &App, pos: IVec3) -> shared::item::Inventory {
        app.world().resource::<Chests>().0[&pos].clone()
    }

    fn survivor(app: &mut App) -> Survivor {
        app.world_mut()
            .query::<&Survivor>()
            .single(app.world())
            .unwrap()
            .clone()
    }

    #[test]
    fn the_farmer_harvests_ripe_wheat_and_replants_it() {
        let (mut app, post, chest_pos) = camp(Order::Farm);
        let field = post + IVec3::new(-2, 0, 2);
        {
            let mut world = app.world_mut().resource_mut::<GameWorld>();
            world.set_block(field - IVec3::Y, Block::Dirt);
            world.set_block(field, Block::RipeWheat);
        }
        run(&mut app, 30.0);
        assert!(chest(&app, chest_pos).has(Item::Wheat, 2));
        assert_eq!(app.world().resource::<GameWorld>().block(field), Block::Sprout);
    }

    #[test]
    fn the_farmer_sows_bare_soil_with_seeds_from_the_chest() {
        let (mut app, post, chest_pos) = camp(Order::Farm);
        let field = post + IVec3::new(2, 0, -2);
        app.world_mut()
            .resource_mut::<GameWorld>()
            .set_block(field - IVec3::Y, Block::Dirt);
        app.world_mut()
            .resource_mut::<Chests>()
            .0
            .get_mut(&chest_pos)
            .unwrap()
            .add(Item::Seeds, 1);
        run(&mut app, 30.0);
        assert_eq!(app.world().resource::<GameWorld>().block(field), Block::Sprout);
        assert!(!chest(&app, chest_pos).has(Item::Seeds, 1));
    }

    #[test]
    fn the_cook_turns_raw_meat_into_meals_by_the_fire() {
        let (mut app, post, chest_pos) = camp(Order::Cook);
        app.world_mut()
            .resource_mut::<GameWorld>()
            .set_block(post + IVec3::new(0, 0, -3), Block::Campfire);
        app.world_mut()
            .resource_mut::<Chests>()
            .0
            .get_mut(&chest_pos)
            .unwrap()
            .add(Item::RawMeat, 2);
        run(&mut app, 40.0);
        let inv = chest(&app, chest_pos);
        assert!(inv.has(Item::CookedMeat, 1), "{inv:?}");
        assert!(!inv.has(Item::RawMeat, 2));
    }

    #[test]
    fn the_medic_heals_the_wounded_and_rolls_bandages() {
        let (mut app, _, chest_pos) = camp(Order::Heal);
        {
            let mut q = app.world_mut().query::<&mut Survivor>();
            let mut s = q.single_mut(app.world_mut()).unwrap();
            s.health = 10.0;
        }
        app.world_mut()
            .resource_mut::<Chests>()
            .0
            .get_mut(&chest_pos)
            .unwrap()
            .add(Item::Herb, 2);
        run(&mut app, 45.0);
        let s = survivor(&mut app);
        assert!(s.health > 30.0, "{}", s.health);
        let inv = chest(&app, chest_pos);
        assert!(inv.has(Item::Bandage, 1) && !inv.has(Item::Herb, 1), "{inv:?}");
    }

    #[test]
    fn the_engineer_refills_turrets_from_the_chest() {
        let (mut app, post, chest_pos) = camp(Order::Engineer);
        let turret = post + IVec3::new(-3, 0, 1);
        app.world_mut()
            .resource_mut::<GameWorld>()
            .set_block(turret, Block::Turret);
        let mut chests = app.world_mut().resource_mut::<Chests>();
        chests.0.insert(turret, shared::item::Inventory::default());
        chests.0.get_mut(&chest_pos).unwrap().add(Item::Ammo, 20);
        run(&mut app, 30.0);
        let loaded = chest(&app, turret).count(Item::Ammo);
        assert!((TURRET_FULL..=TURRET_FULL + TURRET_LOAD).contains(&loaded), "{loaded}");
        assert_eq!(chest(&app, chest_pos).count(Item::Ammo), 20 - loaded);
    }

    #[test]
    fn the_hunter_brings_back_meat_and_hide() {
        let (mut app, post, chest_pos) = camp(Order::Hunt);
        let at = post.as_vec3() + Vec3::new(8.5, 0.0, 6.5);
        app.world_mut()
            .spawn(Animal::new(7, shared::animal::AnimalKind::Deer, at));
        run(&mut app, 40.0);
        let inv = chest(&app, chest_pos);
        assert!(inv.has(Item::RawMeat, 1) && inv.has(Item::Hide, 1), "{inv:?}");
    }

    #[test]
    fn a_survivor_at_the_end_of_their_rope_leaves() {
        let (mut app, _, _) = camp(Order::Guard);
        {
            let mut q = app.world_mut().query::<&mut Survivor>();
            let mut s = q.single_mut(app.world_mut()).unwrap();
            s.morale = 5.0;
            s.hunger = 0.0;
            s.grief = 40.0;
        }
        run(&mut app, 70.0);
        let s = survivor(&mut app);
        assert_eq!(s.leader, None);
        assert_eq!(s.order, Order::Idle);
    }

    #[test]
    fn a_well_kept_survivor_cheers_up() {
        let (mut app, post, _) = camp(Order::Guard);
        app.world_mut()
            .resource_mut::<GameWorld>()
            .set_block(post + IVec3::new(0, 0, -3), Block::Campfire);
        {
            let mut q = app.world_mut().query::<&mut Survivor>();
            let mut s = q.single_mut(app.world_mut()).unwrap();
            s.weapon = Some(Item::Sword(shared::item::Tier::Stone));
            s.since_hit = 100.0;
        }
        run(&mut app, 60.0);
        assert!(survivor(&mut app).morale >= shared::survivor::MORALE_HIGH);
    }
}

/// Un marchand passe près d'un joueur quand l'événement du jour le prévoit ; il repart à la nuit.
fn spawn_traders(
    mut commands: Commands,
    clock: Res<Clock>,
    lobby: Res<Lobby>,
    world: Res<GameWorld>,
    mut ids: ResMut<NextSurvivorId>,
    plans: Option<ResMut<crate::events::Happenings>>,
    traders: Query<(Entity, &Survivor)>,
    mut server: Option<ResMut<RenetServer>>,
) {
    if clock.is_night() {
        for (entity, s) in traders.iter().filter(|(_, s)| !s.offers.is_empty()) {
            commands.entity(entity).despawn();
            if let Some(server) = server.as_deref_mut() {
                broadcast(
                    server,
                    &ServerMessage::Chat {
                        from: "Serveur".into(),
                        text: format!("{} repart avant la nuit.", s.name),
                    },
                );
            }
        }
        return;
    }
    let Some(mut plans) = plans else { return };
    if !plans.trader_due {
        return;
    }
    plans.trader_due = false;
    let players: Vec<(&str, Vec3)> = lobby
        .players
        .values()
        .filter(|p| p.is_alive())
        .map(|p| (p.name.as_str(), p.state.position))
        .collect();
    if players.is_empty() {
        return;
    }
    let (player_name, around) = players[fastrand::usize(0..players.len())];
    let Some(spot) = find_spawn_spot(&world, around, 12.0, 25.0) else {
        return;
    };
    let id = next_id(&mut ids);
    let seed = fastrand::u32(..);
    let name = format!("{} le marchand", name_from_seed(seed));
    let mut trader = Survivor::new(id, name.clone(), SurvivorStats::from_seed(seed), spot);
    let mut deck = TRADES.to_vec();
    fastrand::shuffle(&mut deck);
    trader.offers = deck.into_iter().take(3).collect();
    if let Some(server) = server.as_deref_mut() {
        broadcast(server, &trader.info());
        broadcast(
            server,
            &ServerMessage::Chat {
                from: "Serveur".into(),
                text: format!(
                    "{name} passe {}, près de {player_name}, jusqu'au soir.",
                    crate::events::compass(around, spot)
                ),
            },
        );
    }
    commands.spawn(trader);
}

#[cfg(test)]
mod trader_tests {
    use super::*;
    use crate::net::ConnectedPlayer;

    #[test]
    fn a_trader_swaps_goods_instead_of_joining() {
        let mut trader = Survivor::new(
            9,
            "Ambroise le marchand".into(),
            SurvivorStats::from_seed(1),
            Vec3::ZERO,
        );
        trader.offers = vec![TRADES[0]];
        let mut player = ConnectedPlayer::new(Vec3::new(1.0, 0.0, 0.0));
        player.name = "Alice".into();
        player.inventory.add(Item::Hide, 5);
        player.inventory.add(Item::Ration, 1);
        let mut lobby = Lobby::default();
        lobby.players.insert(1, player);
        let mut app = App::new();
        app.insert_resource(lobby)
            .init_resource::<SurvivorTalk>()
            .add_systems(Update, handle_talk);
        app.world_mut().spawn(trader);
        let talk = TalkRequest {
            client: 1,
            id: 9,
            dismiss: false,
            give: false,
        };
        app.world_mut().resource_mut::<SurvivorTalk>().0.push(talk);
        app.update();
        let lobby = app.world().resource::<Lobby>();
        let inv = &lobby.players[&1].inventory;
        assert!(inv.has(Item::Ammo, 12) && inv.has(Item::Hide, 1) && !inv.has(Item::Hide, 2));
        assert!(inv.has(Item::Ration, 1), "la ration n'a pas servi à le recruter");
        let s = app.world_mut().query::<&Survivor>().single(app.world()).unwrap();
        assert_eq!(s.leader, None);
    }
}
