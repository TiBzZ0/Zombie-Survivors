//! Types de blocs du monde voxel.

use bevy_math::IVec3;
use serde::{Deserialize, Serialize};

/// Un bloc du monde. Tient sur un octet pour garder les chunks compacts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Block {
    #[default]
    Air = 0,
    Grass,
    Dirt,
    Stone,
    Sand,
    Wood,
    Leaves,
    IronOre,
    /// Planches : se construit vite, plus solide que la terre.
    Planks,
    /// Mur renforcé au fer : très résistant.
    ReinforcedWall,
    /// Piques : blesse les zombies qui les frappent ou marchent dessus.
    Spikes,
    /// Filon de charbon : se récolte plusieurs fois avant de disparaître.
    CoalOre,
    /// Porte fermée : bloque comme un mur, s'ouvre au clic droit.
    Door,
    /// Porte ouverte : on passe à travers.
    DoorOpen,
    /// Coffre : stockage partagé entre les joueurs.
    Chest,
    /// Établi : outils en pierre, arc, portes, coffres.
    Workbench,
    /// Forge : tout ce qui demande du fer.
    Forge,
    /// Tourelle : tire seule sur les zombies avec les flèches ou cartouches rangées dedans.
    Turret,
    /// Barbelés : se traversent, mais ralentissent et blessent les zombies (et s'usent).
    BarbedWire,
    /// Mitrailleuse : se pilote, tire vite avec les cartouches du joueur.
    MachineGun,
    /// Canon : se pilote, tire des boulets qui explosent à l'impact.
    Cannon,
    /// Catapulte : se pilote, lance des pierres en cloche loin derrière les murs.
    Catapult,
    /// Pousse de blé, tout juste semée.
    Sprout,
    /// Blé en herbe.
    YoungWheat,
    /// Blé mûr, prêt à récolter.
    RipeWheat,
    /// Feu de camp : on y cuit la viande et le pain.
    Campfire,
    /// Eau des lacs : on la traverse à la nage, on s'y désaltère.
    Water,
    /// Neige qui couvre le sol des terres froides.
    Snow,
    /// Cactus du désert : il pique.
    Cactus,
    /// Eau qui s'écoule depuis une source (lac ou seau vidé) ; disparaît si la source tarit.
    FlowingWater,
    /// Générateur : brûle le charbon qu'on y range et alimente les appareils électriques proches.
    Generator,
    /// Projecteur : éclaire la nuit et brûle les zombies proches quand il est alimenté.
    Floodlight,
    /// Porte électrique blindée : s'ouvre seule devant les joueurs et les survivants si elle est alimentée.
    ElectricDoor,
    ElectricDoorOpen,
    /// Bobine électrique : foudroie le zombie le plus proche quand elle est alimentée.
    TeslaCoil,
    /// Caisse de ravitaillement larguée du ciel, pleine de matériel.
    SupplyCrate,
    /// Bannière : marque la base de son propriétaire, protégée des autres joueurs en JcJ.
    Banner,
    /// Herbes hautes : décor qui ondule au vent, traversable ; on les fauche pour des graines.
    TallGrass,
    /// Tronc de bouleau, blanc tacheté de noir.
    BirchWood,
    /// Feuillage clair du bouleau.
    BirchLeaves,
    /// Tronc de sapin, sombre.
    PineWood,
    /// Aiguilles de sapin.
    PineLeaves,
    /// Feuillage de pommier, sans fruits mûrs.
    FruitLeaves,
    /// Feuillage de pommier chargé de pommes mûres (clic droit pour cueillir).
    RipeFruitLeaves,
    /// Pomme tombée au pied de l'arbre : à ramasser avant qu'elle ne pourrisse.
    FallenFruit,
    /// Pomme pourrie, qui finit par disparaître.
    RottenFruit,
    /// Jeunes arbres à planter sur la terre ; ils deviennent des arbres.
    OakSapling,
    BirchSapling,
    PineSapling,
    AppleSapling,
    /// Jeune carotte, semée (première étape).
    CarrotSprout,
    /// Carotte en croissance.
    CarrotPlant,
    /// Carotte mûre, à récolter.
    RipeCarrot,
    /// Sac de couchage : posé au sol, il devient le point de réapparition de son dormeur.
    Bedroll,
    /// Case occupée par une grande machine (établi, tourelle...) posée à côté : on ne la voit
    /// pas, la machine la recouvre, et la casser casse toute la machine.
    Part,
    /// Mine : explose sous le premier zombie qui marche dessus.
    Landmine,
    /// Piège à mâchoires : happe et retient le zombie qui marche dessus (et s'use).
    BearTrap,
}

/// Rayon (à l'horizontale, sur toute la hauteur) de la base protégée par une bannière.
pub const CLAIM_RADIUS: f32 = 12.0;

/// La case `pos` est-elle dans la base de la bannière posée en `banner` ?
pub fn in_claim(banner: IVec3, pos: IVec3) -> bool {
    let d = (pos - banner).as_vec3();
    d.x * d.x + d.z * d.z <= CLAIM_RADIUS * CLAIM_RADIUS
}

/// Distance à laquelle un générateur en marche alimente les appareils électriques.
pub const POWER_RADIUS: f32 = 12.0;

/// Distance maximale entre un joueur et l'arme fixe qu'il pilote.
pub const EMPLACEMENT_REACH: f32 = 3.0;

/// Arme fixe qu'un joueur peut piloter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Emplacement {
    /// Munition prise dans l'inventaire du pilote.
    pub ammo: crate::item::Item,
    /// Délai entre deux tirs, en secondes.
    pub cooldown: f32,
    /// Vitesse du projectile (0 : tir instantané, comme un fusil).
    pub speed: f32,
    /// Dégâts d'un tir (au centre de l'explosion pour un projectile).
    pub damage: f32,
    /// Rayon de l'explosion à l'impact (0 : pas d'explosion).
    pub radius: f32,
    /// Part de la gravité subie par le projectile.
    pub gravity: f32,
}

/// Nombre de récoltes d'un bloc de filon avant qu'il ne disparaisse.
pub const DEPOSIT_HARVESTS: u8 = 20;

impl Block {
    /// Un filon se récolte `DEPOSIT_HARVESTS` fois : chaque récolte donne un objet,
    /// le bloc ne disparaît qu'à la dernière.
    pub const fn is_deposit(self) -> bool {
        matches!(self, Block::IronOre | Block::CoalOre)
    }

    /// Un bloc solide bloque le déplacement et cache les faces voisines.
    pub const fn is_solid(self) -> bool {
        !matches!(
            self,
            Block::Air
                | Block::DoorOpen
                | Block::ElectricDoorOpen
                | Block::BarbedWire
                | Block::Campfire
                | Block::Water
                | Block::FlowingWater
                | Block::TallGrass
                | Block::FallenFruit
                | Block::RottenFruit
                | Block::Bedroll
                | Block::Landmine
                | Block::BearTrap
        ) && !self.is_crop()
            && !self.is_sapling()
    }

    /// Eau, de source ou qui s'écoule.
    pub const fn is_water(self) -> bool {
        matches!(self, Block::Water | Block::FlowingWater)
    }

    /// Tronc d'arbre.
    pub const fn is_log(self) -> bool {
        matches!(self, Block::Wood | Block::BirchWood | Block::PineWood)
    }

    /// Feuillage (ajouré, il ne cache pas ce qui est derrière lui).
    pub const fn is_leaves(self) -> bool {
        matches!(
            self,
            Block::Leaves | Block::BirchLeaves | Block::PineLeaves | Block::FruitLeaves | Block::RipeFruitLeaves
        )
    }

    /// Jeune arbre planté.
    pub const fn is_sapling(self) -> bool {
        matches!(
            self,
            Block::OakSapling | Block::BirchSapling | Block::PineSapling | Block::AppleSapling
        )
    }

    /// Culture qui pousse sur la terre.
    pub const fn is_crop(self) -> bool {
        matches!(
            self,
            Block::Sprout
                | Block::YoungWheat
                | Block::RipeWheat
                | Block::CarrotSprout
                | Block::CarrotPlant
                | Block::RipeCarrot
        )
    }

    /// Culture arrivée à maturité, prête à récolter.
    pub const fn is_ripe_crop(self) -> bool {
        matches!(self, Block::RipeWheat | Block::RipeCarrot)
    }

    /// Un bloc opaque est un cube plein : il cache les faces de ses voisins.
    pub const fn is_opaque(self) -> bool {
        !matches!(
            self,
            Block::Air
                | Block::DoorOpen
                | Block::ElectricDoorOpen
                | Block::BarbedWire
                | Block::Campfire
                | Block::TallGrass
                | Block::FallenFruit
                | Block::RottenFruit
                | Block::Bedroll
                | Block::Landmine
                | Block::BearTrap
        ) && !self.is_crop()
            && !self.is_sapling()
    }

    /// Sac de couchage : point de réapparition personnel.
    pub const fn is_bed(self) -> bool {
        matches!(self, Block::Bedroll)
    }

    /// Bloc qui garde des objets (ouvert avec le panneau du coffre).
    pub const fn is_storage(self) -> bool {
        matches!(
            self,
            Block::Chest | Block::Turret | Block::Generator | Block::SupplyCrate
        )
    }

    /// Appareil qui ne marche qu'alimenté par un générateur proche.
    pub const fn is_electric(self) -> bool {
        matches!(
            self,
            Block::Floodlight | Block::ElectricDoor | Block::ElectricDoorOpen | Block::TeslaCoil
        )
    }

    /// Arme fixe pilotable, avec ses caractéristiques de tir.
    pub const fn emplacement(self) -> Option<Emplacement> {
        use crate::item::Item;
        match self {
            Block::MachineGun => Some(Emplacement {
                ammo: Item::Ammo,
                cooldown: 0.12,
                speed: 0.0,
                damage: 28.0,
                radius: 0.0,
                gravity: 0.0,
            }),
            Block::Cannon => Some(Emplacement {
                ammo: Item::CannonBall,
                cooldown: 2.0,
                speed: 38.0,
                damage: 110.0,
                radius: 3.5,
                gravity: 0.35,
            }),
            Block::Catapult => Some(Emplacement {
                ammo: Item::Block(Block::Stone),
                cooldown: 3.0,
                speed: 24.0,
                damage: 80.0,
                radius: 3.0,
                gravity: 1.0,
            }),
            _ => None,
        }
    }

    /// Bloc qu'un joueur peut poser depuis son inventaire.
    pub const fn is_placeable(self) -> bool {
        !matches!(
            self,
            Block::Air
                | Block::DoorOpen
                | Block::ElectricDoorOpen
                | Block::Water
                | Block::FlowingWater
                | Block::TallGrass
                | Block::RipeFruitLeaves
                | Block::FallenFruit
                | Block::RottenFruit
                | Block::Part
        )
    }

    /// Case où l'on peut poser un bloc : vide ou pleine d'eau.
    pub const fn is_replaceable(self) -> bool {
        matches!(self, Block::Air | Block::Water | Block::FlowingWater | Block::TallGrass)
    }

    /// Plante dessinée en croix (deux plans en diagonale) qui ondule au vent.
    pub const fn is_plant(self) -> bool {
        matches!(self, Block::TallGrass) || self.is_crop() || self.is_sapling()
    }

    /// Un bloc visable par le joueur (pour casser ou utiliser), même s'il ne bloque pas.
    pub const fn is_targetable(self) -> bool {
        !matches!(self, Block::Air | Block::Water | Block::FlowingWater)
    }

    /// Forme d'un bloc qui n'est pas un cube plein : coin minimal et maximal dans le bloc.
    pub const fn partial_shape(self) -> Option<([f32; 3], [f32; 3])> {
        match self {
            // Porte ouverte : un battant fin plaqué contre un côté.
            Block::DoorOpen | Block::ElectricDoorOpen => Some(([0.0, 0.0, 0.0], [0.15, 1.0, 1.0])),
            // Barbelés : une haie basse.
            Block::BarbedWire => Some(([0.05, 0.0, 0.05], [0.95, 0.45, 0.95])),
            Block::Sprout => Some(([0.2, 0.0, 0.2], [0.8, 0.25, 0.8])),
            Block::FallenFruit | Block::RottenFruit => Some(([0.35, 0.0, 0.35], [0.65, 0.3, 0.65])),
            Block::YoungWheat => Some(([0.12, 0.0, 0.12], [0.88, 0.55, 0.88])),
            Block::CarrotSprout => Some(([0.25, 0.0, 0.25], [0.75, 0.3, 0.75])),
            Block::CarrotPlant | Block::RipeCarrot => Some(([0.15, 0.0, 0.15], [0.85, 0.5, 0.85])),
            Block::RipeWheat => Some(([0.08, 0.0, 0.08], [0.92, 0.85, 0.92])),
            Block::Campfire => Some(([0.1, 0.0, 0.1], [0.9, 0.4, 0.9])),
            // Sac de couchage : un matelas fin posé à même le sol.
            Block::Bedroll => Some(([0.1, 0.0, 0.1], [0.9, 0.18, 0.9])),
            // Pièges : à ras du sol.
            Block::Landmine => Some(([0.25, 0.0, 0.25], [0.75, 0.12, 0.75])),
            Block::BearTrap => Some(([0.1, 0.0, 0.1], [0.9, 0.15, 0.9])),
            _ => None,
        }
    }

    /// Bloc qu'on peut ouvrir et fermer.
    pub const fn is_door(self) -> bool {
        matches!(
            self,
            Block::Door | Block::DoorOpen | Block::ElectricDoor | Block::ElectricDoorOpen
        )
    }

    /// Couleur unie (sRGB, 0 à 1) utilisée tant qu'il n'y a pas de textures.
    pub const fn color(self) -> [f32; 3] {
        match self {
            Block::Air => [0.0, 0.0, 0.0],
            Block::Grass => [0.36, 0.62, 0.27],
            Block::Dirt => [0.47, 0.33, 0.22],
            Block::Stone => [0.50, 0.50, 0.52],
            Block::Sand => [0.86, 0.80, 0.56],
            Block::Wood => [0.42, 0.30, 0.17],
            Block::Leaves => [0.22, 0.45, 0.18],
            Block::IronOre => [0.64, 0.52, 0.45],
            Block::Planks => [0.72, 0.56, 0.34],
            Block::ReinforcedWall => [0.36, 0.38, 0.42],
            Block::Spikes => [0.55, 0.20, 0.18],
            Block::CoalOre => [0.20, 0.20, 0.22],
            Block::Door | Block::DoorOpen => [0.55, 0.38, 0.20],
            Block::Chest => [0.62, 0.44, 0.18],
            Block::Workbench => [0.50, 0.34, 0.20],
            Block::Forge => [0.30, 0.28, 0.30],
            Block::Turret => [0.40, 0.42, 0.46],
            Block::Generator => [0.75, 0.62, 0.15],
            Block::Floodlight => [0.92, 0.92, 0.85],
            Block::ElectricDoor | Block::ElectricDoorOpen => [0.45, 0.50, 0.56],
            Block::TeslaCoil => [0.55, 0.40, 0.25],
            Block::SupplyCrate => [0.95, 0.50, 0.10],
            Block::Banner => [0.75, 0.12, 0.15],
            Block::BarbedWire => [0.55, 0.55, 0.58],
            Block::MachineGun => [0.30, 0.32, 0.30],
            Block::Cannon => [0.22, 0.22, 0.25],
            Block::Catapult => [0.55, 0.40, 0.25],
            Block::Sprout => [0.45, 0.75, 0.30],
            Block::YoungWheat => [0.50, 0.70, 0.25],
            Block::RipeWheat => [0.85, 0.72, 0.30],
            Block::Campfire => [0.80, 0.40, 0.15],
            Block::Water | Block::FlowingWater => [0.20, 0.40, 0.75],
            Block::Snow => [0.95, 0.97, 1.0],
            Block::Cactus => [0.25, 0.55, 0.25],
            Block::TallGrass => [0.42, 0.7, 0.3],
            Block::BirchWood => [0.88, 0.86, 0.8],
            Block::BirchLeaves => [0.42, 0.62, 0.25],
            Block::PineWood => [0.3, 0.2, 0.12],
            Block::PineLeaves => [0.12, 0.3, 0.16],
            Block::FruitLeaves | Block::RipeFruitLeaves => [0.26, 0.5, 0.2],
            Block::FallenFruit => [0.8, 0.15, 0.12],
            Block::RottenFruit => [0.35, 0.25, 0.12],
            Block::OakSapling | Block::AppleSapling => [0.3, 0.55, 0.22],
            Block::CarrotSprout => [0.4, 0.7, 0.3],
            Block::CarrotPlant => [0.45, 0.7, 0.3],
            Block::RipeCarrot => [0.5, 0.68, 0.28],
            Block::BirchSapling => [0.45, 0.65, 0.3],
            Block::PineSapling => [0.15, 0.38, 0.2],
            Block::Bedroll => [0.78, 0.36, 0.30],
            Block::Part => [0.45, 0.45, 0.45],
            Block::Landmine => [0.3, 0.32, 0.28],
            Block::BearTrap => [0.5, 0.48, 0.45],
        }
    }

    /// Nom affiché au joueur.
    pub const fn name(self) -> &'static str {
        match self {
            Block::Air => "Air",
            Block::Grass => "Herbe",
            Block::Dirt => "Terre",
            Block::Stone => "Pierre",
            Block::Sand => "Sable",
            Block::Wood => "Bois",
            Block::Leaves => "Feuilles",
            Block::IronOre => "Filon de fer",
            Block::Planks => "Planches",
            Block::ReinforcedWall => "Mur renforcé",
            Block::Spikes => "Piques",
            Block::CoalOre => "Filon de charbon",
            Block::Door | Block::DoorOpen => "Porte",
            Block::Chest => "Coffre",
            Block::Workbench => "Établi",
            Block::Forge => "Forge",
            Block::Turret => "Tourelle",
            Block::Generator => "Générateur",
            Block::Floodlight => "Projecteur",
            Block::ElectricDoor | Block::ElectricDoorOpen => "Porte électrique",
            Block::TeslaCoil => "Bobine électrique",
            Block::SupplyCrate => "Caisse de ravitaillement",
            Block::Banner => "Bannière",
            Block::BarbedWire => "Barbelés",
            Block::MachineGun => "Mitrailleuse",
            Block::Cannon => "Canon",
            Block::Catapult => "Catapulte",
            Block::Sprout => "Pousse de blé",
            Block::YoungWheat => "Blé en herbe",
            Block::RipeWheat => "Blé mûr",
            Block::Campfire => "Feu de camp",
            Block::Water => "Eau",
            Block::FlowingWater => "Eau courante",
            Block::Snow => "Neige",
            Block::Cactus => "Cactus",
            Block::TallGrass => "Herbes hautes",
            Block::BirchWood => "Bouleau",
            Block::BirchLeaves => "Feuilles de bouleau",
            Block::PineWood => "Sapin",
            Block::PineLeaves => "Aiguilles de sapin",
            Block::FruitLeaves => "Feuilles de pommier",
            Block::RipeFruitLeaves => "Pommier en fruits",
            Block::FallenFruit => "Pomme tombée",
            Block::RottenFruit => "Pomme pourrie",
            Block::OakSapling => "Pousse de chêne",
            Block::BirchSapling => "Pousse de bouleau",
            Block::PineSapling => "Pousse de sapin",
            Block::AppleSapling => "Pousse de pommier",
            Block::CarrotSprout => "Jeune carotte",
            Block::CarrotPlant => "Carotte",
            Block::RipeCarrot => "Carotte mûre",
            Block::Bedroll => "Sac de couchage",
            Block::Part => "Machine",
            Block::Landmine => "Mine",
            Block::BearTrap => "Piège à mâchoires",
        }
    }

    /// Points de vie du bloc, utilisés par la collecte et par les zombies qui cassent les murs.
    pub const fn max_health(self) -> u16 {
        match self {
            Block::Air => 0,
            Block::Leaves => 10,
            Block::Grass | Block::Dirt | Block::Sand => 30,
            Block::Wood => 80,
            Block::Stone => 150,
            Block::IronOre => 100,
            Block::CoalOre => 70,
            Block::Door | Block::DoorOpen => 200,
            Block::Chest => 80,
            Block::Workbench => 100,
            Block::Forge => 250,
            Block::Planks => 120,
            Block::ReinforcedWall => 600,
            Block::Spikes => 100,
            Block::Turret => 300,
            Block::Generator => 300,
            Block::Floodlight => 150,
            Block::ElectricDoor | Block::ElectricDoorOpen => 500,
            Block::TeslaCoil => 300,
            Block::SupplyCrate => 150,
            Block::Banner => 200,
            Block::BarbedWire => 60,
            Block::MachineGun => 320,
            Block::Cannon => 400,
            Block::Catapult => 260,
            Block::Sprout | Block::YoungWheat | Block::RipeWheat => 4,
            Block::CarrotSprout | Block::CarrotPlant | Block::RipeCarrot => 4,
            Block::TallGrass | Block::FallenFruit | Block::RottenFruit => 2,
            Block::BirchWood | Block::PineWood => 80,
            Block::BirchLeaves | Block::PineLeaves | Block::FruitLeaves | Block::RipeFruitLeaves => 10,
            Block::OakSapling | Block::BirchSapling | Block::PineSapling | Block::AppleSapling => 3,
            Block::Campfire => 60,
            Block::Water | Block::FlowingWater => 1,
            Block::Snow => 30,
            Block::Cactus => 30,
            Block::Bedroll => 8,
            Block::Part => 200,
            Block::Landmine => 10,
            Block::BearTrap => 60,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bedroll_is_a_placeable_mat_you_can_walk_over() {
        let bed = Block::Bedroll;
        assert!(bed.is_bed());
        assert!(bed.is_placeable(), "on peut le poser depuis l'inventaire");
        assert!(!bed.is_solid(), "on se tient dessus sans buter");
        assert!(bed.partial_shape().is_some(), "dessiné comme un matelas fin");
        assert!(!Block::Grass.is_bed());
    }
}
