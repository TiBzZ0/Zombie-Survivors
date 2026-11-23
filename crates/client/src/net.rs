//! Réseau côté client : connexion, réception de l'état du monde, envoi de la position.

use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::{Duration, SystemTime};

use bevy::prelude::*;
use bevy_renet::netcode::{ClientAuthentication, NetcodeClientPlugin, NetcodeClientTransport};
use bevy_renet::renet::{ConnectionConfig, DefaultChannel};
use bevy_renet::{RenetClient, RenetClientPlugin, client_connected};
use server::{LocalServer, ServerConfig};
use shared::daytime::GameTime;
use shared::protocol::{ClientMessage, PROTOCOL_ID, PlayerId, PlayerState, ServerMessage, ZombieState, decode, encode};
use shared::world::WorldState;

use crate::{AppState, LaunchOptions, MenuNotice};

/// Délai maximal pour recevoir la réponse du serveur.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Fréquence d'envoi de la position du joueur.
const STATE_SEND_HZ: f64 = 20.0;

pub struct NetPlugin;

impl Plugin for NetPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((RenetClientPlugin, NetcodeClientPlugin))
            .add_message::<NetEvent>()
            .add_systems(OnEnter(AppState::Connecting), start_connection)
            .add_systems(
                Update,
                (
                    connection_timeout.run_if(in_state(AppState::Connecting)),
                    receive_messages.run_if(client_connected),
                    detect_disconnect.run_if(not(in_state(AppState::Menu))),
                ),
            )
            .add_systems(OnEnter(AppState::Menu), close_connection);
    }
}

/// Le monde tel que le client le connaît.
#[derive(Resource, Deref, DerefMut)]
pub struct ClientWorld(pub WorldState);

/// Infos de session reçues du serveur.
#[derive(Resource, Debug, Clone)]
pub struct Session {
    pub my_id: PlayerId,
    pub my_name: String,
    pub spawn: Vec3,
    /// Combat entre joueurs activé sur ce serveur.
    pub pvp: bool,
}

/// Bases protégées par une bannière : position et propriétaire.
#[derive(Resource, Default, Debug)]
pub struct Claims(pub Vec<(IVec3, String)>);

impl Claims {
    /// Propriétaire de la base qui contient ce point, s'il y en a une.
    pub fn owner_at(&self, pos: Vec3) -> Option<&str> {
        let p = pos.floor().as_ivec3();
        self.0
            .iter()
            .find(|(banner, _)| shared::block::in_claim(*banner, p))
            .map(|(_, owner)| owner.as_str())
    }
}

/// Serveur local, gardé en vie pendant la partie.
#[derive(Resource)]
struct HostedServer(#[allow(dead_code)] LocalServer);

#[derive(Resource)]
struct ConnectStarted(f32);

/// Événements réseau transmis au reste du client.
#[derive(Message, Debug, Clone)]
pub enum NetEvent {
    BlockChanged(IVec3),
    /// Un bloc vient d'être cassé (pour les éclats).
    BlockBroken {
        pos: IVec3,
        block: shared::block::Block,
    },
    PlayerJoined {
        id: PlayerId,
        name: String,
    },
    PlayerLeft(PlayerId),
    Players(Vec<(PlayerId, PlayerState, Option<shared::item::Item>)>),
    Teleport(Vec3),
    Chat(String),
    Zombies(Vec<ZombieState>),
    Animals(Vec<shared::animal::AnimalState>),
    Time {
        time: GameTime,
        zombies_alive: u32,
    },
    Vitals {
        health: f32,
        hunger: f32,
        thirst: f32,
        body_temp: f32,
        felt: f32,
        infection: f32,
    },
    Inventory(shared::item::Inventory),
    Deposit {
        pos: IVec3,
        remaining: u8,
    },
    Shot {
        origin: Vec3,
        end: Vec3,
        rifle: bool,
    },
    Chest {
        pos: IVec3,
        contents: shared::item::Inventory,
    },
    ChestClosed,
    BlockDamage {
        pos: IVec3,
        damage: u8,
    },
    Died(String),
    ZombieKilled(u32),
    Survivors(Vec<shared::protocol::SurvivorState>),
    SurvivorInfo {
        id: u32,
        name: String,
        stats: shared::survivor::SurvivorStats,
        leader: Option<String>,
        order: shared::survivor::Order,
        weapon: Option<shared::item::Item>,
        armor: [Option<shared::item::Item>; 2],
        offers: Vec<shared::survivor::Offer>,
    },
    /// Brouillard ou non.
    Weather {
        fog: bool,
    },
    /// Un coup de zombie a été paré.
    Parried,
    Thrown {
        origin: Vec3,
        velocity: Vec3,
        fuse: f32,
        item: shared::item::Item,
    },
    Explosion {
        pos: Vec3,
        radius: f32,
    },
    /// Une construction s'est effondrée.
    Collapse {
        pos: Vec3,
        blocks: u16,
    },
    Acid {
        origin: Vec3,
        end: Vec3,
    },
    /// Générateurs en marche.
    Power(Vec<IVec3>),
    Zap {
        origin: Vec3,
        end: Vec3,
    },
}

fn fail(commands: &mut Commands, next: &mut NextState<AppState>, message: String) {
    error!("{message}");
    commands.insert_resource(MenuNotice(Some(message)));
    next.set(AppState::Menu);
}

fn resolve(address: &str) -> Option<SocketAddr> {
    let address = if address.contains(':') {
        address.to_string()
    } else {
        format!("{address}:{}", shared::DEFAULT_PORT)
    };
    address.to_socket_addrs().ok()?.next()
}

fn start_connection(
    mut commands: Commands,
    options: Res<LaunchOptions>,
    time: Res<Time>,
    mut next: ResMut<NextState<AppState>>,
) {
    let server_addr = if options.host_locally {
        let config = ServerConfig {
            pvp: options.pvp,
            ..ServerConfig::local()
        };
        let port = config.port;
        info!("Sauvegarde de la partie locale : {}", config.data_dir.display());
        match server::spawn_local(config) {
            Ok(local) => commands.insert_resource(HostedServer(local)),
            Err(error) => {
                return fail(&mut commands, &mut next, format!("Serveur local impossible : {error}"));
            }
        }
        info!("Partie locale : les joueurs du réseau local peuvent rejoindre sur le port {port}");
        SocketAddr::from(([127, 0, 0, 1], port))
    } else {
        match resolve(&options.address) {
            Some(addr) => addr,
            None => {
                return fail(
                    &mut commands,
                    &mut next,
                    format!("Adresse introuvable : {}", options.address),
                );
            }
        }
    };

    let bind: SocketAddr = if server_addr.is_ipv6() {
        "[::]:0".parse().unwrap()
    } else {
        "0.0.0.0:0".parse().unwrap()
    };
    let socket = match UdpSocket::bind(bind) {
        Ok(socket) => socket,
        Err(error) => {
            return fail(&mut commands, &mut next, format!("Réseau indisponible : {error}"));
        }
    };
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let authentication = ClientAuthentication::Unsecure {
        client_id: now.as_nanos() as u64 ^ u64::from(std::process::id()),
        protocol_id: PROTOCOL_ID,
        server_addr,
        user_data: None,
    };
    match NetcodeClientTransport::new(now, authentication, socket) {
        Ok(transport) => {
            let mut client = RenetClient::new(ConnectionConfig::default());
            client.send_message(
                DefaultChannel::ReliableOrdered,
                encode(&ClientMessage::Hello {
                    name: options.name.clone(),
                }),
            );
            commands.insert_resource(client);
            commands.insert_resource(transport);
            commands.insert_resource(ConnectStarted(time.elapsed_secs()));
            info!("Connexion à {server_addr}…");
        }
        Err(error) => fail(&mut commands, &mut next, format!("Connexion impossible : {error}")),
    }
}

fn connection_timeout(
    mut commands: Commands,
    started: Option<Res<ConnectStarted>>,
    time: Res<Time>,
    mut next: ResMut<NextState<AppState>>,
) {
    if let Some(started) = started
        && time.elapsed_secs() - started.0 > CONNECT_TIMEOUT.as_secs_f32()
    {
        fail(&mut commands, &mut next, "Le serveur ne répond pas.".into());
    }
}

fn detect_disconnect(
    mut commands: Commands,
    client: Option<Res<RenetClient>>,
    state: Res<State<AppState>>,
    mut next: ResMut<NextState<AppState>>,
) {
    let Some(client) = client else { return };
    if client.is_disconnected() {
        let reason = client
            .disconnect_reason()
            .map(|r| r.to_string())
            .unwrap_or_else(|| "raison inconnue".into());
        let message = if *state.get() == AppState::Playing {
            format!("Connexion perdue : {reason}")
        } else {
            format!("Connexion refusée : {reason}")
        };
        fail(&mut commands, &mut next, message);
    }
}

fn receive_messages(
    mut commands: Commands,
    mut client: ResMut<RenetClient>,
    mut world: Option<ResMut<ClientWorld>>,
    mut events: MessageWriter<NetEvent>,
    state: Res<State<AppState>>,
    mut next: ResMut<NextState<AppState>>,
) {
    // Le monde peut arriver dans ce même appel : on le garde localement jusqu'à la fin.
    let mut fresh_world: Option<WorldState> = None;
    while let Some(bytes) = client.receive_message(DefaultChannel::ReliableOrdered) {
        let Some(message) = decode::<ServerMessage>(&bytes) else {
            warn!("Message du serveur illisible (versions différentes ?)");
            continue;
        };
        let world: Option<&mut WorldState> = match fresh_world.as_mut() {
            Some(w) => Some(w),
            None => world.as_deref_mut().map(|w| &mut w.0),
        };
        match message {
            ServerMessage::Welcome {
                your_id,
                your_name,
                world,
                spawn,
                pvp,
            } => {
                info!("Connecté en tant que joueur {your_id} ({your_name})");
                fresh_world = Some(WorldState::new(world));
                commands.insert_resource(Session {
                    my_id: your_id,
                    my_name: your_name,
                    spawn,
                    pvp,
                });
                commands.insert_resource(Claims::default());
                commands.remove_resource::<ConnectStarted>();
                if *state.get() == AppState::Connecting {
                    next.set(AppState::Playing);
                }
            }
            ServerMessage::WorldEdits(edits) => {
                if let Some(world) = world {
                    for (pos, block) in edits {
                        world.set_block(pos, block);
                        events.write(NetEvent::BlockChanged(pos));
                    }
                }
            }
            ServerMessage::BlockChanged { pos, block } => {
                if let Some(world) = world
                    && world.block(pos) != block
                {
                    let old = world.block(pos);
                    world.set_block(pos, block);
                    events.write(NetEvent::BlockChanged(pos));
                    if block == shared::block::Block::Air {
                        events.write(NetEvent::BlockBroken { pos, block: old });
                    }
                }
            }
            ServerMessage::PlayerJoined { id, name } => {
                events.write(NetEvent::Chat(format!("{name} a rejoint la partie")));
                events.write(NetEvent::PlayerJoined { id, name });
            }
            ServerMessage::PlayerLeft { id } => {
                events.write(NetEvent::PlayerLeft(id));
            }
            ServerMessage::Teleport { position } => {
                events.write(NetEvent::Teleport(position));
            }
            ServerMessage::Chat { from, text } => {
                events.write(NetEvent::Chat(format!("{from} : {text}")));
            }
            ServerMessage::Time { time, zombies_alive } => {
                events.write(NetEvent::Time { time, zombies_alive });
            }
            ServerMessage::Chest { pos, contents } => {
                events.write(NetEvent::Chest { pos, contents });
            }
            ServerMessage::BlockDamage { pos, damage } => {
                events.write(NetEvent::BlockDamage { pos, damage });
            }
            ServerMessage::ChestClosed => {
                events.write(NetEvent::ChestClosed);
            }
            ServerMessage::Shot { origin, end, rifle, .. } => {
                events.write(NetEvent::Shot { origin, end, rifle });
            }
            ServerMessage::Deposit { pos, remaining } => {
                events.write(NetEvent::Deposit { pos, remaining });
            }
            ServerMessage::Inventory(inventory) => {
                events.write(NetEvent::Inventory(inventory));
            }
            ServerMessage::Vitals {
                health,
                hunger,
                thirst,
                body_temp,
                felt,
                infection,
            } => {
                events.write(NetEvent::Vitals {
                    health,
                    hunger,
                    thirst,
                    body_temp,
                    felt,
                    infection,
                });
            }
            ServerMessage::Died { cause } => {
                events.write(NetEvent::Died(cause));
            }
            ServerMessage::ZombieKilled { id, .. } => {
                events.write(NetEvent::ZombieKilled(id));
            }
            ServerMessage::SurvivorInfo {
                id,
                name,
                stats,
                leader,
                order,
                weapon,
                armor,
                offers,
            } => {
                events.write(NetEvent::SurvivorInfo {
                    id,
                    name,
                    stats,
                    leader,
                    order,
                    weapon,
                    armor,
                    offers,
                });
            }
            ServerMessage::Weather { fog } => {
                events.write(NetEvent::Weather { fog });
            }
            ServerMessage::Parried => {
                events.write(NetEvent::Parried);
            }
            ServerMessage::Thrown {
                origin,
                velocity,
                fuse,
                item,
            } => {
                events.write(NetEvent::Thrown {
                    origin,
                    velocity,
                    fuse,
                    item,
                });
            }
            ServerMessage::Explosion { pos, radius } => {
                events.write(NetEvent::Explosion { pos, radius });
            }
            ServerMessage::Claims(list) => {
                commands.insert_resource(Claims(list));
            }
            ServerMessage::Collapse { pos, blocks } => {
                events.write(NetEvent::Collapse { pos, blocks });
            }
            ServerMessage::Power(running) => {
                events.write(NetEvent::Power(running));
            }
            ServerMessage::Zap { origin, end } => {
                events.write(NetEvent::Zap { origin, end });
            }
            ServerMessage::Acid { origin, end } => {
                events.write(NetEvent::Acid { origin, end });
            }
            ServerMessage::Players(_)
            | ServerMessage::Zombies(_)
            | ServerMessage::Survivors(_)
            | ServerMessage::Animals(_) => {}
        }
    }
    if let Some(world) = fresh_world {
        commands.insert_resource(ClientWorld(world));
    }
    while let Some(bytes) = client.receive_message(DefaultChannel::Unreliable) {
        match decode::<ServerMessage>(&bytes) {
            Some(ServerMessage::Players(players)) => {
                events.write(NetEvent::Players(players));
            }
            Some(ServerMessage::Zombies(zombies)) => {
                events.write(NetEvent::Zombies(zombies));
            }
            Some(ServerMessage::Survivors(survivors)) => {
                events.write(NetEvent::Survivors(survivors));
            }
            Some(ServerMessage::Animals(animals)) => {
                events.write(NetEvent::Animals(animals));
            }
            _ => {}
        }
    }
}

/// Envoie un message fiable au serveur (casser, poser…).
pub fn send_reliable(client: &mut RenetClient, message: &ClientMessage) {
    client.send_message(DefaultChannel::ReliableOrdered, encode(message));
}

/// Envoie la position du joueur, au plus `STATE_SEND_HZ` fois par seconde.
pub fn send_state(client: &mut RenetClient, state: PlayerState, time: &Time, last_sent: &mut f64) {
    let now = time.elapsed_secs_f64();
    if now - *last_sent >= 1.0 / STATE_SEND_HZ {
        *last_sent = now;
        client.send_message(DefaultChannel::Unreliable, encode(&ClientMessage::State(state)));
    }
}

fn close_connection(mut commands: Commands, transport: Option<ResMut<NetcodeClientTransport>>) {
    if let Some(mut transport) = transport {
        transport.disconnect();
    }
    commands.remove_resource::<RenetClient>();
    commands.remove_resource::<NetcodeClientTransport>();
    commands.remove_resource::<ClientWorld>();
    commands.remove_resource::<Session>();
    commands.remove_resource::<ConnectStarted>();
    // Arrête le serveur local s'il y en avait un.
    commands.remove_resource::<HostedServer>();
}
