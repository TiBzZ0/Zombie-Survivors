//! Types de zombies et leurs caractéristiques.

use serde::{Deserialize, Serialize};

use crate::physics::BodySize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum ZombieKind {
    /// Lent et nombreux.
    Walker,
    /// Rapide mais fragile.
    Runner,
    /// Lent, très résistant, défonce les murs.
    Brute,
}

/// Caractéristiques d'un type de zombie.
#[derive(Clone, Copy, Debug)]
pub struct ZombieStats {
    pub health: f32,
    pub speed: f32,
    /// Dégâts infligés à un joueur par coup.
    pub damage: f32,
    /// Dégâts infligés à un bloc par seconde.
    pub block_dps: f32,
    pub size: BodySize,
}

impl ZombieKind {
    pub const fn stats(self) -> ZombieStats {
        match self {
            ZombieKind::Walker => ZombieStats {
                health: 40.0,
                speed: 2.2,
                damage: 10.0,
                block_dps: 12.0,
                size: BodySize {
                    half_width: 0.3,
                    height: 1.8,
                },
            },
            ZombieKind::Runner => ZombieStats {
                health: 25.0,
                speed: 4.6,
                damage: 7.0,
                block_dps: 8.0,
                size: BodySize {
                    half_width: 0.28,
                    height: 1.7,
                },
            },
            ZombieKind::Brute => ZombieStats {
                health: 140.0,
                speed: 1.7,
                damage: 25.0,
                block_dps: 45.0,
                size: BodySize {
                    half_width: 0.45,
                    height: 2.2,
                },
            },
        }
    }

    pub const fn from_u8(value: u8) -> Self {
        match value {
            1 => ZombieKind::Runner,
            2 => ZombieKind::Brute,
            _ => ZombieKind::Walker,
        }
    }
}

/// Portée d'une attaque de zombie, en mètres (du centre du zombie au centre du joueur).
pub const ZOMBIE_REACH: f32 = 1.4;
/// Temps entre deux coups d'un zombie.
pub const ZOMBIE_ATTACK_COOLDOWN: f32 = 1.0;
