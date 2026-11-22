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
use shared::crafting::RECIPES;
use shared::item::{Inventory, Item, break_time, drop_for};
use shared::physics::{PLAYER_EYE_HEIGHT, PLAYER_SIZE, overlaps_solid};
use shared::protocol::{ClientMessage, PROTOCOL_ID, PlayerState, REACH, ServerMessage, decode, encode};
use shared::raycast::{ray_aabb, raycast};

use crate::ServerConfig;
use crate::save::{SavedPlayer, SavedPlayers};
use crate::world::{Deposits, GameWorld, SpawnPoint};
use crate::zombies::{Zombie, ZombieBody};

/// Portée d'un coup de mêlée, en mètres.
pub const MELEE_REACH: f32 = 3.0;
const PLAYER_ATTACK_COOLDOWN: f32 = 0.35;

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
    pub inventory: Inventory,
    /// Objet en main, toujours présent dans l'inventaire.
    pub held: Option<Item>,
    /// Bloc en cours de minage et instant du début (secondes depuis le lancement du serveur).
    pub mining: Option<(IVec3, f64)>,
}

/// Santé maximale d'un joueur.
pub const MAX_HEALTH: f32 = 100.0;
/// Faim maximale d'un joueur.
pub const MAX_HUNGER: f32 = 100.0;
/// Chance qu'un bloc de feuilles donne aussi une pomme.
const APPLE_CHANCE: f32 = 0.25;
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
            },
            health: MAX_HEALTH,
            since_hit: 0.0,
            attack_cooldown: 0.0,
            hunger: MAX_HUNGER,
            inventory,
            held: None,
            mining: None,
        }
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
        },
    );
}

fn send_inventory(server: &mut RenetServer, client: ClientId, player: &ConnectedPlayer) {
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

fn send(server: &mut RenetServer, client: ClientId, message: &ServerMessage) {
    server.send_message(client, DefaultChannel::ReliableOrdered, encode(message));
}

fn broadcast(server: &mut RenetServer, message: &ServerMessage) {
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
            };
            handle_reliable(client, message, &mut ctx, &mut zombies);
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
}

fn handle_reliable(
    client: ClientId,
    message: ClientMessage,
    ctx: &mut Context,
    zombies: &mut Query<(&mut Zombie, &ZombieBody)>,
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
    } = ctx;
    let (spawn, now) = (*spawn, *now);
    let ready = lobby.players.get(&client).is_some_and(ConnectedPlayer::is_ready);
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
                    world: world.config().clone(),
                    spawn: start,
                },
            );
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
            if !player.is_alive() || player.attack_cooldown > 0.0 {
                return;
            }
            let eye = player.state.position + Vec3::Y * PLAYER_EYE_HEIGHT;
            let look = Quat::from_euler(EulerRot::YXZ, player.state.yaw, 0.0, 0.0) * Vec3::NEG_Z;
            if let Some((mut z, body)) = zombies.iter_mut().find(|(z, _)| z.id == zombie) {
                let center = body.position + Vec3::Y * z.kind.stats().size.height * 0.5;
                if center.distance(eye) <= MELEE_REACH + 1.0 {
                    player.attack_cooldown = PLAYER_ATTACK_COOLDOWN;
                    z.health -= Item::melee_damage(player.held_item());
                    z.hurt_timer = 0.3;
                    z.knockback = Vec3::new(look.x, 0.0, look.z).normalize_or_zero() * 6.0 + Vec3::Y * 3.0;
                    z.last_attacker = Some(client);
                }
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
            let wall = raycast(eye, dir, weapon.range, |p| world.is_solid(p))
                .map(|hit| ray_aabb(eye, dir, hit.block.as_vec3(), hit.block.as_vec3() + Vec3::ONE).unwrap_or(0.0))
                .unwrap_or(weapon.range);
            let target = zombies
                .iter_mut()
                .filter_map(|(z, body)| {
                    let size = z.kind.stats().size;
                    let half = Vec3::new(size.half_width + 0.1, 0.0, size.half_width + 0.1);
                    let top = Vec3::Y * size.height;
                    ray_aabb(eye, dir, body.position - half, body.position + half + top)
                        .filter(|t| *t < wall)
                        .map(|t| (z, t))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1));
            let end = match target {
                Some((mut z, t)) => {
                    z.health -= weapon.damage;
                    z.hurt_timer = 0.3;
                    z.knockback = Vec3::new(dir.x, 0.0, dir.z).normalize_or_zero() * 3.0;
                    z.last_attacker = Some(client);
                    eye + dir * t
                }
                None => eye + dir * wall,
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
            if let Some(recipe) = RECIPES.get(recipe as usize)
                && player.is_alive()
            {
                recipe.craft(&mut player.inventory);
            }
            send_inventory(server, client, player);
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
            }
            send_inventory(server, client, player);
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
            let allowed = pos.y > 0 && within_reach(player, pos) && block.is_solid() && mined_long_enough;
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
                send_inventory(server, client, player);
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
            let allowed = block.is_solid()
                && player.inventory.has(Item::Block(block), 1)
                && within_reach(player, pos)
                && !world.is_solid(pos)
                && !blocked_by_player
                && world.in_bounds(pos);
            if allowed && world.set_block(pos, block) {
                player.inventory.remove(Item::Block(block), 1);
                version.0 += 1;
                broadcast(server, &ServerMessage::BlockChanged { pos, block });
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
        .map(|(id, p)| (*id, p.state))
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

    #[test]
    fn names_are_cleaned() {
        assert_eq!(sanitize_name("  Bob\n ", 1), "Bob");
        assert_eq!(sanitize_name("", 1234), "Survivant 234");
        assert_eq!(sanitize_name(&"x".repeat(40), 1).len(), 16);
    }
}
