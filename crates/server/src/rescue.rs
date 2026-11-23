//! Évacuation côté serveur : appels de détresse lancés depuis une radio alimentée,
//! attente de l'hélicoptère, embarquement des joueurs proches au départ.

use std::collections::HashSet;

use bevy::prelude::*;
use bevy_renet::RenetServer;
use bevy_renet::renet::ClientId;
use shared::block::{Block, POWER_RADIUS};
use shared::protocol::ServerMessage;
use shared::rescue::{BOARD_RADIUS, CALL_REACH, PAD_OFFSET, RESCUE_NIGHTS, RescueState, WAIT_SECS};

use crate::daynight::Clock;
use crate::electricity::Power;
use crate::net::{Lobby, broadcast, notify, send};
use crate::world::GameWorld;

pub struct RescuePlugin;

impl Plugin for RescuePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RadioCalls>()
            .init_resource::<Rescue>()
            .add_systems(Update, run_rescue);
    }
}

/// Appels reçus des joueurs depuis le dernier passage (joueur, radio).
#[derive(Resource, Default)]
pub struct RadioCalls(pub Vec<(ClientId, IVec3)>);

/// Appel en cours, gardé dans la sauvegarde.
#[derive(Resource, Default, Debug)]
pub struct Rescue(pub Option<RescueState>);

fn chat(server: Option<&mut RenetServer>, text: String) {
    info!("{text}");
    if let Some(server) = server {
        broadcast(
            server,
            &ServerMessage::Chat {
                from: "Radio".into(),
                text,
            },
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn run_rescue(
    time: Res<Time>,
    clock: Res<Clock>,
    world: Res<GameWorld>,
    power: Option<Res<Power>>,
    mut calls: ResMut<RadioCalls>,
    mut rescue: ResMut<Rescue>,
    mut lobby: ResMut<Lobby>,
    mut server: Option<ResMut<RenetServer>>,
    mut informed: Local<HashSet<ClientId>>,
) {
    let mut changed = false;
    for (client, pos) in std::mem::take(&mut calls.0) {
        let Some(player) = lobby.players.get(&client) else {
            continue;
        };
        let close = player.state.position.distance(pos.as_vec3() + Vec3::splat(0.5)) <= CALL_REACH;
        if !player.is_alive() || !close || world.block(pos) != Block::Radio {
            continue;
        }
        let refusal = if let Some(r) = rescue.0 {
            Some(format!(
                "Un appel est déjà en cours : l'hélicoptère arrive le jour {} au matin.",
                r.day
            ))
        } else if !power.as_ref().is_some_and(|p| p.powers(pos)) {
            Some(format!(
                "La radio reste muette : il faut un générateur en marche à moins de {POWER_RADIUS:.0} blocs."
            ))
        } else {
            None
        };
        if let Some(text) = refusal {
            if let Some(server) = server.as_deref_mut() {
                notify(server, client, text);
            }
            continue;
        }
        let state = RescueState::new(pos, clock.0.day);
        rescue.0 = Some(state);
        changed = true;
        chat(
            server.as_deref_mut(),
            format!(
                "{} lance un appel de détresse ! Un hélicoptère se posera près de la radio ({}, {}) le jour {} au matin. Tenez {} nuits : le signal attire les morts.",
                player.name, pos.x, pos.z, state.day, RESCUE_NIGHTS
            ),
        );
    }
    if let Some(state) = rescue.0.as_mut() {
        if world.block(state.radio) != Block::Radio && state.landed.is_none() {
            rescue.0 = None;
            changed = true;
            chat(
                server.as_deref_mut(),
                "La radio est détruite : l'appel de détresse est perdu !".into(),
            );
        } else if let Some(left) = state.landed.as_mut() {
            *left -= time.delta_secs();
            if *left <= 0.0 {
                // Décollage : ceux qui sont assez près montent à bord.
                let pad = (state.radio + PAD_OFFSET).as_vec3() + Vec3::splat(0.5);
                let mut saved = Vec::new();
                for (id, player) in lobby.players.iter_mut() {
                    let flat = Vec2::new(player.state.position.x - pad.x, player.state.position.z - pad.z);
                    if player.is_ready() && player.is_alive() && flat.length() <= BOARD_RADIUS {
                        player.record.evacuations += 1;
                        saved.push(player.name.clone());
                        if let Some(server) = server.as_deref_mut() {
                            send(server, *id, &ServerMessage::Evacuated { day: clock.0.day });
                        }
                    }
                }
                rescue.0 = None;
                changed = true;
                let text = if saved.is_empty() {
                    "L'hélicoptère repart à vide…".to_string()
                } else {
                    format!(
                        "L'hélicoptère décolle avec {} à son bord. Évacuation réussie !",
                        saved.join(", ")
                    )
                };
                chat(server.as_deref_mut(), text);
            }
        } else if state.due(clock.0.day, clock.0.phase) {
            state.landed = Some(WAIT_SECS);
            changed = true;
            chat(
                server.as_deref_mut(),
                format!("L'hélicoptère se pose près de la radio ! Il repart dans {WAIT_SECS:.0} s : rejoignez-le."),
            );
        }
    }
    let Some(server) = server.as_deref_mut() else {
        return;
    };
    informed.retain(|id| lobby.players.contains_key(id));
    let message = ServerMessage::Rescue(rescue.0);
    for (id, player) in &lobby.players {
        if player.is_ready() && (changed || !informed.contains(id)) {
            send(server, *id, &message);
            informed.insert(*id);
        }
    }
}

#[cfg(test)]
mod tests {
    use shared::config::WorldConfig;
    use shared::daytime::GameTime;
    use shared::world::WorldState;

    use super::*;
    use crate::net::ConnectedPlayer;

    #[test]
    fn a_powered_radio_brings_the_helicopter_that_takes_nearby_players() {
        let mut world = GameWorld(WorldState::new(WorldConfig::default()));
        let radio = IVec3::new(100, world.generator().surface_height(100, 100) + 20, 100);
        world.set_block(radio, Block::Radio);
        let mut app = App::new();
        app.insert_resource(world)
            .insert_resource(Clock(GameTime { day: 3, phase: 0.3 }))
            .init_resource::<Time>()
            .init_resource::<Lobby>()
            .init_resource::<Power>()
            .add_plugins(RescuePlugin);
        let mut player = ConnectedPlayer::new(radio.as_vec3() + Vec3::new(2.0, 0.0, 0.0));
        player.name = "Alice".into();
        app.world_mut().resource_mut::<Lobby>().players.insert(1, player);
        // Sans générateur, la radio reste muette.
        app.world_mut().resource_mut::<RadioCalls>().0.push((1, radio));
        app.update();
        assert!(app.world().resource::<Rescue>().0.is_none());
        app.world_mut()
            .resource_mut::<Power>()
            .running
            .push(radio + IVec3::X * 3);
        app.world_mut().resource_mut::<RadioCalls>().0.push((1, radio));
        app.update();
        assert_eq!(app.world().resource::<Rescue>().0.map(|r| r.day), Some(5));
        // Deux nuits plus tard, il se pose puis repart avec Alice.
        app.world_mut().resource_mut::<Clock>().0 = GameTime { day: 5, phase: 0.1 };
        app.update();
        assert!(app.world().resource::<Rescue>().0.unwrap().landed.is_some());
        for _ in 0..62 {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::from_secs(1));
            app.update();
        }
        assert!(app.world().resource::<Rescue>().0.is_none());
        assert_eq!(app.world().resource::<Lobby>().players[&1].record.evacuations, 1);
    }
}
