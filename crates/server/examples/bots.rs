//! Bots de test de charge : `cargo run -p server --release --example bots -- 127.0.0.1:7777 4`
//!
//! Chaque bot se connecte, reste près du point d'apparition en tournant sur lui-même
//! et réapparaît quand il meurt. Combiné à `STRESS_ZOMBIES=500` côté serveur, cela
//! permet de mesurer le coût des hordes.

use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant, SystemTime};

use bevy::math::Vec3;
use bevy_renet::netcode::{ClientAuthentication, NetcodeClientTransport};
use bevy_renet::renet::{ConnectionConfig, DefaultChannel, RenetClient};
use shared::protocol::{ClientMessage, PROTOCOL_ID, PlayerState, ServerMessage, decode, encode};

struct Bot {
    client: RenetClient,
    transport: NetcodeClientTransport,
    spawn: Option<Vec3>,
    yaw: f32,
    zombies_seen: usize,
    bytes: usize,
}

fn main() {
    let mut args = std::env::args().skip(1);
    let addr: SocketAddr = args
        .next()
        .unwrap_or_else(|| "127.0.0.1:7777".into())
        .parse()
        .expect("adresse invalide");
    let count: usize = args.next().and_then(|n| n.parse().ok()).unwrap_or(2);

    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap();
    let mut bots: Vec<Bot> = (0..count)
        .map(|i| {
            let socket = UdpSocket::bind("0.0.0.0:0").unwrap();
            let auth = ClientAuthentication::Unsecure {
                client_id: now.as_millis() as u64 + i as u64,
                protocol_id: PROTOCOL_ID,
                server_addr: addr,
                user_data: None,
            };
            let mut client = RenetClient::new(ConnectionConfig::default());
            client.send_message(
                DefaultChannel::ReliableOrdered,
                encode(&ClientMessage::Hello {
                    name: format!("Bot {i}"),
                }),
            );
            Bot {
                transport: NetcodeClientTransport::new(now, auth, socket).unwrap(),
                client,
                spawn: None,
                yaw: i as f32,
                zombies_seen: 0,
                bytes: 0,
            }
        })
        .collect();

    let mut last = Instant::now();
    let mut report = Instant::now();
    loop {
        let dt = last.elapsed();
        last = Instant::now();
        for bot in &mut bots {
            bot.client.update(dt);
            if bot.transport.0.update(dt, &mut bot.client).is_err() {
                continue;
            }
            for channel in [2u8, 0u8] {
                while let Some(bytes) = bot.client.receive_message(channel) {
                    bot.bytes += bytes.len();
                    match decode::<ServerMessage>(&bytes) {
                        Some(ServerMessage::Welcome { spawn, .. }) => bot.spawn = Some(spawn),
                        Some(ServerMessage::Teleport { position }) => bot.spawn = Some(position),
                        Some(ServerMessage::Zombies(z)) => bot.zombies_seen = bot.zombies_seen.max(z.len()),
                        Some(ServerMessage::Died { .. }) => {
                            bot.client
                                .send_message(DefaultChannel::ReliableOrdered, encode(&ClientMessage::Respawn));
                        }
                        _ => {}
                    }
                }
            }
            if let Some(spawn) = bot.spawn {
                bot.yaw += dt.as_secs_f32();
                let state = PlayerState {
                    position: spawn,
                    yaw: bot.yaw,
                    pitch: 0.0,
                    // Frappe une seconde sur deux, pour voir l'animation chez les autres.
                    swinging: bot.yaw % 2.0 < 1.0,
                };
                bot.client
                    .send_message(DefaultChannel::Unreliable, encode(&ClientMessage::State(state)));
            }
            let _ = bot.transport.0.send_packets(&mut bot.client);
        }
        if report.elapsed() > Duration::from_secs(5) {
            let secs = report.elapsed().as_secs_f32();
            for (i, bot) in bots.iter_mut().enumerate() {
                println!(
                    "bot {i} : connecté={} zombies reçus (max par paquet)={} débit={:.1} Ko/s",
                    bot.client.is_connected(),
                    bot.zombies_seen,
                    bot.bytes as f32 / 1024.0 / secs
                );
                bot.bytes = 0;
                bot.zombies_seen = 0;
            }
            report = Instant::now();
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
