//! Succès : vérifie les compteurs des joueurs, débloque ce qui est atteint, donne
//! l'expérience promise et tient le client au courant de ses statistiques.

use std::collections::HashMap;

use bevy::prelude::*;
use bevy_renet::RenetServer;
use bevy_renet::renet::{ClientId, DefaultChannel};
use shared::achievements::Record;
use shared::protocol::{ServerMessage, encode};

use crate::net::{Lobby, send, send_progress};

/// Intervalle entre deux vérifications.
const CHECK_SECS: f32 = 0.5;

pub struct AchievementsPlugin;

impl Plugin for AchievementsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, check_achievements);
    }
}

fn check_achievements(
    time: Res<Time>,
    mut lobby: ResMut<Lobby>,
    mut server: Option<ResMut<RenetServer>>,
    mut since: Local<f32>,
    mut sent: Local<HashMap<ClientId, Record>>,
) {
    *since += time.delta_secs();
    if *since < CHECK_SECS {
        return;
    }
    *since = 0.0;
    sent.retain(|id, _| lobby.players.contains_key(id));
    let mut news = Vec::new();
    for (id, player) in lobby.players.iter_mut().filter(|(_, p)| p.is_ready()) {
        let earned = player.record.newly_earned();
        for achievement in &earned {
            let info = achievement.info();
            player.record.unlocked.push(*achievement);
            player.progress.add_xp(info.xp);
            info!("{} débloque le succès « {} »", player.name, info.title);
            news.push(format!("{} a débloqué le succès « {} » !", player.name, info.title));
        }
        let Some(server) = server.as_deref_mut() else {
            continue;
        };
        for achievement in &earned {
            send(server, *id, &ServerMessage::Achievement(*achievement));
        }
        if !earned.is_empty() {
            send_progress(server, *id, player);
        }
        if sent.get(id) != Some(&player.record) {
            send(server, *id, &ServerMessage::Record(player.record.clone()));
            sent.insert(*id, player.record.clone());
        }
    }
    if let Some(server) = server.as_deref_mut() {
        for text in news {
            server.broadcast_message(
                DefaultChannel::ReliableOrdered,
                encode(&ServerMessage::Chat {
                    from: "Serveur".into(),
                    text,
                }),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use shared::achievements::Achievement;
    use shared::zombie::ZombieKind;

    use super::*;
    use crate::net::ConnectedPlayer;

    #[test]
    fn reaching_a_goal_unlocks_the_achievement_once_with_its_xp() {
        let mut app = App::new();
        app.init_resource::<Time>()
            .init_resource::<Lobby>()
            .add_systems(Update, check_achievements);
        let mut player = ConnectedPlayer::new(Vec3::ZERO);
        player.name = "Alice".into();
        player.record.add_kill(ZombieKind::Walker);
        app.world_mut().resource_mut::<Lobby>().players.insert(1, player);
        for _ in 0..3 {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::from_secs_f32(0.6));
            app.update();
        }
        let lobby = app.world().resource::<Lobby>();
        let p = &lobby.players[&1];
        assert_eq!(p.record.unlocked, vec![Achievement::FirstBlood]);
        assert_eq!(p.progress.xp, Achievement::FirstBlood.info().xp);
    }
}
