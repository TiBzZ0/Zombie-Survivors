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

fn advance_clock(time: Res<Time>, mut clock: ResMut<Clock>, server: Option<ResMut<RenetServer>>) {
    let was_night = clock.0.is_night();
    clock.0.advance(time.delta_secs());
    let is_night = clock.0.is_night();
    if was_night == is_night {
        return;
    }
    let text = if is_night && clock.0.is_horde_night() {
        format!("Nuit {} : LA GRANDE HORDE ARRIVE !", clock.0.day)
    } else if is_night {
        format!("La nuit {} tombe. Les morts se réveillent…", clock.0.day)
    } else {
        format!("L'aube du jour {} se lève. Les zombies brûlent.", clock.0.day)
    };
    info!("{text}");
    if let Some(mut server) = server {
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
