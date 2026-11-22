//! Messages échangés entre client et serveur, sérialisés avec postcard.

use bevy_math::{IVec3, Vec3};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::block::Block;
use crate::config::WorldConfig;

/// Change à chaque modification incompatible du protocole.
pub const PROTOCOL_ID: u64 = 0x5A53_0001;

/// Portée maximale pour casser ou poser un bloc, en mètres.
pub const REACH: f32 = 6.0;

/// Identifiant d'un joueur, attribué par le serveur (identifiant de connexion renet).
pub type PlayerId = u64;

/// Ce que le serveur sait d'un joueur, envoyé plusieurs fois par seconde.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlayerState {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
}

/// Messages du client vers le serveur.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ClientMessage {
    /// Premier message après connexion.
    Hello {
        name: String,
    },
    /// Position du joueur (canal non fiable, envoyé en continu).
    State(PlayerState),
    BreakBlock {
        pos: IVec3,
    },
    PlaceBlock {
        pos: IVec3,
        block: Block,
    },
}

/// Messages du serveur vers le client.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ServerMessage {
    /// Réponse à `Hello` : paramètres du monde et point d'apparition.
    Welcome {
        your_id: PlayerId,
        world: WorldConfig,
        spawn: Vec3,
    },
    /// Un paquet de blocs modifiés, envoyé à l'arrivée pour rattraper l'état du monde.
    WorldEdits(Vec<(IVec3, Block)>),
    BlockChanged {
        pos: IVec3,
        block: Block,
    },
    PlayerJoined {
        id: PlayerId,
        name: String,
    },
    PlayerLeft {
        id: PlayerId,
    },
    /// État de tous les joueurs (canal non fiable).
    Players(Vec<(PlayerId, PlayerState)>),
    /// Le serveur replace le joueur (déplacement refusé, réapparition).
    Teleport {
        position: Vec3,
    },
    Chat {
        from: String,
        text: String,
    },
}

/// Sérialise un message.
pub fn encode<T: Serialize>(message: &T) -> Vec<u8> {
    postcard::to_allocvec(message).expect("la sérialisation d'un message ne peut pas échouer")
}

/// Désérialise un message ; `None` si les octets sont invalides (client modifié, version différente).
pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Option<T> {
    postcard::from_bytes(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_round_trip() {
        let msg = ServerMessage::WorldEdits(vec![(IVec3::new(1, -2, 3), Block::Stone)]);
        assert_eq!(decode::<ServerMessage>(&encode(&msg)), Some(msg));
        let msg = ClientMessage::State(PlayerState {
            position: Vec3::new(1.0, 2.0, 3.0),
            yaw: 0.5,
            pitch: -0.1,
        });
        assert_eq!(decode::<ClientMessage>(&encode(&msg)), Some(msg));
    }

    #[test]
    fn garbage_is_rejected() {
        assert_eq!(decode::<ClientMessage>(&[255, 255, 255]), None);
    }
}
