//! Réseau côté serveur : connexions, messages des joueurs, diffusion de l'état.

use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::time::SystemTime;

use bevy::prelude::*;
use bevy_renet::netcode::{
    NetcodeServerPlugin, NetcodeServerTransport, ServerAuthentication, ServerConfig as NetcodeConfig,
};
use bevy_renet::renet::{ClientId, ConnectionConfig, DefaultChannel, ServerEvent};
use bevy_renet::{RenetServer, RenetServerEvent, RenetServerPlugin};
use shared::block::{Block, DEPOSIT_HARVESTS};
use shared::crafting::{RECIPES, station_near};
use shared::item::{Inventory, Item, break_time, drop_for};
use shared::physics::{PLAYER_EYE_HEIGHT, PLAYER_SIZE, overlaps_solid};
use shared::protocol::{ClientMessage, PROTOCOL_ID, PlayerState, REACH, ServerMessage, decode, encode};
use shared::raycast::{ray_aabb, raycast};
use shared::structure::would_hold;

use crate::ServerConfig;
use crate::animals::Animal;
use crate::claims::{ClaimRefusal, Claims};
use crate::explosives::Projectiles;
use crate::save::{SavedPlayer, SavedPlayers};
use crate::survivors::SurvivorTalk;
use crate::world::{Chests, Deposits, GameWorld, SpawnPoint};
use crate::zombies::{Zombie, ZombieBody};

/// Portée d'un coup de mêlée, en mètres.
pub const MELEE_REACH: f32 = 3.0;

/// Nombre de blocs modifiés par message lors du rattrapage d'un joueur qui arrive.
const EDITS_PER_MESSAGE: usize = 2048;
/// Distance maximale parcourue entre deux mises à jour avant que le serveur ne refuse (anti-téléportation).
const MAX_MOVE_PER_UPDATE: f32 = 6.0;

pub struct NetPlugin;

impl Plugin for NetPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((RenetServerPlugin, NetcodeServerPlugin))
            .init_resource::<Lobby>()
            .init_resource::<WorldVersion>()
            .add_systems(PreStartup, open_socket)
            .add_observer(on_server_event)
            .add_systems(Update, receive_messages.run_if(resource_exists::<RenetServer>))
            .add_systems(FixedUpdate, broadcast_players.run_if(resource_exists::<RenetServer>));
    }
}

/// Un joueur connecté.
#[derive(Debug, Clone)]
pub struct ConnectedPlayer {
    /// Vide tant que le joueur n'a pas envoyé `Hello`.
    pub name: String,
    pub state: PlayerState,
    pub health: f32,
    /// Secondes depuis le dernier coup reçu (pour la régénération).
    pub since_hit: f32,
    /// Temps avant de pouvoir frapper à nouveau.
    pub attack_cooldown: f32,
    /// Faim : 100 = rassasié, 0 = le joueur perd de la vie.
    pub hunger: f32,
    /// Soif : 100 = désaltéré, 0 = le joueur perd de la vie.
    pub thirst: f32,
    pub inventory: Inventory,
    /// Objet en main, toujours présent dans l'inventaire.
    pub held: Option<Item>,
    /// Bloc en cours de minage et instant du début (secondes depuis le lancement du serveur).
    pub mining: Option<(IVec3, f64)>,
    /// Coffre ouvert par le joueur.
    pub open_chest: Option<IVec3>,
    /// Depuis combien de secondes le joueur tient sa garde levée.
    pub guarding: Option<f32>,
    /// Température du corps, en °C.
    pub body_temp: f32,
    /// Air ressenti autour du joueur, avec ses vêtements, en °C.
    pub felt: f32,
    /// Avancée de l'infection : 0 = sain, puis de 1 à 100.
    pub infection: f32,
}

/// Durée après avoir levé la garde pendant laquelle un coup est paré.
pub const PARRY_WINDOW: f32 = 0.3;
/// Part des dégâts qui passe à travers la garde.
const GUARD_FACTOR: f32 = 0.35;

/// Santé maximale d'un joueur.
pub const MAX_HEALTH: f32 = 100.0;
/// Faim maximale d'un joueur.
pub const MAX_HUNGER: f32 = 100.0;
/// Soif maximale d'un joueur.
pub const MAX_THIRST: f32 = 100.0;
/// Chance qu'un bloc de feuilles donne aussi une pomme.
const APPLE_CHANCE: f32 = 0.25;
/// Chance qu'un bloc d'herbe donne aussi des graines.
const SEED_CHANCE: f32 = 0.15;
/// Chance qu'une touffe d'herbes hautes fauchée donne des graines.
const GRASS_SEED_CHANCE: f32 = 0.3;
/// Tolérance sur la durée de minage (latence, horloges).
const MINING_TOLERANCE: f64 = 0.75;

impl ConnectedPlayer {
    pub fn new(position: Vec3) -> Self {
        let mut inventory = Inventory::default();
        inventory.add(Item::Apple, 3);
        Self {
            name: String::new(),
            state: PlayerState {
                position,
                yaw: 0.0,
                pitch: 0.0,
                swinging: false,
            },
            health: MAX_HEALTH,
            since_hit: 0.0,
            attack_cooldown: 0.0,
            hunger: MAX_HUNGER,
            thirst: MAX_THIRST,
            inventory,
            held: None,
            mining: None,
            open_chest: None,
            guarding: None,
            body_temp: shared::climate::BODY_NORMAL,
            felt: 18.0,
            infection: 0.0,
        }
    }

    /// Garde levée avec une arme de mêlée en main.
    pub fn guard_time(&self) -> Option<f32> {
        self.guarding
            .filter(|_| self.held_item().is_some_and(Item::is_melee_weapon))
    }

    /// Dégâts réellement subis d'un coup venu de `from` : la garde levée face au coup
    /// les réduit (ou les annule si elle vient d'être levée : parade), l'armure aussi.
    /// Renvoie aussi vrai si le coup a été paré.
    pub fn incoming_damage(&self, amount: f32, from: Vec3) -> (f32, bool) {
        let look = Quat::from_rotation_y(self.state.yaw) * Vec3::NEG_Z;
        let to = Vec3::new(from.x - self.state.position.x, 0.0, from.z - self.state.position.z).normalize_or_zero();
        let facing = look.dot(to) > 0.2;
        let (amount, parried) = match self.guard_time() {
            Some(t) if facing && t <= PARRY_WINDOW => (0.0, true),
            Some(_) if facing => (amount * GUARD_FACTOR, false),
            _ => (amount, false),
        };
        (amount * (1.0 - self.inventory.armor_reduction()), parried)
    }

    /// Objet en main s'il est encore dans l'inventaire.
    pub fn held_item(&self) -> Option<Item> {
        self.held.filter(|item| self.inventory.has(*item, 1))
    }
}

/// Envoie au joueur sa santé et sa faim.
pub fn send_vitals(server: &mut RenetServer, client: ClientId, player: &ConnectedPlayer) {
    send(
        server,
        client,
        &ServerMessage::Vitals {
            health: player.health,
            hunger: player.hunger,
            thirst: player.thirst,
            body_temp: player.body_temp,
            felt: player.felt,
            infection: player.infection,
        },
    );
}

pub(crate) fn send_inventory(server: &mut RenetServer, client: ClientId, player: &ConnectedPlayer) {
    send(server, client, &ServerMessage::Inventory(player.inventory.clone()));
}

impl ConnectedPlayer {
    pub fn is_ready(&self) -> bool {
        !self.name.is_empty()
    }

    /// Prêt et vivant : les zombies le poursuivent.
    pub fn is_alive(&self) -> bool {
        self.is_ready() && self.health > 0.0
    }
}

/// Augmente à chaque bloc modifié : le champ de flux des zombies se recalcule.
#[derive(Resource, Default, Debug)]
pub struct WorldVersion(pub u64);

/// Joueurs connectés, par identifiant.
#[derive(Resource, Default, Debug)]
pub struct Lobby {
    pub players: HashMap<ClientId, ConnectedPlayer>,
}

fn open_socket(mut commands: Commands, config: Res<ServerConfig>) {
    let addr: SocketAddr = ([0, 0, 0, 0], config.port).into();
    let socket = match UdpSocket::bind(addr) {
        Ok(socket) => socket,
        Err(error) => {
            error!("Impossible d'ouvrir le port UDP {} : {error}", config.port);
            return;
        }
    };
    let current_time = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let netcode = NetcodeConfig {
        current_time,
        max_clients: config.max_players as usize,
        protocol_id: PROTOCOL_ID,
        public_addresses: vec![addr],
        authentication: ServerAuthentication::Unsecure,
    };
    match NetcodeServerTransport::new(netcode, socket) {
        Ok(transport) => {
            commands.insert_resource(RenetServer::new(ConnectionConfig::default()));
            commands.insert_resource(transport);
            info!("En écoute sur UDP {addr}");
        }
        Err(error) => error!("Impossible de démarrer le transport réseau : {error}"),
    }
}

pub(crate) fn send(server: &mut RenetServer, client: ClientId, message: &ServerMessage) {
    server.send_message(client, DefaultChannel::ReliableOrdered, encode(message));
}

pub(crate) fn broadcast(server: &mut RenetServer, message: &ServerMessage) {
    server.broadcast_message(DefaultChannel::ReliableOrdered, encode(message));
}

fn on_server_event(
    event: On<RenetServerEvent>,
    mut server: ResMut<RenetServer>,
    mut lobby: ResMut<Lobby>,
    mut saved: ResMut<SavedPlayers>,
    spawn: Res<SpawnPoint>,
) {
    match **event {
        ServerEvent::ClientConnected { client_id } => {
            lobby.players.insert(client_id, ConnectedPlayer::new(spawn.0));
        }
        ServerEvent::ClientDisconnected { client_id, ref reason } => {
            if let Some(player) = lobby.players.remove(&client_id)
                && player.is_ready()
            {
                info!("{} est parti ({reason})", player.name);
                saved.0.insert(player.name.clone(), SavedPlayer::from_player(&player));
                broadcast(&mut server, &ServerMessage::PlayerLeft { id: client_id });
            }
        }
    }
}

fn receive_messages(
    mut server: ResMut<RenetServer>,
    mut lobby: ResMut<Lobby>,
    mut world: ResMut<GameWorld>,
    mut version: ResMut<WorldVersion>,
    mut zombies: Query<(&mut Zombie, &ZombieBody)>,
    spawn: Res<SpawnPoint>,
    time: Res<Time>,
    mut saved: ResMut<SavedPlayers>,
    mut deposits: ResMut<Deposits>,
    mut chests: ResMut<Chests>,
    mut talk: ResMut<SurvivorTalk>,
    mut projectiles: ResMut<Projectiles>,
    mut animals: Query<&mut Animal>,
    mut claims: ResMut<Claims>,
    config: Res<ServerConfig>,
) {
    for client in server.clients_id() {
        while let Some(bytes) = server.receive_message(client, DefaultChannel::ReliableOrdered) {
            let Some(message) = decode::<ClientMessage>(&bytes) else {
                warn!("Message illisible du client {client}, déconnexion");
                server.disconnect(client);
                break;
            };
            let mut ctx = Context {
                server: &mut server,
                lobby: &mut lobby,
                world: &mut world,
                version: &mut version,
                spawn: spawn.0,
                now: time.elapsed_secs_f64(),
                saved: &mut saved,
                deposits: &mut deposits,
                chests: &mut chests,
                talk: &mut talk,
                projectiles: &mut projectiles,
                claims: &mut claims,
                pvp: config.pvp,
            };
            handle_reliable(client, message, &mut ctx, &mut zombies, &mut animals);
        }
        while let Some(bytes) = server.receive_message(client, DefaultChannel::Unreliable) {
            if let Some(ClientMessage::State(state)) = decode::<ClientMessage>(&bytes) {
                handle_state(client, state, &mut server, &mut lobby, &world);
            }
        }
    }
}

struct Context<'a> {
    server: &'a mut RenetServer,
    lobby: &'a mut Lobby,
    world: &'a mut GameWorld,
    version: &'a mut WorldVersion,
    spawn: Vec3,
    now: f64,
    saved: &'a mut SavedPlayers,
    deposits: &'a mut Deposits,
    chests: &'a mut Chests,
    talk: &'a mut SurvivorTalk,
    projectiles: &'a mut Projectiles,
    claims: &'a mut Claims,
    pvp: bool,
}

fn handle_reliable(
    client: ClientId,
    message: ClientMessage,
    ctx: &mut Context,
    zombies: &mut Query<(&mut Zombie, &ZombieBody)>,
    animals: &mut Query<&mut Animal>,
) {
    let Context {
        server,
        lobby,
        world,
        version,
        spawn,
        now,
        saved,
        deposits,
        chests,
        talk,
        projectiles,
        claims,
        pvp,
    } = ctx;
    let (spawn, now, pvp) = (*spawn, *now, *pvp);
    let ready = lobby.players.get(&client).is_some_and(ConnectedPlayer::is_ready);
    // En JcJ, rien ne se touche dans la base d'un autre joueur.
    let touched = match &message {
        ClientMessage::BreakBlock { pos }
        | ClientMessage::PlaceBlock { pos, .. }
        | ClientMessage::StartMining { pos }
        | ClientMessage::OpenChest { pos }
        | ClientMessage::ToggleDoor { pos }
        | ClientMessage::PourWater { pos }
        | ClientMessage::FireEmplacement { pos, .. } => Some(*pos),
        _ => None,
    };
    if pvp
        && ready
        && let Some(pos) = touched
        && let Some(player) = lobby.players.get(&client)
        && let Some(owner) = claims.foreign_owner(pos, &player.name)
    {
        if !matches!(message, ClientMessage::StartMining { .. }) {
            notify(server, client, format!("Base de {owner} : protégée."));
        }
        if matches!(
            message,
            ClientMessage::BreakBlock { .. } | ClientMessage::PlaceBlock { .. }
        ) {
            let block = world.block(pos);
            send(server, client, &ServerMessage::BlockChanged { pos, block });
        }
        return;
    }
    match message {
        ClientMessage::Hello { name } if !ready => {
            let name = sanitize_name(&name, client);
            info!("{name} a rejoint la partie");
            // Un joueur qui revient retrouve sa position, sa santé et son inventaire.
            let name_taken = lobby.players.values().any(|p| p.name == name);
            let returning = if name_taken { None } else { saved.0.remove(&name) };
            let start = returning.as_ref().map_or(spawn, |r| r.position);
            send(
                server,
                client,
                &ServerMessage::Welcome {
                    your_id: client,
                    your_name: name.clone(),
                    world: world.config().clone(),
                    spawn: start,
                    pvp,
                },
            );
            send(server, client, &claims.message());
            let edits: Vec<_> = world.edits().iter().map(|(p, b)| (*p, *b)).collect();
            for batch in edits.chunks(EDITS_PER_MESSAGE) {
                send(server, client, &ServerMessage::WorldEdits(batch.to_vec()));
            }
            for (id, other) in lobby.players.iter().filter(|(_, p)| p.is_ready()) {
                send(
                    server,
                    client,
                    &ServerMessage::PlayerJoined {
                        id: *id,
                        name: other.name.clone(),
                    },
                );
            }
            server.broadcast_message_except(
                client,
                DefaultChannel::ReliableOrdered,
                encode(&ServerMessage::PlayerJoined {
                    id: client,
                    name: name.clone(),
                }),
            );
            if let Some(player) = lobby.players.get_mut(&client) {
                player.name = name;
                if let Some(back) = returning {
                    player.state.position = back.position;
                    player.health = back.health;
                    player.hunger = back.hunger;
                    player.thirst = back.thirst;
                    player.infection = back.infection;
                    player.inventory = back.inventory;
                }
                send_inventory(server, client, player);
                send_vitals(server, client, player);
                if player.health <= 0.0 {
                    send(
                        server,
                        client,
                        &ServerMessage::Died {
                            cause: "Mort avant de partir".into(),
                        },
                    );
                }
            }
        }
        _ if !ready => {}
        ClientMessage::Hello { .. } | ClientMessage::State(_) => {}
        ClientMessage::Attack { zombie } => {
            let Some(player) = lobby.players.get_mut(&client) else {
                return;
            };
            if !player.is_alive() || player.attack_cooldown > 0.0 || player.guard_time().is_some() {
                return;
            }
            let melee = Item::melee(player.held_item());
            let eye = player.state.position + Vec3::Y * PLAYER_EYE_HEIGHT;
            let look = Quat::from_euler(EulerRot::YXZ, player.state.yaw, 0.0, 0.0) * Vec3::NEG_Z;
            let flat_look = Vec3::new(look.x, 0.0, look.z).normalize_or_zero();
            let in_reach = |z: &Zombie, body: &ZombieBody| {
                let center = body.position + Vec3::Y * z.kind.stats().size.height * 0.5;
                center.distance(eye) <= melee.reach + 1.0
            };
            let Some(target) = zombies
                .iter()
                .find(|(z, b)| z.id == zombie && in_reach(z, b))
                .map(|(_, b)| b.position)
            else {
                return;
            };
            // Petite marge : le client et le serveur ne mesurent pas le temps tout à fait pareil.
            player.attack_cooldown = melee.cooldown * 0.85;
            for (mut z, body) in zombies.iter_mut() {
                // La masse balaie tous les zombies devant le joueur.
                let to = Vec3::new(body.position.x - target.x, 0.0, body.position.z - target.z);
                let ahead = Vec3::new(body.position.x - eye.x, 0.0, body.position.z - eye.z).normalize_or_zero();
                let hit = z.id == zombie
                    || (melee.sweep
                        && z.health > 0.0
                        && in_reach(&z, body)
                        && (ahead.dot(flat_look) > 0.5 || to.length() < 1.0));
                if hit {
                    z.health -= melee.damage;
                    z.hurt_timer = 0.3;
                    z.knockback = flat_look * melee.knockback + Vec3::Y * 3.0;
                    z.last_attacker = Some(client);
                }
            }
        }
        ClientMessage::AttackPlayer { id } => {
            let Some(player) = lobby.players.get_mut(&client) else {
                return;
            };
            if !pvp
                || id == client
                || !player.is_alive()
                || player.attack_cooldown > 0.0
                || player.guard_time().is_some()
            {
                return;
            }
            let melee = Item::melee(player.held_item());
            let eye = player.state.position + Vec3::Y * PLAYER_EYE_HEIGHT;
            let in_reach = lobby
                .players
                .get(&id)
                .is_some_and(|v| (v.state.position + Vec3::Y * 0.9).distance(eye) <= melee.reach + 1.0);
            if !in_reach {
                return;
            }
            if let Some(player) = lobby.players.get_mut(&client) {
                player.attack_cooldown = melee.cooldown * 0.85;
            }
            pvp_hit(server, lobby, claims, client, id, melee.damage);
        }
        ClientMessage::Throw { direction } => {
            let Some(player) = lobby.players.get_mut(&client) else {
                return;
            };
            let Some((item, _)) = player.held_item().and_then(|i| i.throwable().map(|t| (i, t))) else {
                return;
            };
            let dir = direction.normalize_or_zero();
            if !player.is_alive() || player.attack_cooldown > 0.0 || dir == Vec3::ZERO || !dir.is_finite() {
                return;
            }
            player.inventory.remove(item, 1);
            player.attack_cooldown = crate::explosives::THROW_COOLDOWN * 0.85;
            let eye = player.state.position + Vec3::Y * PLAYER_EYE_HEIGHT;
            let projectile = crate::explosives::Projectile::thrown(client, item, eye, dir);
            send_inventory(server, client, player);
            broadcast(server, &projectile.message());
            projectiles.0.push(projectile);
        }
        ClientMessage::FireEmplacement { pos, direction } => {
            let Some(player) = lobby.players.get_mut(&client) else {
                return;
            };
            let Some(gun) = world.block(pos).emplacement() else {
                return;
            };
            let dir = direction.normalize_or_zero();
            let center = pos.as_vec3() + Vec3::splat(0.5);
            let close = center.distance(player.state.position + Vec3::Y) <= shared::block::EMPLACEMENT_REACH;
            if !player.is_alive() || player.attack_cooldown > 0.0 || !close || dir == Vec3::ZERO || !dir.is_finite() {
                return;
            }
            if !player.inventory.remove(gun.ammo, 1) {
                send_inventory(server, client, player);
                return;
            }
            player.attack_cooldown = gun.cooldown * 0.85;
            send_inventory(server, client, player);
            // Le tir part du haut de l'arme, un peu devant.
            let origin = center + Vec3::Y * 0.9 + dir * 0.6;
            if gun.speed == 0.0 {
                // Mitrailleuse : légère dispersion.
                let spread = Vec3::new(fastrand::f32() - 0.5, fastrand::f32() - 0.5, fastrand::f32() - 0.5) * 0.04;
                let dir = (dir + spread).normalize_or_zero();
                let end = hitscan(origin, dir, 70.0, gun.damage, 1, client, world, zombies, animals);
                broadcast(
                    server,
                    &ServerMessage::Shot {
                        from: client,
                        origin,
                        end,
                        rifle: true,
                    },
                );
            } else {
                let shell = crate::explosives::Projectile::shell(client, gun, origin, dir);
                broadcast(server, &shell.message());
                projectiles.0.push(shell);
            }
        }
        ClientMessage::AttackAnimal { id } => {
            let Some(player) = lobby.players.get_mut(&client) else {
                return;
            };
            if !player.is_alive() || player.attack_cooldown > 0.0 || player.guard_time().is_some() {
                return;
            }
            let melee = Item::melee(player.held_item());
            let eye = player.state.position + Vec3::Y * PLAYER_EYE_HEIGHT;
            let look = Quat::from_rotation_y(player.state.yaw) * Vec3::NEG_Z;
            if let Some(mut a) = animals.iter_mut().find(|a| a.id == id) {
                let (min, max) = a.bounds();
                if ((min + max) * 0.5).distance(eye) <= melee.reach + 1.0 {
                    player.attack_cooldown = melee.cooldown * 0.85;
                    a.hurt(melee.damage, Some(client), look * melee.knockback * 0.5);
                }
            }
        }
        ClientMessage::Guard { active } => {
            if let Some(player) = lobby.players.get_mut(&client) {
                player.guarding = match (active, player.guarding) {
                    (true, Some(t)) => Some(t),
                    (true, None) => Some(0.0),
                    (false, _) => None,
                };
            }
        }
        ClientMessage::Shoot { direction } => {
            let Some(player) = lobby.players.get_mut(&client) else {
                return;
            };
            let Some(weapon) = player.held_item().and_then(Item::ranged) else {
                return;
            };
            let dir = direction.normalize_or_zero();
            if !player.is_alive() || player.attack_cooldown > 0.0 || dir == Vec3::ZERO || !dir.is_finite() {
                return;
            }
            if !player.inventory.remove(weapon.ammo, 1) {
                send_inventory(server, client, player);
                return;
            }
            // Petite marge : le client et le serveur ne mesurent pas le temps tout à fait pareil.
            player.attack_cooldown = weapon.cooldown * 0.85;
            let eye = player.state.position + Vec3::Y * PLAYER_EYE_HEIGHT;
            // En JcJ, un joueur sur la trajectoire arrête le tir.
            let victim = if pvp {
                aimed_player(lobby, client, eye, dir, weapon.range, world)
            } else {
                None
            };
            let range = victim.map_or(weapon.range, |(_, t)| t);
            let end = hitscan(
                eye,
                dir,
                range,
                weapon.damage,
                weapon.pierce,
                client,
                world,
                zombies,
                animals,
            );
            if let Some((id, t)) = victim
                && end.distance(eye) >= t - 0.05
            {
                pvp_hit(server, lobby, claims, client, id, weapon.damage);
            }
            let Some(player) = lobby.players.get_mut(&client) else {
                return;
            };
            send_inventory(server, client, player);
            broadcast(
                server,
                &ServerMessage::Shot {
                    from: client,
                    origin: eye,
                    end,
                    rifle: weapon.ammo == Item::Ammo,
                },
            );
        }
        ClientMessage::Respawn => {
            let Some(player) = lobby.players.get_mut(&client) else {
                return;
            };
            if player.health <= 0.0 {
                player.health = MAX_HEALTH;
                player.hunger = player.hunger.max(MAX_HUNGER * 0.5);
                player.thirst = player.thirst.max(MAX_THIRST * 0.5);
                player.body_temp = shared::climate::BODY_NORMAL;
                player.infection = 0.0;
                player.since_hit = 0.0;
                player.state.position = spawn;
                send(server, client, &ServerMessage::Teleport { position: spawn });
                send_vitals(server, client, player);
            }
        }
        ClientMessage::StartMining { pos } => {
            if let Some(player) = lobby.players.get_mut(&client) {
                player.mining = Some((pos, now));
            }
        }
        ClientMessage::Hold { item } => {
            if let Some(player) = lobby.players.get_mut(&client) {
                player.held = item;
            }
        }
        ClientMessage::Craft { recipe } => {
            let Some(player) = lobby.players.get_mut(&client) else {
                return;
            };
            let feet = player.state.position.floor().as_ivec3();
            if let Some(recipe) = RECIPES.get(recipe as usize)
                && player.is_alive()
                && recipe.station.is_none_or(|s| station_near(feet, s, |p| world.block(p)))
            {
                recipe.craft(&mut player.inventory);
            }
            send_inventory(server, client, player);
        }
        ClientMessage::ToggleDoor { pos } => {
            let Some(player) = lobby.players.get(&client) else {
                return;
            };
            let block = world.block(pos);
            if !player.is_alive() || !block.is_door() || !within_reach(player, pos) {
                return;
            }
            // Les portes empilées (une porte fait souvent deux blocs de haut) bougent ensemble.
            let mut column = vec![pos];
            for dir in [IVec3::Y, IVec3::NEG_Y] {
                let mut p = pos + dir;
                while world.block(p) == block && column.len() < 4 {
                    column.push(p);
                    p += dir;
                }
            }
            let next = match block {
                Block::Door => Block::DoorOpen,
                Block::DoorOpen => Block::Door,
                Block::ElectricDoor => Block::ElectricDoorOpen,
                _ => Block::ElectricDoor,
            };
            if next.is_solid() {
                let blocked = lobby
                    .players
                    .values()
                    .any(|p| overlaps_solid(p.state.position, PLAYER_SIZE, &|b| column.contains(&b)));
                if blocked {
                    return;
                }
            }
            for p in column {
                world.set_block(p, next);
                broadcast(server, &ServerMessage::BlockChanged { pos: p, block: next });
            }
            version.0 += 1;
        }
        ClientMessage::OpenChest { pos } => {
            let Some(player) = lobby.players.get_mut(&client) else {
                return;
            };
            if player.is_alive() && world.block(pos).is_storage() && within_reach(player, pos) {
                chests.fill_ruin_chest(world, pos);
                player.open_chest = Some(pos);
                let contents = chests.0.get(&pos).cloned().unwrap_or_default();
                send(server, client, &ServerMessage::Chest { pos, contents });
            }
        }
        ClientMessage::CloseChest => {
            if let Some(player) = lobby.players.get_mut(&client) {
                player.open_chest = None;
            }
        }
        ClientMessage::ChestPut { item, count } | ClientMessage::ChestTake { item, count } => {
            let put = matches!(message, ClientMessage::ChestPut { .. });
            let Some(player) = lobby.players.get_mut(&client) else {
                return;
            };
            let Some(pos) = player.open_chest else { return };
            if !player.is_alive() || !world.block(pos).is_storage() || !within_reach(player, pos) {
                player.open_chest = None;
                send(server, client, &ServerMessage::ChestClosed);
                return;
            }
            let chest = chests.0.entry(pos).or_default();
            let (from, to) = if put {
                (&mut player.inventory, chest)
            } else {
                (chest, &mut player.inventory)
            };
            let moved = count.min(from.count(item));
            if moved > 0 {
                from.remove(item, moved);
                to.add(item, moved);
            }
            if chests.0.get(&pos).is_some_and(Inventory::is_empty) && !Chests::is_ruin_chest(world, pos) {
                chests.0.remove(&pos);
            }
            send_inventory(server, client, player);
            send_chest_to_viewers(server, lobby, chests, pos);
        }
        ClientMessage::Chat { text } => {
            let Some(player) = lobby.players.get(&client) else {
                return;
            };
            let text: String = text.chars().filter(|c| !c.is_control()).take(200).collect();
            let text = text.trim();
            if !text.is_empty() {
                info!("[chat] {} : {text}", player.name);
                broadcast(
                    server,
                    &ServerMessage::Chat {
                        from: player.name.clone(),
                        text: text.to_string(),
                    },
                );
            }
        }
        ClientMessage::TalkToSurvivor { id, dismiss, give } => {
            talk.0.push(crate::survivors::TalkRequest {
                client,
                id,
                dismiss,
                give,
            });
        }
        ClientMessage::Eat { item } => {
            let Some(player) = lobby.players.get_mut(&client) else {
                return;
            };
            if let Some((hunger, health)) = item.food()
                && player.is_alive()
                && player.inventory.remove(item, 1)
            {
                player.hunger = (player.hunger + hunger).min(MAX_HUNGER);
                player.health = (player.health + health).min(MAX_HEALTH);
                send_vitals(server, client, player);
            } else if let Some(thirst) = item.drink()
                && player.is_alive()
                && player.inventory.remove(item, 1)
            {
                // La gourde vidée reste dans l'inventaire.
                player.inventory.add(Item::Flask, 1);
                player.thirst = (player.thirst + thirst).min(MAX_THIRST);
                send_vitals(server, client, player);
            } else if item == Item::Antidote && player.is_alive() && player.inventory.remove(item, 1) {
                player.infection = 0.0;
                send_vitals(server, client, player);
            }
            send_inventory(server, client, player);
        }
        ClientMessage::Drink { pos } => {
            let Some(player) = lobby.players.get_mut(&client) else {
                return;
            };
            if !player.is_alive() || !world.block(pos).is_water() || !within_reach(player, pos) {
                return;
            }
            if player.held_item() == Some(Item::Flask) {
                player.inventory.remove(Item::Flask, 1);
                player.inventory.add(Item::WaterFlask, 1);
                if !player.inventory.has(Item::Flask, 1) {
                    player.held = Some(Item::WaterFlask);
                }
                send_inventory(server, client, player);
            } else if player.held_item() == Some(Item::Bucket) {
                player.inventory.remove(Item::Bucket, 1);
                player.inventory.add(Item::WaterBucket, 1);
                if !player.inventory.has(Item::Bucket, 1) {
                    player.held = Some(Item::WaterBucket);
                }
                // Une source versée au seau se reprend ; un lac reste un lac.
                if world.edits().get(&pos) == Some(&Block::Water) && world.set_block(pos, Block::Air) {
                    version.0 += 1;
                    broadcast(server, &ServerMessage::BlockChanged { pos, block: Block::Air });
                }
                send_inventory(server, client, player);
            } else {
                player.thirst = MAX_THIRST;
                send_vitals(server, client, player);
            }
        }
        ClientMessage::PourWater { pos } => {
            let Some(player) = lobby.players.get_mut(&client) else {
                return;
            };
            let allowed = player.is_alive()
                && player.held_item() == Some(Item::WaterBucket)
                && within_reach(player, pos)
                && world.in_bounds(pos)
                && pos.y > 0
                && matches!(world.block(pos), Block::Air | Block::FlowingWater);
            if allowed && world.set_block(pos, Block::Water) {
                player.inventory.remove(Item::WaterBucket, 1);
                player.inventory.add(Item::Bucket, 1);
                if !player.inventory.has(Item::WaterBucket, 1) {
                    player.held = Some(Item::Bucket);
                }
                version.0 += 1;
                broadcast(
                    server,
                    &ServerMessage::BlockChanged {
                        pos,
                        block: Block::Water,
                    },
                );
                send_inventory(server, client, player);
            }
        }
        ClientMessage::BreakBlock { pos } => {
            let Some(player) = lobby.players.get_mut(&client) else {
                return;
            };
            if !player.is_alive() {
                return;
            }
            let block = world.block(pos);
            let mined_long_enough = match (player.mining, break_time(block, player.held_item())) {
                (Some((p, start)), Some(needed)) => p == pos && now - start >= needed as f64 * MINING_TOLERANCE - 0.1,
                _ => false,
            };
            let allowed = pos.y > 0 && within_reach(player, pos) && block.is_placeable() && mined_long_enough;
            if allowed && block == Block::Chest {
                // Un coffre de ruine cassé sans avoir été ouvert rend quand même son butin.
                chests.fill_ruin_chest(world, pos);
            }
            if allowed && block.is_deposit() {
                // Un filon donne une ressource par récolte et ne disparaît qu'à la dernière.
                let remaining = deposits.0.entry(pos).or_insert(DEPOSIT_HARVESTS);
                *remaining = remaining.saturating_sub(1);
                let left = *remaining;
                if let Some(item) = drop_for(block) {
                    player.inventory.add(item, 1);
                }
                send_inventory(server, client, player);
                if left == 0 {
                    deposits.0.remove(&pos);
                    world.set_block(pos, Block::Air);
                    player.mining = None;
                    version.0 += 1;
                    broadcast(server, &ServerMessage::BlockChanged { pos, block: Block::Air });
                } else {
                    // La récolte suivante commence tout de suite.
                    player.mining = Some((pos, now));
                    send(server, client, &ServerMessage::Deposit { pos, remaining: left });
                }
            } else if allowed && world.set_block(pos, Block::Air) {
                player.mining = None;
                version.0 += 1;
                broadcast(server, &ServerMessage::BlockChanged { pos, block: Block::Air });
                if let Some(item) = drop_for(block) {
                    player.inventory.add(item, 1);
                }
                if block == Block::Leaves && fastrand::f32() < APPLE_CHANCE {
                    player.inventory.add(Item::Apple, 1);
                }
                if block == Block::Grass && fastrand::f32() < SEED_CHANCE {
                    player.inventory.add(Item::Seeds, 1);
                }
                if block == Block::TallGrass && fastrand::f32() < GRASS_SEED_CHANCE {
                    player.inventory.add(Item::Seeds, 1);
                }
                // Une plante privée de son sol disparaît avec lui.
                let above = pos + IVec3::Y;
                if world.block(above).is_plant() && world.set_block(above, Block::Air) {
                    broadcast(
                        server,
                        &ServerMessage::BlockChanged {
                            pos: above,
                            block: Block::Air,
                        },
                    );
                }
                if block == Block::RipeWheat {
                    // Une récolte : deux blés et de quoi ressemer.
                    player.inventory.add(Item::Wheat, 1);
                    player.inventory.add(Item::Seeds, 1 + fastrand::u32(0..2));
                }
                send_inventory(server, client, player);
                if block.is_storage() {
                    chest_destroyed(server, lobby, chests, pos, Some(client));
                }
            } else {
                // Corrige le client qui avait anticipé.
                send(
                    server,
                    client,
                    &ServerMessage::BlockChanged {
                        pos,
                        block: world.block(pos),
                    },
                );
            }
        }
        ClientMessage::PlaceBlock { pos, block } => {
            let blocked_by_player = lobby
                .players
                .values()
                .any(|p| overlaps_solid(p.state.position, PLAYER_SIZE, &|b| b == pos));
            let Some(player) = lobby.players.get_mut(&client) else {
                return;
            };
            if !player.is_alive() {
                return;
            }
            // Une pousse se sème avec des graines, sur de la terre ou de l'herbe.
            let cost = if block == Block::Sprout {
                Item::Seeds
            } else {
                Item::Block(block)
            };
            let soil = !block.is_crop()
                || (block == Block::Sprout && matches!(world.block(pos - IVec3::Y), Block::Grass | Block::Dirt));
            // Les barbelés se posent même là où un joueur se tient.
            let allowed = block.is_placeable()
                && soil
                && player.inventory.has(cost, 1)
                && within_reach(player, pos)
                && world.block(pos).is_replaceable()
                && (!blocked_by_player || !block.is_solid())
                && world.in_bounds(pos);
            // Un bloc trop loin de tout appui s'effondrerait aussitôt : on refuse de le poser.
            let holds = would_hold(pos, block, |p| world.block(p), |p| world.edits().contains_key(&p));
            if allowed && !holds {
                notify(
                    server,
                    client,
                    "Ce bloc ne tiendrait pas : rapprochez-le d'un pilier ou du sol.".into(),
                );
            }
            // Une bannière par joueur, et deux bases ne se chevauchent pas.
            let plant = if block == Block::Banner && allowed && holds {
                claims.can_plant(pos, &player.name)
            } else {
                Ok(())
            };
            match &plant {
                Err(ClaimRefusal::AlreadyOwns(at)) => notify(
                    server,
                    client,
                    format!("Vous avez déjà une bannière en {} {} {}.", at.x, at.y, at.z),
                ),
                Err(ClaimRefusal::TooClose(owner)) => {
                    notify(server, client, format!("Trop près de la base de {owner}."))
                }
                Ok(()) => {}
            }
            if allowed && holds && plant.is_ok() && world.set_block(pos, block) {
                player.inventory.remove(cost, 1);
                version.0 += 1;
                broadcast(server, &ServerMessage::BlockChanged { pos, block });
                if block == Block::Banner {
                    claims.0.insert(pos, player.name.clone());
                    broadcast(server, &claims.message());
                }
            } else {
                send(
                    server,
                    client,
                    &ServerMessage::BlockChanged {
                        pos,
                        block: world.block(pos),
                    },
                );
            }
            send_inventory(server, client, player);
        }
    }
}

/// Tir instantané le long d'un rayon : blesse jusqu'à `pierce` zombies avant le premier mur.
/// Renvoie le point d'arrivée, pour l'effet visuel.
#[allow(clippy::too_many_arguments)]
fn hitscan(
    eye: Vec3,
    dir: Vec3,
    range: f32,
    damage: f32,
    pierce: u8,
    client: ClientId,
    world: &GameWorld,
    zombies: &mut Query<(&mut Zombie, &ZombieBody)>,
    animals: &mut Query<&mut Animal>,
) -> Vec3 {
    let wall = raycast(eye, dir, range, |p| world.is_solid(p))
        .map(|hit| ray_aabb(eye, dir, hit.block.as_vec3(), hit.block.as_vec3() + Vec3::ONE).unwrap_or(0.0))
        .unwrap_or(range);
    // Les zombies sur la trajectoire, du plus proche au plus loin ; un carreau en traverse plusieurs.
    let mut hits: Vec<(Mut<Zombie>, f32)> = zombies
        .iter_mut()
        .filter_map(|(z, body)| {
            let size = z.kind.stats().size;
            let half = Vec3::new(size.half_width + 0.1, 0.0, size.half_width + 0.1);
            let top = Vec3::Y * size.height;
            ray_aabb(eye, dir, body.position - half, body.position + half + top)
                .filter(|t| *t < wall)
                .map(|t| (z, t))
        })
        .collect();
    // Le gibier aussi se trouve sur la trajectoire.
    let mut game: Vec<(Mut<Animal>, f32)> = animals
        .iter_mut()
        .filter_map(|a| {
            let (min, max) = a.bounds();
            ray_aabb(eye, dir, min, max).filter(|t| *t < wall).map(|t| (a, t))
        })
        .collect();
    let mut order: Vec<(bool, usize, f32)> = hits
        .iter()
        .enumerate()
        .map(|(i, (_, t))| (true, i, *t))
        .chain(game.iter().enumerate().map(|(i, (_, t))| (false, i, *t)))
        .collect();
    order.sort_by(|a, b| a.2.total_cmp(&b.2));
    order.truncate(pierce as usize);
    let pierced_all = order.len() == pierce as usize;
    let push = Vec3::new(dir.x, 0.0, dir.z).normalize_or_zero() * 3.0;
    let mut end = eye + dir * wall;
    for (zombie, i, t) in order {
        if zombie {
            let z = &mut hits[i].0;
            z.health -= damage;
            z.hurt_timer = 0.3;
            z.knockback = push;
            z.last_attacker = Some(client);
        } else {
            game[i].0.hurt(damage, Some(client), push);
        }
        if pierced_all {
            end = eye + dir * t;
        }
    }
    end
}

/// Message du serveur à un seul joueur.
fn notify(server: &mut RenetServer, client: ClientId, text: String) {
    send(
        server,
        client,
        &ServerMessage::Chat {
            from: "Serveur".into(),
            text,
        },
    );
}

/// Premier joueur (autre que le tireur) sur la trajectoire, avant le premier mur.
fn aimed_player(
    lobby: &Lobby,
    shooter: ClientId,
    eye: Vec3,
    dir: Vec3,
    range: f32,
    world: &GameWorld,
) -> Option<(ClientId, f32)> {
    let wall = raycast(eye, dir, range, |p| world.is_solid(p))
        .map(|hit| ray_aabb(eye, dir, hit.block.as_vec3(), hit.block.as_vec3() + Vec3::ONE).unwrap_or(0.0))
        .unwrap_or(range);
    lobby
        .players
        .iter()
        .filter(|(id, p)| **id != shooter && p.is_alive())
        .filter_map(|(id, p)| {
            let half = Vec3::new(PLAYER_SIZE.half_width, 0.0, PLAYER_SIZE.half_width);
            let min = p.state.position - half;
            let max = p.state.position + half + Vec3::Y * PLAYER_SIZE.height;
            ray_aabb(eye, dir, min, max).filter(|t| *t < wall).map(|t| (*id, t))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
}

/// Un joueur en frappe un autre. Personne n'est blessé chez soi (dans sa propre base).
fn pvp_hit(
    server: &mut RenetServer,
    lobby: &mut Lobby,
    claims: &Claims,
    attacker: ClientId,
    victim: ClientId,
    damage: f32,
) {
    let Some(name) = lobby.players.get(&attacker).map(|p| p.name.clone()) else {
        return;
    };
    let Some(target) = lobby.players.get_mut(&victim) else {
        return;
    };
    if !target.is_alive() {
        return;
    }
    if claims.at_home(target.state.position.floor().as_ivec3(), &target.name) {
        notify(server, attacker, format!("{} est chez lui : protégé.", target.name));
        return;
    }
    let amount = damage * (1.0 - target.inventory.armor_reduction());
    crate::zombies::hurt_player(victim, target, amount, &format!("Tué par {name}"), Some(server));
    if !target.is_alive() {
        let text = format!("{} a été tué par {name}.", target.name);
        broadcast(
            server,
            &ServerMessage::Chat {
                from: "Serveur".into(),
                text,
            },
        );
    }
}

/// Envoie le contenu d'un coffre à tous les joueurs qui l'ont ouvert.
pub(crate) fn send_chest_to_viewers(server: &mut RenetServer, lobby: &Lobby, chests: &Chests, pos: IVec3) {
    let contents = chests.0.get(&pos).cloned().unwrap_or_default();
    for (id, player) in &lobby.players {
        if player.open_chest == Some(pos) {
            send(
                server,
                *id,
                &ServerMessage::Chest {
                    pos,
                    contents: contents.clone(),
                },
            );
        }
    }
}

/// Un coffre a disparu : ses objets vont à celui qui l'a cassé (s'il y en a un), les autres le ferment.
pub fn chest_destroyed(
    server: &mut RenetServer,
    lobby: &mut Lobby,
    chests: &mut Chests,
    pos: IVec3,
    breaker: Option<ClientId>,
) {
    let contents = chests.0.remove(&pos).unwrap_or_default();
    for (id, player) in lobby.players.iter_mut() {
        if player.open_chest == Some(pos) {
            player.open_chest = None;
            send(server, *id, &ServerMessage::ChestClosed);
        }
        if Some(*id) == breaker && !contents.is_empty() {
            for (item, n) in contents.iter() {
                player.inventory.add(item, n);
            }
            send_inventory(server, *id, player);
        }
    }
}

fn handle_state(client: ClientId, state: PlayerState, server: &mut RenetServer, lobby: &mut Lobby, world: &GameWorld) {
    let Some(player) = lobby.players.get_mut(&client) else {
        return;
    };
    if !player.is_alive() || !state.position.is_finite() {
        return;
    }
    let moved = state.position.distance(player.state.position);
    let inside_wall = overlaps_solid(state.position, PLAYER_SIZE, &|b| world.is_solid(b));
    if moved > MAX_MOVE_PER_UPDATE || inside_wall {
        send(
            server,
            client,
            &ServerMessage::Teleport {
                position: player.state.position,
            },
        );
        return;
    }
    player.state = state;
}

fn broadcast_players(mut server: ResMut<RenetServer>, lobby: Res<Lobby>) {
    let players: Vec<_> = lobby
        .players
        .iter()
        .filter(|(_, p)| p.is_ready())
        .map(|(id, p)| (*id, p.state, p.held_item()))
        .collect();
    if !players.is_empty() {
        server.broadcast_message(DefaultChannel::Unreliable, encode(&ServerMessage::Players(players)));
    }
}

fn within_reach(player: &ConnectedPlayer, pos: IVec3) -> bool {
    let eye = player.state.position + Vec3::Y * PLAYER_EYE_HEIGHT;
    // Une marge d'un bloc couvre la latence et la taille du bloc.
    eye.distance(pos.as_vec3() + Vec3::splat(0.5)) <= REACH + 1.5
}

/// Garde un nom lisible : 16 caractères, sans caractères de contrôle.
fn sanitize_name(name: &str, client: ClientId) -> String {
    let clean: String = name.chars().filter(|c| !c.is_control()).take(16).collect();
    let clean = clean.trim().to_string();
    if clean.is_empty() {
        format!("Survivant {}", client % 1000)
    } else {
        clean
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::item::Tier;

    #[test]
    fn guard_parry_and_armor_reduce_damage() {
        // Le joueur regarde vers -Z ; le zombie arrive de face.
        let mut p = ConnectedPlayer::new(Vec3::ZERO);
        let front = Vec3::new(0.0, 0.0, -1.0);
        let back = Vec3::new(0.0, 0.0, 1.0);
        assert_eq!(p.incoming_damage(20.0, front), (20.0, false));
        // Sans arme de mêlée, lever la garde ne sert à rien.
        p.guarding = Some(0.0);
        assert_eq!(p.incoming_damage(20.0, front), (20.0, false));
        p.inventory.add(Item::Sword(Tier::Wood), 1);
        p.held = Some(Item::Sword(Tier::Wood));
        assert_eq!(p.incoming_damage(20.0, front), (0.0, true));
        p.guarding = Some(1.0);
        assert_eq!(p.incoming_damage(20.0, front), (20.0 * GUARD_FACTOR, false));
        assert_eq!(p.incoming_damage(20.0, back), (20.0, false));
        p.guarding = None;
        p.inventory.add(Item::BodyArmor(Tier::Iron), 1);
        let (dealt, _) = p.incoming_damage(20.0, back);
        assert!((dealt - 14.0).abs() < 1e-4);
    }

    #[test]
    fn names_are_cleaned() {
        assert_eq!(sanitize_name("  Bob\n ", 1), "Bob");
        assert_eq!(sanitize_name("", 1234), "Survivant 234");
        assert_eq!(sanitize_name(&"x".repeat(40), 1).len(), 16);
    }
}
