//! Messages échangés entre client et serveur, sérialisés avec postcard.

use bevy_math::{IVec3, Vec3};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::block::Block;
use crate::config::WorldConfig;
use crate::daytime::GameTime;
use crate::item::{Inventory, Item};
use crate::zombie::ZombieKind;

/// Change à chaque modification incompatible du protocole.
pub const PROTOCOL_ID: u64 = 0x5A53_0005;

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
    /// Le joueur commence à miner ce bloc (le serveur vérifie la durée avant `BreakBlock`).
    StartMining {
        pos: IVec3,
    },
    BreakBlock {
        pos: IVec3,
    },
    /// Pose un bloc de l'inventaire.
    PlaceBlock {
        pos: IVec3,
        block: Block,
    },
    /// Coup porté à un zombie visé.
    Attack {
        zombie: u32,
    },
    /// Demande de réapparition après la mort.
    Respawn,
    /// Objet tenu en main (dégâts et vitesse de minage).
    Hold {
        item: Option<Item>,
    },
    /// Fabrique la recette `RECIPES[recipe]`.
    Craft {
        recipe: u16,
    },
    /// Tir avec l'arme à distance en main, dans la direction donnée.
    Shoot {
        direction: Vec3,
    },
    /// Ouvre ou ferme une porte (et les portes empilées avec elle).
    ToggleDoor {
        pos: IVec3,
    },
    /// Ouvre un coffre : le serveur répond avec son contenu et le tient à jour.
    OpenChest {
        pos: IVec3,
    },
    /// Ferme le coffre ouvert.
    CloseChest,
    /// Dépose des objets dans le coffre ouvert.
    ChestPut {
        item: Item,
        count: u32,
    },
    /// Prend des objets dans le coffre ouvert.
    ChestTake {
        item: Item,
        count: u32,
    },
    /// Message de discussion.
    Chat {
        text: String,
    },
    /// Mange ou utilise un consommable.
    Eat {
        item: Item,
    },
}

/// Position compacte d'un zombie (12 octets environ une fois sérialisée).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ZombieState {
    pub id: u32,
    /// Position en 1/32 de mètre.
    pub x: u16,
    pub y: u16,
    pub z: u16,
    /// Orientation sur 256 pas.
    pub yaw: u8,
    pub kind: u8,
    /// Bits d'état : 1 = attaque, 2 = vient d'être touché.
    pub flags: u8,
}

impl ZombieState {
    pub const ATTACKING: u8 = 1;
    pub const HURT: u8 = 2;

    pub fn new(id: u32, position: Vec3, yaw: f32, kind: ZombieKind, flags: u8) -> Self {
        let q = |v: f32| (v * 32.0).clamp(0.0, u16::MAX as f32) as u16;
        let turn = yaw.rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU;
        Self {
            id,
            x: q(position.x),
            y: q(position.y),
            z: q(position.z),
            yaw: (turn * 256.0) as u8,
            kind: kind as u8,
            flags,
        }
    }

    pub fn position(&self) -> Vec3 {
        Vec3::new(self.x as f32, self.y as f32, self.z as f32) / 32.0
    }

    pub fn yaw(&self) -> f32 {
        self.yaw as f32 / 256.0 * std::f32::consts::TAU
    }

    pub fn kind(&self) -> ZombieKind {
        ZombieKind::from_u8(self.kind)
    }
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
    /// Zombies proches du joueur (canal non fiable). Un zombie absent a disparu ou est trop loin.
    Zombies(Vec<ZombieState>),
    /// Heure de la partie et nombre de zombies en vie.
    Time {
        time: GameTime,
        zombies_alive: u32,
    },
    /// Santé et faim du joueur.
    Vitals {
        health: f32,
        hunger: f32,
    },
    /// Contenu complet de l'inventaire après un changement.
    Inventory(Inventory),
    /// Contenu d'un coffre ouvert, après chaque changement.
    Chest {
        pos: IVec3,
        contents: Inventory,
    },
    /// Le coffre ouvert n'est plus accessible (cassé, trop loin).
    ChestClosed,
    /// Un joueur a tiré : trajectoire pour l'effet visuel et le son.
    Shot {
        from: PlayerId,
        origin: Vec3,
        end: Vec3,
        rifle: bool,
    },
    /// Récoltes restantes d'un bloc de filon, après une récolte.
    Deposit {
        pos: IVec3,
        remaining: u8,
    },
    /// Le joueur est mort ; il peut demander à réapparaître.
    Died {
        cause: String,
    },
    /// Un zombie est mort (pour l'effet visuel et le compteur).
    ZombieKilled {
        id: u32,
        by: Option<PlayerId>,
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
    fn zombie_state_is_compact_and_precise() {
        let state = ZombieState::new(7, Vec3::new(512.3, 40.0, 100.9), 1.5, ZombieKind::Runner, 0);
        assert!(state.position().distance(Vec3::new(512.3, 40.0, 100.9)) < 0.05);
        assert!((state.yaw() - 1.5).abs() < 0.03);
        assert_eq!(state.kind(), ZombieKind::Runner);
        assert!(encode(&state).len() <= 14);
    }

    #[test]
    fn garbage_is_rejected() {
        assert_eq!(decode::<ClientMessage>(&[255, 255, 255]), None);
    }
}
