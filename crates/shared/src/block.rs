//! Types de blocs du monde voxel.

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
        !matches!(self, Block::Air | Block::DoorOpen | Block::BarbedWire)
    }

    /// Un bloc opaque est un cube plein : il cache les faces de ses voisins.
    pub const fn is_opaque(self) -> bool {
        !matches!(self, Block::Air | Block::DoorOpen | Block::BarbedWire)
    }

    /// Bloc qui garde des objets (ouvert avec le panneau du coffre).
    pub const fn is_storage(self) -> bool {
        matches!(self, Block::Chest | Block::Turret)
    }

    /// Bloc qu'un joueur peut poser depuis son inventaire.
    pub const fn is_placeable(self) -> bool {
        !matches!(self, Block::Air | Block::DoorOpen)
    }

    /// Un bloc visable par le joueur (pour casser ou utiliser), même s'il ne bloque pas.
    pub const fn is_targetable(self) -> bool {
        !matches!(self, Block::Air)
    }

    /// Forme d'un bloc qui n'est pas un cube plein : coin minimal et maximal dans le bloc.
    pub const fn partial_shape(self) -> Option<([f32; 3], [f32; 3])> {
        match self {
            // Porte ouverte : un battant fin plaqué contre un côté.
            Block::DoorOpen => Some(([0.0, 0.0, 0.0], [0.15, 1.0, 1.0])),
            // Barbelés : une haie basse.
            Block::BarbedWire => Some(([0.05, 0.0, 0.05], [0.95, 0.45, 0.95])),
            _ => None,
        }
    }

    /// Bloc qu'on peut ouvrir et fermer.
    pub const fn is_door(self) -> bool {
        matches!(self, Block::Door | Block::DoorOpen)
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
            Block::BarbedWire => [0.55, 0.55, 0.58],
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
            Block::BarbedWire => "Barbelés",
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
            Block::BarbedWire => 60,
        }
    }
}
