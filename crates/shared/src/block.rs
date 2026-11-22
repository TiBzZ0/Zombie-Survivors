//! Types de blocs du monde voxel.

use serde::{Deserialize, Serialize};

/// Un bloc du monde. Tient sur un octet pour garder les chunks compacts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
}

impl Block {
    /// Un bloc solide bloque le déplacement et cache les faces voisines.
    pub const fn is_solid(self) -> bool {
        !matches!(self, Block::Air)
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
            Block::IronOre => "Minerai de fer",
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
            Block::IronOre => 200,
        }
    }
}
