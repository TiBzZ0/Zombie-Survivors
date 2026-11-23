//! Messages échangés entre client et serveur, sérialisés avec postcard.

use bevy_math::{IVec3, Vec3};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::block::Block;
use crate::config::WorldConfig;
use crate::daytime::GameTime;
use crate::item::{Inventory, Item};
use crate::survivor::{Order, SurvivorStats};
use crate::zombie::ZombieKind;

/// Change à chaque modification incompatible du protocole.
pub const PROTOCOL_ID: u64 = 0x5A53_001F;

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
    /// Le joueur frappe ou mine (pour animer son bras chez les autres).
    pub swinging: bool,
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
    /// Coup porté à un zombie visé, avec sa force (endurance : 0,5 à bout de souffle,
    /// 1 normal, 1,8 pour un coup puissant porté depuis la garde).
    Attack {
        zombie: u32,
        strength: f32,
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
    /// Se couche dans un sac de couchage : en fait le point de réapparition du joueur.
    Rest {
        pos: IVec3,
    },
    /// Dépense un point de compétence.
    LearnSkill {
        skill: crate::skills::Skill,
    },
    /// Rend tous les points de compétence pour les répartir autrement.
    ResetSkills,
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
    /// Boit à la source visée (de l'eau) ; remplit plutôt une gourde vide tenue en main.
    Drink {
        pos: IVec3,
    },
    /// Vide le seau d'eau tenu en main dans cette case : une source y naît.
    PourWater {
        pos: IVec3,
    },
    /// Parle à un survivant : le recrute, change son ordre, le renvoie (`dismiss`)
    /// ou lui donne l'arme tenue en main (`give`).
    TalkToSurvivor {
        id: u32,
        dismiss: bool,
        give: bool,
    },
    /// Lève ou baisse la garde (arme de mêlée en main, clic droit maintenu).
    Guard {
        active: bool,
    },
    /// Lance l'explosif tenu en main dans cette direction.
    Throw {
        direction: Vec3,
    },
    /// Tire avec l'arme fixe pilotée (mitrailleuse, canon, catapulte).
    FireEmplacement {
        pos: IVec3,
        direction: Vec3,
    },
    /// Frappe un animal au corps-à-corps.
    AttackAnimal {
        id: u32,
    },
    /// Frappe au corps à corps sur un autre joueur (JcJ seulement).
    AttackPlayer {
        id: PlayerId,
    },
    /// Cueillir les pommes mûres d'un pommier.
    Harvest {
        pos: IVec3,
    },
    /// Lance la ligne (canne à pêche en main) dans l'eau visée.
    Cast {
        at: IVec3,
    },
    /// Ferre si le bouchon plonge ; sinon, remonte la ligne.
    Reel,
}

/// Position compacte d'un survivant.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SurvivorState {
    pub id: u32,
    pub x: u16,
    pub y: u16,
    pub z: u16,
    pub yaw: u8,
    /// Vie restante sur 255.
    pub health: u8,
    /// Bits d'état : 1 = attaque, 2 = vient d'être touché, 4 = travaille.
    pub flags: u8,
    /// Moral, de 0 à 100.
    pub morale: u8,
}

impl SurvivorState {
    pub const ATTACKING: u8 = 1;
    pub const HURT: u8 = 2;
    pub const WORKING: u8 = 4;
    /// Le survivant a faim : il ne travaille plus et perd de la vie.
    pub const HUNGRY: u8 = 8;

    pub fn new(id: u32, position: Vec3, yaw: f32, health: f32, flags: u8, morale: f32) -> Self {
        let z = ZombieState::new(id, position, yaw, ZombieKind::Walker, 0, 0.0);
        Self {
            id,
            x: z.x,
            y: z.y,
            z: z.z,
            yaw: z.yaw,
            health: (health.clamp(0.0, 1.0) * 255.0).ceil() as u8,
            flags,
            morale: morale.clamp(0.0, 100.0).round() as u8,
        }
    }

    pub fn position(&self) -> Vec3 {
        Vec3::new(self.x as f32, self.y as f32, self.z as f32) / 32.0
    }

    pub fn yaw(&self) -> f32 {
        self.yaw as f32 / 256.0 * std::f32::consts::TAU
    }

    pub fn health_fraction(&self) -> f32 {
        self.health as f32 / 255.0
    }
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
    /// Vie restante sur 255.
    pub health: u8,
}

impl ZombieState {
    pub const ATTACKING: u8 = 1;
    pub const HURT: u8 = 2;
    /// Grimpe sur les autres zombies pour passer un mur.
    pub const CLIMBING: u8 = 4;
    /// Enragé par le cri d'un hurleur.
    pub const FRENZY: u8 = 8;
    /// Gonflé dont la mèche brûle : il va exploser.
    pub const FUSE: u8 = 16;

    pub fn new(id: u32, position: Vec3, yaw: f32, kind: ZombieKind, flags: u8, health: f32) -> Self {
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
            health: ((health / kind.stats().health).clamp(0.0, 1.0) * 255.0).ceil() as u8,
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

    /// Part de vie restante, de 0 à 1.
    pub fn health_fraction(&self) -> f32 {
        self.health as f32 / 255.0
    }
}

/// Messages du serveur vers le client.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ServerMessage {
    /// Réponse à `Hello` : paramètres du monde et point d'apparition.
    Welcome {
        your_id: PlayerId,
        /// Pseudo retenu par le serveur (nettoyé, éventuellement modifié).
        your_name: String,
        world: WorldConfig,
        spawn: Vec3,
        /// Les joueurs peuvent se blesser entre eux (bases protégées par les bannières).
        pvp: bool,
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
    /// État de tous les joueurs et objet qu'ils tiennent (canal non fiable).
    Players(Vec<(PlayerId, PlayerState, Option<Item>)>),
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
    /// Santé, faim, soif, température et infection du joueur.
    Vitals {
        health: f32,
        hunger: f32,
        thirst: f32,
        /// Température du corps, en °C.
        body_temp: f32,
        /// Air ressenti (avec les vêtements), en °C.
        felt: f32,
        /// Avancée de l'infection, de 0 (sain) à 100.
        infection: f32,
        /// Souffle restant sous l'eau, en secondes.
        breath: f32,
    },
    /// Effets temporaires actifs du joueur (café, bière...).
    Buffs(crate::buff::Buffs),
    /// Expérience et compétences du joueur.
    Progress(crate::skills::Progress),
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
    /// Dégâts subis par un bloc que des zombies attaquent (sur 255 ; 0 = réparé ou cassé).
    BlockDamage {
        pos: IVec3,
        damage: u8,
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
    /// Survivants proches du joueur (canal non fiable). Un survivant absent est mort ou trop loin.
    Survivors(Vec<SurvivorState>),
    /// Identité d'un survivant, envoyée à l'arrivée et à chaque changement de chef ou d'ordre.
    SurvivorInfo {
        id: u32,
        name: String,
        stats: SurvivorStats,
        leader: Option<String>,
        order: Order,
        /// Arme confiée par son chef.
        weapon: Option<Item>,
        /// Casque et armure confiés.
        armor: [Option<Item>; 2],
        /// Marchés proposés (vide sauf pour un marchand).
        offers: Vec<crate::survivor::Offer>,
    },
    /// Un zombie est mort (pour l'effet visuel et le compteur).
    ZombieKilled {
        id: u32,
        by: Option<PlayerId>,
    },
    /// Le joueur a paré un coup au bon moment.
    Parried,
    /// Un explosif a été lancé (le client simule sa trajectoire pour l'afficher).
    Thrown {
        origin: Vec3,
        velocity: Vec3,
        fuse: f32,
        item: Item,
    },
    /// Une explosion a eu lieu.
    Explosion {
        pos: Vec3,
        radius: f32,
    },
    /// Animaux proches du joueur (canal non fiable).
    Animals(Vec<crate::animal::AnimalState>),
    /// Un cracheur a lancé de l'acide.
    Acid {
        origin: Vec3,
        end: Vec3,
    },
    /// Temps qu'il fait : brouillard ou non (envoyé régulièrement).
    Weather {
        fog: bool,
    },
    /// Générateurs en marche (tous ceux du monde), envoyés régulièrement.
    Power(Vec<IVec3>),
    /// Arc électrique d'une bobine vers un zombie.
    Zap {
        origin: Vec3,
        end: Vec3,
    },
    /// Bases protégées : position de chaque bannière et pseudo de son propriétaire.
    Claims(Vec<(IVec3, String)>),
    /// Une construction s'est effondrée (pour la poussière et le grondement).
    Collapse {
        pos: Vec3,
        blocks: u16,
    },
    /// Un hurleur a crié : les zombies alentour sont enragés.
    Howl {
        pos: Vec3,
    },
    /// Le bouchon plonge : il faut ferrer tout de suite.
    Bite,
    /// La ligne est remontée, avec une prise (objet et nombre) ou non.
    Caught {
        item: Option<(Item, u32)>,
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
            swinging: true,
        });
        assert_eq!(decode::<ClientMessage>(&encode(&msg)), Some(msg));
    }

    #[test]
    fn zombie_state_is_compact_and_precise() {
        let state = ZombieState::new(7, Vec3::new(512.3, 40.0, 100.9), 1.5, ZombieKind::Runner, 0, 12.5);
        assert!(state.position().distance(Vec3::new(512.3, 40.0, 100.9)) < 0.05);
        assert!((state.yaw() - 1.5).abs() < 0.03);
        assert_eq!(state.kind(), ZombieKind::Runner);
        assert!((state.health_fraction() - 0.5).abs() < 0.01);
        assert!(encode(&state).len() <= 15);
    }

    #[test]
    fn garbage_is_rejected() {
        assert_eq!(decode::<ClientMessage>(&[255, 255, 255]), None);
    }
}
