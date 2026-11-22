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
    inventory.add(Item::Grenade, 2);
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
                thirst: 100.0,
            },
        )],
        deposits: Vec::new(),
        chests: Vec::new(),
        survivors: Vec::new(),
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

    // Une grenade lancée part, puis explose après sa mèche.
    std::thread::sleep(Duration::from_millis(900));
    eve.received.clear();
    eve.send(
        &ClientMessage::Hold {
            item: Some(Item::Grenade),
        },
        true,
    );
    eve.send(&ClientMessage::Throw { direction: Vec3::X }, true);
    pump_until(&mut [&mut eve], "grenade lancée", |c| {
        let thrown = c[0]
            .find(|m| {
                matches!(
                    m,
                    ServerMessage::Thrown {
                        item: Item::Grenade,
                        ..
                    }
                )
                .then_some(())
            })
            .is_some();
        let used = c[0]
            .find(|m| match m {
                ServerMessage::Inventory(inv) if inv.count(Item::Grenade) == 1 => Some(()),
                _ => None,
            })
            .is_some();
        thrown && used
    });
    pump_until(&mut [&mut eve], "explosion", |c| {
        c[0].find(|m| matches!(m, ServerMessage::Explosion { .. }).then_some(()))
            .is_some()
    });
    let _ = std::fs::remove_dir_all(&data_dir);
}

#[test]
fn doors_chests_and_chat() {
    let port = 20_000 + (std::process::id() % 1000) as u16;
    let data_dir = std::env::temp_dir().join(format!("zs-test-coffre-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data_dir);

    // Fay revient avec une porte, un coffre et des pommes.
    let world = WorldState::new(WorldConfig::default());
    let spawn = server::world::find_spawn(&world);
    let mut inventory = Inventory::default();
    inventory.add(Item::Block(Block::Door), 1);
    inventory.add(Item::Block(Block::Chest), 1);
    inventory.add(Item::Apple, 5);
    let save = server::save::SaveFile {
        version: server::save::SAVE_VERSION,
        world: WorldConfig::default(),
        time: GameTime::START,
        edits: Vec::new(),
        players: vec![(
            "Fay".into(),
            server::save::SavedPlayer {
                position: spawn,
                health: 100.0,
                hunger: 100.0,
                inventory,
                thirst: 100.0,
            },
        )],
        deposits: Vec::new(),
        chests: Vec::new(),
        survivors: Vec::new(),
    };
    server::save::write(&data_dir, &save).unwrap();
    let config = ServerConfig {
        port,
        data_dir: data_dir.clone(),
        ..ServerConfig::default()
    };
    let _server = spawn_local(config).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let mut fay = TestClient::connect(port, 1, "Fay");
    let mut gus = TestClient::connect(port, 2, "Gus");
    pump_until(&mut [&mut fay, &mut gus], "arrivée", |c| {
        c[0].find(|m| matches!(m, ServerMessage::PlayerJoined { id: 2, .. }).then_some(()))
            .is_some()
    });

    // Deux cases d'air au-dessus de la tête de Fay, à portée.
    let feet = spawn.floor().as_ivec3();
    let spots: Vec<IVec3> = [(1, 3, 0), (-1, 3, 0), (0, 3, 1), (0, 3, -1), (1, 3, 1)]
        .into_iter()
        .map(|(x, y, z)| feet + IVec3::new(x, y, z))
        .filter(|p| !world.is_solid(*p))
        .take(2)
        .collect();
    let (door, chest) = (spots[0], spots[1]);
    for (pos, block) in [(door, Block::Door), (chest, Block::Chest)] {
        fay.send(&ClientMessage::PlaceBlock { pos, block }, true);
    }

    // La porte s'ouvre pour tout le monde.
    fay.send(&ClientMessage::ToggleDoor { pos: door }, true);
    pump_until(&mut [&mut fay, &mut gus], "porte ouverte", |c| {
        c[1].find(|m| match m {
            ServerMessage::BlockChanged {
                pos,
                block: Block::DoorOpen,
            } if *pos == door => Some(()),
            _ => None,
        })
        .is_some()
    });

    // Fay dépose 3 pommes dans le coffre.
    fay.received.clear();
    fay.send(&ClientMessage::OpenChest { pos: chest }, true);
    fay.send(
        &ClientMessage::ChestPut {
            item: Item::Apple,
            count: 3,
        },
        true,
    );
    pump_until(&mut [&mut fay, &mut gus], "pommes dans le coffre", |c| {
        c[0].find(|m| match m {
            ServerMessage::Chest { contents, .. } if contents.count(Item::Apple) == 3 => Some(()),
            _ => None,
        })
        .is_some()
    });

    // Gus n'est pas à portée tant qu'il n'est pas venu ; il discute en attendant.
    gus.send(
        &ClientMessage::Chat {
            text: "  j'arrive !\n".into(),
        },
        true,
    );
    pump_until(&mut [&mut fay, &mut gus], "message de Gus", |c| {
        c[0].find(|m| match m {
            ServerMessage::Chat { from, text } if from == "Gus" && text == "j'arrive !" => Some(()),
            _ => None,
        })
        .is_some()
    });

    // Fay casse le coffre : elle récupère le coffre et ce qu'il contenait.
    fay.send(&ClientMessage::StartMining { pos: chest }, true);
    let start = Instant::now();
    while start.elapsed().as_secs_f32() < break_time(Block::Chest, None).unwrap() + 0.1 {
        fay.pump();
        gus.pump();
        std::thread::sleep(Duration::from_millis(10));
    }
    fay.received.clear();
    fay.send(&ClientMessage::BreakBlock { pos: chest }, true);
    pump_until(&mut [&mut fay, &mut gus], "coffre cassé, contenu récupéré", |c| {
        let closed = c[0]
            .find(|m| matches!(m, ServerMessage::ChestClosed).then_some(()))
            .is_some();
        let back = c[0]
            .find(|m| match m {
                ServerMessage::Inventory(inv)
                    if inv.count(Item::Apple) == 5 && inv.has(Item::Block(Block::Chest), 1) =>
                {
                    Some(())
                }
                _ => None,
            })
            .is_some();
        closed && back
    });
    let _ = std::fs::remove_dir_all(&data_dir);
}

#[test]
fn survivors_are_recruited_and_given_orders() {
    use server::save::SavedSurvivor;
    use shared::survivor::{Order, SurvivorStats};

    let port = 21_000 + (std::process::id() % 1000) as u16;
    let data_dir = std::env::temp_dir().join(format!("zs-test-survivants-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data_dir);

    // Hal a une ration et deux pommes ; deux survivants attendent à côté de lui.
    let world = WorldState::new(WorldConfig::default());
    let spawn = server::world::find_spawn(&world);
    let mut inventory = Inventory::default();
    inventory.add(Item::Ration, 1);
    inventory.add(Item::Apple, 2);
    inventory.add(Item::Sword(shared::item::Tier::Iron), 1);
    inventory.add(Item::Helmet(shared::item::Tier::Iron), 1);
    let wild = |name: &str| SavedSurvivor {
        name: name.into(),
        stats: SurvivorStats::from_seed(99),
        health: 100.0,
        position: spawn,
        post: spawn,
        leader: None,
        order: Order::Idle,
        weapon: None,
        armor: [None; 2],
    };
    let save = server::save::SaveFile {
        version: server::save::SAVE_VERSION,
        world: WorldConfig::default(),
        time: GameTime::START,
        edits: Vec::new(),
        players: vec![(
            "Hal".into(),
            server::save::SavedPlayer {
                position: spawn,
                health: 100.0,
                hunger: 100.0,
                inventory,
                thirst: 100.0,
            },
        )],
        deposits: Vec::new(),
        chests: Vec::new(),
        survivors: vec![wild("Marie"), wild("Lucas")],
    };
    server::save::write(&data_dir, &save).unwrap();
    let config = ServerConfig {
        port,
        data_dir: data_dir.clone(),
        ..ServerConfig::default()
    };
    let server = spawn_local(config.clone()).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let mut hal = TestClient::connect(port, 1, "Hal");

    // Hal apprend qui sont les survivants et les voit bouger.
    pump_until(&mut [&mut hal], "survivants connus et visibles", |c| {
        let named = c[0]
            .received
            .iter()
            .filter(|m| matches!(m, ServerMessage::SurvivorInfo { leader: None, .. }))
            .count();
        let seen = c[0]
            .find(|m| matches!(m, ServerMessage::Survivors(list) if list.len() == 2).then_some(()))
            .is_some();
        named == 2 && seen
    });
    let id_of = |hal: &TestClient, who: &str| {
        hal.find(|m| match m {
            ServerMessage::SurvivorInfo { id, name, .. } if name == who => Some(*id),
            _ => None,
        })
        .unwrap()
    };
    let (marie, lucas) = (id_of(&hal, "Marie"), id_of(&hal, "Lucas"));

    // La ration suffit à recruter Marie.
    hal.received.clear();
    hal.send(
        &ClientMessage::TalkToSurvivor {
            id: marie,
            dismiss: false,
            give: false,
        },
        true,
    );
    pump_until(&mut [&mut hal], "Marie recrutée", |c| {
        let joined = c[0]
            .find(|m| match m {
                ServerMessage::SurvivorInfo {
                    id,
                    leader: Some(leader),
                    order: Order::Follow,
                    ..
                } if *id == marie && leader == "Hal" => Some(()),
                _ => None,
            })
            .is_some();
        let paid = c[0]
            .find(|m| match m {
                ServerMessage::Inventory(inv) if !inv.has(Item::Ration, 1) && inv.count(Item::Apple) == 2 => Some(()),
                _ => None,
            })
            .is_some();
        joined && paid
    });

    // Deux pommes ne suffisent pas pour Lucas.
    hal.received.clear();
    hal.send(
        &ClientMessage::TalkToSurvivor {
            id: lucas,
            dismiss: false,
            give: false,
        },
        true,
    );
    pump_until(&mut [&mut hal], "Lucas refuse", |c| {
        c[0].find(|m| match m {
            ServerMessage::Chat { from, text } if from == "Lucas" && text.contains("faim") => Some(()),
            _ => None,
        })
        .is_some()
    });

    // Un deuxième clic poste Marie en garde.
    hal.send(
        &ClientMessage::TalkToSurvivor {
            id: marie,
            dismiss: false,
            give: false,
        },
        true,
    );
    pump_until(&mut [&mut hal], "Marie monte la garde", |c| {
        c[0].find(|m| match m {
            ServerMessage::SurvivorInfo {
                id,
                order: Order::Guard,
                ..
            } if *id == marie => Some(()),
            _ => None,
        })
        .is_some()
    });

    // Hal lui confie son épée.
    let sword = Item::Sword(shared::item::Tier::Iron);
    hal.received.clear();
    hal.send(&ClientMessage::Hold { item: Some(sword) }, true);
    hal.send(
        &ClientMessage::TalkToSurvivor {
            id: marie,
            dismiss: false,
            give: true,
        },
        true,
    );
    pump_until(&mut [&mut hal], "Marie reçoit l'épée", |c| {
        let armed = c[0]
            .find(|m| match m {
                ServerMessage::SurvivorInfo { id, weapon, .. } if *id == marie && *weapon == Some(sword) => Some(()),
                _ => None,
            })
            .is_some();
        let given = c[0]
            .find(|m| match m {
                ServerMessage::Inventory(inv) if !inv.has(sword, 1) => Some(()),
                _ => None,
            })
            .is_some();
        armed && given
    });

    // Puis son casque.
    let helmet = Item::Helmet(shared::item::Tier::Iron);
    hal.received.clear();
    hal.send(&ClientMessage::Hold { item: Some(helmet) }, true);
    hal.send(
        &ClientMessage::TalkToSurvivor {
            id: marie,
            dismiss: false,
            give: true,
        },
        true,
    );
    pump_until(&mut [&mut hal], "Marie porte le casque", |c| {
        c[0].find(|m| match m {
            ServerMessage::SurvivorInfo { id, weapon, armor, .. }
                if *id == marie && *weapon == Some(sword) && armor[0] == Some(helmet) =>
            {
                Some(())
            }
            _ => None,
        })
        .is_some()
    });

    // À l'arrêt, seule Marie (recrutée) est sauvegardée, avec son ordre et son arme.
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(700) {
        hal.pump();
        std::thread::sleep(Duration::from_millis(10));
    }
    drop(server);
    let saved = server::save::load(&data_dir).unwrap().unwrap();
    assert_eq!(saved.survivors.len(), 1);
    assert_eq!(saved.survivors[0].name, "Marie");
    assert_eq!(saved.survivors[0].leader.as_deref(), Some("Hal"));
    assert_eq!(saved.survivors[0].order, Order::Guard);
    assert_eq!(saved.survivors[0].weapon, Some(sword));
    assert_eq!(saved.survivors[0].armor, [Some(helmet), None]);
    let _ = std::fs::remove_dir_all(&data_dir);
}
