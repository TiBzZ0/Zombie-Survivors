//! Horloge de la partie : jour, nuit, nuits de grande horde.

use bevy::prelude::*;
use bevy_renet::RenetServer;
use bevy_renet::renet::DefaultChannel;
use shared::daytime::GameTime;
use shared::protocol::{ServerMessage, encode};

use crate::zombies::Zombie;

pub struct DayNightPlugin;

impl Plugin for DayNightPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Clock(GameTime::START))
            .add_systems(FixedUpdate, (advance_clock, broadcast_time).chain());
    }
}

/// Heure de la partie.
#[derive(Resource, Debug, Clone, Copy, Deref)]
pub struct Clock(pub GameTime);

fn advance_clock(
    time: Res<Time>,
    mut clock: ResMut<Clock>,
    mut server: Option<ResMut<RenetServer>>,
    lobby: Option<ResMut<crate::net::Lobby>>,
) {
    let was_night = clock.0.is_night();
    let horde = clock.0.is_horde_night();
    let blood = clock.0.is_blood_moon();
    clock.0.advance(time.delta_secs());
    let is_night = clock.0.is_night();
    if was_night == is_night {
        return;
    }
    let text = if is_night && clock.0.is_horde_night() {
        format!("Nuit {} : LA GRANDE HORDE ARRIVE !", clock.0.day)
    } else if is_night && clock.0.is_blood_moon() {
        format!(
            "Nuit {} : la lune se lève, rouge sang. Les morts sont plus nombreux et plus rapides…",
            clock.0.day
        )
    } else if is_night {
        format!("La nuit {} tombe. Les morts se réveillent…", clock.0.day)
    } else {
        format!("L'aube du jour {} se lève. Les zombies brûlent.", clock.0.day)
    };
    info!("{text}");
    // Chaque nuit traversée vivant rapporte de l'expérience (le double après une grande horde).
    if !is_night && let Some(mut lobby) = lobby {
        // Une nuit de lune de sang rapporte une fois et demie plus.
        let xp = shared::skills::NIGHT_SURVIVED_XP
            * if horde {
                4
            } else if blood {
                3
            } else {
                2
            }
            / 2;
        for (id, player) in lobby.players.iter_mut().filter(|(_, p)| p.is_alive()) {
            player.progress.add_xp(xp);
            if let Some(server) = server.as_deref_mut() {
                crate::net::send_progress(server, *id, player);
            }
        }
    }
    if let Some(server) = server.as_deref_mut() {
        server.broadcast_message(
            DefaultChannel::ReliableOrdered,
            encode(&ServerMessage::Chat {
                from: "Serveur".into(),
                text,
            }),
        );
    }
}

fn broadcast_time(
    time: Res<Time>,
    clock: Res<Clock>,
    zombies: Query<(), With<Zombie>>,
    server: Option<ResMut<RenetServer>>,
    mut since: Local<f32>,
) {
    *since += time.delta_secs();
    if *since < 1.0 {
        return;
    }
    *since = 0.0;
    if let Some(mut server) = server {
        let message = ServerMessage::Time {
            time: clock.0,
            zombies_alive: zombies.iter().count() as u32,
        };
        server.broadcast_message(DefaultChannel::ReliableOrdered, encode(&message));
    }
}
