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
    /// Crache de l'acide à distance.
    Spitter,
    /// Colosse : mène les grandes hordes, énorme et presque indestructible.
    Colossus,
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
            ZombieKind::Spitter => ZombieStats {
                health: 30.0,
                speed: 2.0,
                damage: 6.0,
                block_dps: 6.0,
                size: BodySize {
                    half_width: 0.3,
                    height: 1.7,
                },
            },
            ZombieKind::Colossus => ZombieStats {
                health: 1500.0,
                speed: 1.5,
                damage: 40.0,
                block_dps: 160.0,
                size: BodySize {
                    half_width: 0.8,
                    height: 3.4,
                },
            },
        }
    }

    pub const fn from_u8(value: u8) -> Self {
        match value {
            1 => ZombieKind::Runner,
            2 => ZombieKind::Brute,
            3 => ZombieKind::Spitter,
            4 => ZombieKind::Colossus,
            _ => ZombieKind::Walker,
        }
    }
}

/// Portée d'une attaque de zombie, en mètres (du centre du zombie au centre du joueur).
pub const ZOMBIE_REACH: f32 = 1.4;
/// Temps entre deux coups d'un zombie.
pub const ZOMBIE_ATTACK_COOLDOWN: f32 = 1.0;

/// Portée, cadence et dégâts du crachat d'acide.
pub const SPIT_RANGE: f32 = 14.0;
pub const SPIT_COOLDOWN: f32 = 2.5;
pub const SPIT_DAMAGE: f32 = 9.0;

/// À partir de cette nuit, les zombies arrêtés par un mur grimpent les uns sur les autres.
pub const STACK_FROM_NIGHT: u32 = 3;

/// Zombies qu'il faut serrer contre un mur pour qu'un d'eux grimpe sur les autres,
/// et hauteur qu'ils peuvent franchir ainsi (en blocs), selon la nuit.
pub const fn stacking(night: u32) -> Option<(usize, f32)> {
    if night < STACK_FROM_NIGHT {
        return None;
    }
    // Nuit 3 : 5 zombies passent un mur de 2 ; nuit 5 : 4 pour 3 blocs ; nuit 9 : 2 pour 5 blocs.
    let step = (night as usize).div_ceil(2);
    let needed = 7usize.saturating_sub(step);
    let needed = if needed < 2 { 2 } else { needed };
    let height = night.div_ceil(2);
    let height = if height > 6 { 6 } else { height };
    Some((needed, height as f32))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stacking_gets_easier_and_higher_with_nights() {
        assert_eq!(stacking(1), None);
        assert_eq!(stacking(2), None);
        // Un mur de 3 blocs tient jusqu'à la cinquième nuit.
        assert!(stacking(4).unwrap().1 < 3.0);
        let (n2, h2) = stacking(3).unwrap();
        let (n10, h10) = stacking(10).unwrap();
        assert!(n10 < n2 && h10 > h2);
        assert_eq!(stacking(40), Some((2, 6.0)));
    }
}
