//! Pêche côté serveur : attente de la touche, fenêtre pour ferrer, prise.

use bevy::prelude::*;
use bevy_renet::RenetServer;
use bevy_renet::renet::{ClientId, DefaultChannel};
use shared::fishing::{BITE_WINDOW, FISH_XP, LINE_LENGTH, catch, wait};
use shared::item::Item;
use shared::physics::PLAYER_EYE_HEIGHT;
use shared::protocol::{ServerMessage, encode};

use crate::net::{ConnectedPlayer, Lobby, send_inventory, send_progress};

pub struct FishingPlugin;

impl Plugin for FishingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, update_lines);
    }
}

/// Une ligne à l'eau.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Line {
    /// Bloc d'eau où flotte le bouchon.
    pub at: IVec3,
    /// Secondes avant la prochaine touche.
    pub wait: f32,
    /// Secondes restantes pour ferrer (0 : pas de touche en cours).
    pub bite: f32,
}

impl Line {
    pub fn cast(at: IVec3) -> Self {
        Self {
            at,
            wait: wait(fastrand::f32()),
            bite: 0.0,
        }
    }
}

fn send(server: Option<&mut RenetServer>, client: ClientId, message: &ServerMessage) {
    if let Some(server) = server {
        server.send_message(client, DefaultChannel::ReliableOrdered, encode(message));
    }
}

/// Remonte la ligne : une prise si le poisson mord, rien sinon.
pub fn reel(client: ClientId, player: &mut ConnectedPlayer, mut server: Option<&mut RenetServer>) {
    let Some(line) = player.fishing.take() else {
        return;
    };
    let item = (line.bite > 0.0).then(|| catch(fastrand::f32(), fastrand::f32()));
    if let Some((item, n)) = item {
        player.inventory.add(item, n);
        player.progress.add_xp(FISH_XP);
        player.record.fish += 1;
        if let Some(server) = server.as_deref_mut() {
            send_inventory(server, client, player);
            send_progress(server, client, player);
        }
    }
    send(server, client, &ServerMessage::Caught { item });
}

/// Fait avancer les lignes à l'eau : touches, poissons qui s'échappent, lignes cassées.
fn update_lines(time: Res<Time>, mut lobby: ResMut<Lobby>, mut server: Option<ResMut<RenetServer>>) {
    let dt = time.delta_secs();
    for (id, player) in lobby.players.iter_mut() {
        let Some(mut line) = player.fishing else {
            continue;
        };
        // Canne rangée, joueur mort ou trop loin : la ligne est perdue.
        let eye = player.state.position + Vec3::Y * PLAYER_EYE_HEIGHT;
        let far = eye.distance(line.at.as_vec3() + Vec3::splat(0.5)) > LINE_LENGTH;
        if !player.is_alive() || player.held_item() != Some(Item::FishingRod) || far {
            player.fishing = None;
            send(server.as_deref_mut(), *id, &ServerMessage::Caught { item: None });
            continue;
        }
        if line.bite > 0.0 {
            line.bite -= dt;
            if line.bite <= 0.0 {
                // Trop tard : le poisson s'en va, un autre viendra.
                line.bite = 0.0;
                line.wait = wait(fastrand::f32());
            }
        } else {
            line.wait -= dt;
            if line.wait <= 0.0 {
                line.bite = BITE_WINDOW;
                send(server.as_deref_mut(), *id, &ServerMessage::Bite);
            }
        }
        player.fishing = Some(line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fish_bites_and_must_be_struck_in_time() {
        let mut app = App::new();
        app.init_resource::<Lobby>()
            .init_resource::<Time>()
            .add_systems(Update, update_lines);
        let mut player = ConnectedPlayer::new(Vec3::new(0.5, 10.0, 0.5));
        player.name = "Pêcheur".into();
        player.inventory.add(Item::FishingRod, 1);
        player.held = Some(Item::FishingRod);
        player.fishing = Some(Line::cast(IVec3::new(4, 9, 0)));
        app.world_mut().resource_mut::<Lobby>().players.insert(1, player);
        let step = |app: &mut App| {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::from_secs_f32(0.1));
            app.update();
        };
        // Tôt ou tard, le bouchon plonge.
        let mut bitten = false;
        for _ in 0..200 {
            step(&mut app);
            if app.world().resource::<Lobby>().players[&1].fishing.unwrap().bite > 0.0 {
                bitten = true;
                break;
            }
        }
        assert!(bitten);
        let mut lobby = app.world_mut().resource_mut::<Lobby>();
        let player = lobby.players.get_mut(&1).unwrap();
        let before = player.inventory.clone();
        reel(1, player, None);
        assert!(player.fishing.is_none());
        assert_ne!(player.inventory, before, "la prise est dans le sac");
        assert_eq!(player.progress.xp, FISH_XP);

        // Ferrer trop tôt ne rapporte rien.
        player.fishing = Some(Line::cast(IVec3::new(4, 9, 0)));
        let before = player.inventory.clone();
        reel(1, player, None);
        assert_eq!(player.inventory, before);

        // Ranger la canne casse la ligne.
        player.fishing = Some(Line::cast(IVec3::new(4, 9, 0)));
        player.held = None;
        step(&mut app);
        assert!(app.world().resource::<Lobby>().players[&1].fishing.is_none());
    }
}
