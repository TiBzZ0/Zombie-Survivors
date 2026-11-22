//! Test de bout en bout : un vrai serveur, deux vrais clients réseau.
//! Vérifie la connexion, la diffusion des joueurs et la synchronisation des blocs.

use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant, SystemTime};

use bevy::math::{IVec3, Vec3};
use bevy_renet::netcode::ClientAuthentication;
use bevy_renet::netcode::NetcodeClientTransport;
use bevy_renet::renet::{ConnectionConfig, DefaultChannel, RenetClient};
use server::{ServerConfig, spawn_local};
use shared::block::Block;
use shared::config::WorldConfig;
use shared::item::{Item, break_time, drop_for};
use shared::protocol::{ClientMessage, PROTOCOL_ID, PlayerState, ServerMessage, decode, encode};
use shared::worldgen::WorldGenerator;

struct TestClient {
    client: RenetClient,
    transport: NetcodeClientTransport,
    received: Vec<ServerMessage>,
    last: Instant,
}

impl TestClient {
    fn connect(port: u16, id: u64, name: &str) -> Self {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap();
        let server_addr: SocketAddr = ([127, 0, 0, 1], port).into();
        let auth = ClientAuthentication::Unsecure {
            client_id: id,
            protocol_id: PROTOCOL_ID,
            server_addr,
            user_data: None,
        };
        let transport = NetcodeClientTransport::new(now, auth, socket).unwrap();
        let mut client = RenetClient::new(ConnectionConfig::default());
        client.send_message(
            DefaultChannel::ReliableOrdered,
            encode(&ClientMessage::Hello { name: name.into() }),
        );
        Self {
            client,
            transport,
            received: Vec::new(),
            last: Instant::now(),
        }
    }

    fn pump(&mut self) {
        let dt = self.last.elapsed();
        self.last = Instant::now();
        self.client.update(dt);
        self.transport.0.update(dt, &mut self.client).unwrap();
        for channel in [2u8, 0u8] {
            while let Some(bytes) = self.client.receive_message(channel) {
                self.received.push(decode(&bytes).expect("message serveur illisible"));
            }
        }
        let _ = self.transport.0.send_packets(&mut self.client);
    }

    fn send(&mut self, message: &ClientMessage, reliable: bool) {
        let channel = if reliable {
            DefaultChannel::ReliableOrdered
        } else {
            DefaultChannel::Unreliable
        };
        self.client.send_message(channel, encode(message));
    }

    fn find<T>(&self, f: impl Fn(&ServerMessage) -> Option<T>) -> Option<T> {
        self.received.iter().find_map(f)
    }
}

/// Fait tourner les clients jusqu'à ce que la condition soit vraie (ou échoue après 10 s).
fn pump_until(clients: &mut [&mut TestClient], what: &str, mut done: impl FnMut(&mut [&mut TestClient]) -> bool) {
    let start = Instant::now();
    while !done(clients) {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "délai dépassé en attendant : {what}"
        );
        for c in clients.iter_mut() {
            c.pump();
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn two_players_share_the_world() {
    let port = 17_000 + (std::process::id() % 1000) as u16;
    let config = ServerConfig {
        port,
        ..ServerConfig::default()
    };
    let _server = spawn_local(config).unwrap();
    std::thread::sleep(Duration::from_millis(300));

    let mut alice = TestClient::connect(port, 1, "Alice");
    pump_until(&mut [&mut alice], "bienvenue d'Alice", |c| {
        c[0].find(|m| matches!(m, ServerMessage::Welcome { .. }).then_some(()))
            .is_some()
    });
    let spawn = alice
        .find(|m| match m {
            ServerMessage::Welcome { spawn, your_id, .. } => {
                assert_eq!(*your_id, 1);
                Some(*spawn)
            }
            _ => None,
        })
        .unwrap();

    let mut bob = TestClient::connect(port, 2, "Bob");
    pump_until(&mut [&mut alice, &mut bob], "arrivée de Bob annoncée à Alice", |c| {
        c[0].find(|m| matches!(m, ServerMessage::PlayerJoined { id: 2, .. }).then_some(()))
            .is_some()
    });

    // Bob voit Alice dans la liste des joueurs connus.
    assert!(
        bob.find(|m| matches!(m, ServerMessage::PlayerJoined { id: 1, .. }).then_some(()))
            .is_some()
    );

    // Casser sans avoir miné assez longtemps est refusé.
    let below = (spawn - Vec3::Y).floor().as_ivec3();
    let ground = WorldGenerator::new(WorldConfig::default()).block_at(below.x, below.y, below.z);
    alice.received.clear();
    alice.send(&ClientMessage::BreakBlock { pos: below }, true);
    pump_until(&mut [&mut alice, &mut bob], "cassage trop rapide refusé", |c| {
        c[0].find(|m| match m {
            ServerMessage::BlockChanged { pos, block } if *pos == below => Some(*block),
            _ => None,
        })
        .is_some()
    });
    let corrected = alice.find(|m| match m {
        ServerMessage::BlockChanged { pos, block } if *pos == below => Some(*block),
        _ => None,
    });
    assert_eq!(corrected, Some(ground));

    // Alice mine le bloc sous ses pieds : Bob reçoit le changement, Alice récupère le bloc.
    alice.send(&ClientMessage::StartMining { pos: below }, true);
    let needed = break_time(ground, None).expect("le sol se casse à la main");
    let start = Instant::now();
    while start.elapsed().as_secs_f32() < needed + 0.1 {
        alice.pump();
        bob.pump();
        std::thread::sleep(Duration::from_millis(10));
    }
    alice.received.clear();
    alice.send(&ClientMessage::BreakBlock { pos: below }, true);
    pump_until(&mut [&mut alice, &mut bob], "bloc cassé reçu par Bob", |c| {
        c[1].find(|m| match m {
            ServerMessage::BlockChanged { pos, block: Block::Air } if *pos == below => Some(()),
            _ => None,
        })
        .is_some()
    });
    let dropped = drop_for(ground).unwrap();
    pump_until(&mut [&mut alice, &mut bob], "bloc récupéré par Alice", |c| {
        c[0].find(|m| match m {
            ServerMessage::Inventory(inv) if inv.has(dropped, 1) => Some(()),
            _ => None,
        })
        .is_some()
    });
    let Item::Block(dropped_block) = dropped else {
        panic!("le sol donne un bloc")
    };

    // Bob n'a pas ce bloc : il ne peut pas le poser.
    let placed = below + IVec3::new(1, 4, 0);
    bob.received.clear();
    bob.send(
        &ClientMessage::PlaceBlock {
            pos: placed,
            block: dropped_block,
        },
        true,
    );
    pump_until(&mut [&mut alice, &mut bob], "pose refusée sans le bloc", |c| {
        c[1].find(|m| match m {
            ServerMessage::BlockChanged { pos, block } if *pos == placed => Some(*block),
            _ => None,
        })
        .is_some()
    });
    let refused = bob.find(|m| match m {
        ServerMessage::BlockChanged { pos, block } if *pos == placed => Some(*block),
        _ => None,
    });
    assert_eq!(refused, Some(Block::Air));

    // Alice pose son bloc à portée : Bob le reçoit.
    // Au-dessus de la tête : toujours de l'air au point d'apparition.
    alice.send(
        &ClientMessage::PlaceBlock {
            pos: placed,
            block: dropped_block,
        },
        true,
    );
    pump_until(&mut [&mut alice, &mut bob], "bloc posé reçu par Bob", |c| {
        c[1].find(|m| match m {
            ServerMessage::BlockChanged { pos, block } if *pos == placed && *block == dropped_block => Some(()),
            _ => None,
        })
        .is_some()
    });

    // Bob mange une de ses trois pommes de départ.
    bob.received.clear();
    bob.send(&ClientMessage::Eat { item: Item::Apple }, true);
    pump_until(&mut [&mut alice, &mut bob], "pomme mangée", |c| {
        c[1].find(|m| match m {
            ServerMessage::Inventory(inv) if inv.count(Item::Apple) == 2 => Some(()),
            _ => None,
        })
        .is_some()
    });

    // Un bloc hors de portée est refusé et le client est corrigé.
    let far = below + IVec3::new(40, 0, 0);
    bob.received.clear();
    bob.send(&ClientMessage::BreakBlock { pos: far }, true);
    pump_until(&mut [&mut alice, &mut bob], "correction du bloc refusé", |c| {
        c[1].find(|m| match m {
            ServerMessage::BlockChanged { pos, block } if *pos == far => Some(block.is_solid() || *block == Block::Air),
            _ => None,
        })
        .is_some()
    });

    // La position d'Alice est diffusée à Bob.
    // Même position (forcément libre), mais Alice tourne la tête.
    let moved = spawn;
    bob.received.clear();
    pump_until(&mut [&mut alice, &mut bob], "position d'Alice reçue par Bob", |c| {
        c[0].send(
            &ClientMessage::State(PlayerState {
                position: moved,
                yaw: 1.0,
                pitch: 0.0,
            }),
            false,
        );
        c[1].find(|m| match m {
            ServerMessage::Players(list) => list.iter().find(|(id, s)| *id == 1 && s.position == moved).map(|_| ()),
            _ => None,
        })
        .is_some()
    });

    // Un nouveau venu reçoit les blocs déjà modifiés.
    let mut carol = TestClient::connect(port, 3, "Carol");
    pump_until(
        &mut [&mut alice, &mut bob, &mut carol],
        "rattrapage des modifications",
        |c| {
            c[2].find(|m| match m {
                ServerMessage::WorldEdits(edits) => edits
                    .iter()
                    .any(|(p, b)| *p == placed && *b == dropped_block)
                    .then_some(()),
                _ => None,
            })
            .is_some()
        },
    );
}
