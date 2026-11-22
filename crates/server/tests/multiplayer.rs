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
use shared::daytime::GameTime;
use shared::item::{Inventory, Item, break_time, drop_for};
use shared::protocol::{ClientMessage, PROTOCOL_ID, PlayerState, ServerMessage, decode, encode};
use shared::world::WorldState;
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
    let data_dir = std::env::temp_dir().join(format!("zs-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data_dir);
    let config = ServerConfig {
        port,
        data_dir: data_dir.clone(),
        ..ServerConfig::default()
    };
    let server = spawn_local(config.clone()).unwrap();
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

    // Arrêt du serveur : la partie est sauvegardée.
    let alice_apples = 3;
    drop(carol);
    drop(bob);
    drop(alice);
    drop(server);
    assert!(data_dir.join("world.sav").exists(), "pas de sauvegarde à l'arrêt");

    // Au redémarrage, le bloc posé est toujours là et Alice retrouve son inventaire.
    let server = spawn_local(config).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let mut alice = TestClient::connect(port, 4, "Alice");
    pump_until(&mut [&mut alice], "retour d'Alice après redémarrage", |c| {
        let edits = c[0]
            .find(|m| match m {
                ServerMessage::WorldEdits(edits) => edits
                    .iter()
                    .any(|(p, b)| *p == placed && *b == dropped_block)
                    .then_some(()),
                _ => None,
            })
            .is_some();
        let inventory = c[0]
            .find(|m| match m {
                ServerMessage::Inventory(inv) => (inv.count(Item::Apple) == alice_apples).then_some(()),
                _ => None,
            })
            .is_some();
        edits && inventory
    });
    drop(server);
    let _ = std::fs::remove_dir_all(&data_dir);
}

#[test]
fn deposits_are_harvested_many_times() {
    let port = 18_000 + (std::process::id() % 1000) as u16;
    let data_dir = std::env::temp_dir().join(format!("zs-test-filon-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data_dir);
    let config = ServerConfig {
        port,
        data_dir: data_dir.clone(),
        ..ServerConfig::default()
    };
    let _server = spawn_local(config).unwrap();
    std::thread::sleep(Duration::from_millis(300));

    let mut dana = TestClient::connect(port, 1, "Dana");
    pump_until(&mut [&mut dana], "bienvenue de Dana", |c| {
        c[0].find(|m| matches!(m, ServerMessage::Welcome { .. }).then_some(()))
            .is_some()
    });
    let spawn = dana
        .find(|m| match m {
            ServerMessage::Welcome { spawn, .. } => Some(*spawn),
            _ => None,
        })
        .unwrap();

    // Le rocher de charbon le plus proche (le charbon se récolte à la main).
    let generator = WorldGenerator::new(WorldConfig::default());
    let (bx, ground, bz, _) = (0..43)
        .flat_map(|cx| (0..43).map(move |cz| (cx, cz)))
        .filter_map(|(cx, cz)| generator.boulder_in_cell(cx, cz))
        .filter(|b| b.3 == Block::CoalOre)
        .filter(|b| generator.block_at(b.0, b.1 + 2, b.2) == Block::Air)
        .min_by_key(|b| (b.0 - spawn.x as i32).pow(2) + (b.2 - spawn.z as i32).pow(2))
        .expect("aucun rocher de charbon");

    // Dana y va par les airs, par petits pas que le serveur accepte.
    let top = Vec3::new(bx as f32 + 0.5, ground as f32 + 2.0, bz as f32 + 0.5);
    let sky = 120.0;
    let mut path = vec![Vec3::new(spawn.x, sky, spawn.z), Vec3::new(top.x, sky, top.z), top];
    let mut position = spawn;
    path.reverse();
    while let Some(target) = path.pop() {
        while position.distance(target) > 0.01 {
            let step = (target - position).clamp_length_max(3.0);
            position += step;
            dana.send(
                &ClientMessage::State(PlayerState {
                    position,
                    yaw: 0.0,
                    pitch: 0.0,
                }),
                false,
            );
            for _ in 0..6 {
                dana.pump();
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
    assert!(
        dana.find(|m| matches!(m, ServerMessage::Teleport { .. }).then_some(()))
            .is_none(),
        "déplacement refusé"
    );

    let target = IVec3::new(bx, ground, bz);
    assert_eq!(generator.block_at(target.x, target.y, target.z), Block::CoalOre);
    let needed = break_time(Block::CoalOre, None).unwrap();
    let wait = |client: &mut TestClient| {
        let start = Instant::now();
        while start.elapsed().as_secs_f32() < needed + 0.1 {
            client.pump();
            std::thread::sleep(Duration::from_millis(10));
        }
    };

    dana.send(&ClientMessage::StartMining { pos: target }, true);
    for expected in [19u8, 18] {
        wait(&mut dana);
        dana.received.clear();
        dana.send(&ClientMessage::BreakBlock { pos: target }, true);
        pump_until(&mut [&mut dana], "récolte du filon", |c| {
            c[0].find(|m| match m {
                ServerMessage::Deposit { pos, remaining } if *pos == target && *remaining == expected => Some(()),
                _ => None,
            })
            .is_some()
        });
        assert!(
            dana.find(|m| matches!(m, ServerMessage::BlockChanged { .. }).then_some(()))
                .is_none(),
            "le filon ne doit pas disparaître"
        );
    }
    pump_until(&mut [&mut dana], "charbon dans l'inventaire", |c| {
        c[0].find(|m| match m {
            ServerMessage::Inventory(inv) if inv.count(Item::Coal) == 2 => Some(()),
            _ => None,
        })
        .is_some()
    });
    let _ = std::fs::remove_dir_all(&data_dir);
}

#[test]
fn bow_shoots_and_uses_arrows() {
    let port = 19_000 + (std::process::id() % 1000) as u16;
    let data_dir = std::env::temp_dir().join(format!("zs-test-arc-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data_dir);

    // Eve revient d'une partie précédente avec un arc et 5 flèches.
    let world = WorldState::new(WorldConfig::default());
    let spawn = server::world::find_spawn(&world);
    let mut inventory = Inventory::default();
    inventory.add(Item::Bow, 1);
    inventory.add(Item::Arrow, 5);
    let save = server::save::SaveFile {
        version: server::save::SAVE_VERSION,
        world: WorldConfig::default(),
        time: GameTime::START,
        edits: Vec::new(),
        players: vec![(
            "Eve".into(),
            server::save::SavedPlayer {
                position: spawn,
                health: 100.0,
                hunger: 100.0,
                inventory,
            },
        )],
        deposits: Vec::new(),
    };
    server::save::write(&data_dir, &save).unwrap();

    let config = ServerConfig {
        port,
        data_dir: data_dir.clone(),
        ..ServerConfig::default()
    };
    let _server = spawn_local(config).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let mut eve = TestClient::connect(port, 1, "Eve");
    pump_until(&mut [&mut eve], "inventaire d'Eve", |c| {
        c[0].find(|m| match m {
            ServerMessage::Inventory(inv) if inv.has(Item::Bow, 1) => Some(()),
            _ => None,
        })
        .is_some()
    });

    // Sans l'arc en main, le tir est ignoré ; avec, une flèche part.
    eve.send(&ClientMessage::Shoot { direction: Vec3::X }, true);
    eve.send(&ClientMessage::Hold { item: Some(Item::Bow) }, true);
    eve.send(&ClientMessage::Shoot { direction: Vec3::X }, true);
    pump_until(&mut [&mut eve], "tir reçu", |c| {
        c[0].find(|m| match m {
            ServerMessage::Shot { from: 1, .. } => Some(()),
            _ => None,
        })
        .is_some()
    });
    pump_until(&mut [&mut eve], "flèche consommée", |c| {
        c[0].find(|m| match m {
            ServerMessage::Inventory(inv) if inv.count(Item::Arrow) == 4 => Some(()),
            _ => None,
        })
        .is_some()
    });
    let shots = eve
        .received
        .iter()
        .filter(|m| matches!(m, ServerMessage::Shot { .. }))
        .count();
    assert_eq!(shots, 1, "le premier tir, sans arc en main, devait être ignoré");
    let _ = std::fs::remove_dir_all(&data_dir);
}
