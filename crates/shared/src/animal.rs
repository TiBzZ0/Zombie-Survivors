//! Gibier : lapins, cerfs, sangliers, renards, ours et volailles, chassés pour leur viande.

use bevy_math::Vec3;
use serde::{Deserialize, Serialize};

use crate::physics::BodySize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum AnimalKind {
    /// Petit et rapide, une viande.
    Rabbit,
    /// Fuit vite et loin, deux ou trois viandes.
    Deer,
    /// Charge quand on l'attaque, trois viandes.
    Boar,
    /// Renard : rusé et rapide, fuit ; une viande et une belle peau.
    Fox,
    /// Ours : grand et dangereux, charge fort ; beaucoup de viande et deux peaux.
    Bear,
    /// Volaille : minuscule, commune dans les plaines ; une viande.
    Fowl,
}

/// Caractéristiques d'une espèce.
#[derive(Clone, Copy, Debug)]
pub struct AnimalStats {
    pub health: f32,
    /// Vitesse en promenade.
    pub speed: f32,
    /// Vitesse en fuite (ou en charge).
    pub run_speed: f32,
    /// Viandes données à la mort (au moins, au plus).
    pub meat: (u32, u32),
    /// Dégâts d'une charge (0 : l'animal ne se défend pas).
    pub damage: f32,
    /// Peaux données à la mort.
    pub hides: u32,
    pub size: BodySize,
}

impl AnimalKind {
    pub const ALL: [AnimalKind; 6] = [
        AnimalKind::Rabbit,
        AnimalKind::Deer,
        AnimalKind::Boar,
        AnimalKind::Fox,
        AnimalKind::Bear,
        AnimalKind::Fowl,
    ];

    pub const fn stats(self) -> AnimalStats {
        match self {
            AnimalKind::Rabbit => AnimalStats {
                health: 10.0,
                speed: 1.5,
                run_speed: 6.0,
                meat: (1, 1),
                damage: 0.0,
                hides: 1,
                size: BodySize {
                    half_width: 0.2,
                    height: 0.4,
                },
            },
            AnimalKind::Deer => AnimalStats {
                health: 40.0,
                speed: 1.6,
                run_speed: 7.0,
                meat: (2, 3),
                damage: 0.0,
                hides: 1,
                size: BodySize {
                    half_width: 0.4,
                    height: 1.4,
                },
            },
            AnimalKind::Boar => AnimalStats {
                health: 60.0,
                speed: 1.3,
                run_speed: 5.0,
                meat: (3, 3),
                damage: 9.0,
                hides: 1,
                size: BodySize {
                    half_width: 0.45,
                    height: 0.9,
                },
            },
            AnimalKind::Fox => AnimalStats {
                health: 18.0,
                speed: 1.8,
                run_speed: 7.5,
                meat: (1, 1),
                damage: 0.0,
                hides: 1,
                size: BodySize {
                    half_width: 0.25,
                    height: 0.5,
                },
            },
            AnimalKind::Bear => AnimalStats {
                health: 110.0,
                speed: 1.2,
                run_speed: 6.5,
                meat: (4, 5),
                damage: 18.0,
                hides: 2,
                size: BodySize {
                    half_width: 0.6,
                    height: 1.3,
                },
            },
            AnimalKind::Fowl => AnimalStats {
                health: 6.0,
                speed: 1.2,
                run_speed: 4.0,
                meat: (1, 1),
                damage: 0.0,
                hides: 0,
                size: BodySize {
                    half_width: 0.18,
                    height: 0.35,
                },
            },
        }
    }

    /// Couleurs du pelage et du détail (ventre, bois, défenses...), en RVB.
    pub const fn colors(self) -> ([f32; 3], [f32; 3]) {
        match self {
            AnimalKind::Rabbit => ([0.62, 0.55, 0.48], [0.92, 0.90, 0.86]),
            AnimalKind::Deer => ([0.55, 0.36, 0.20], [0.85, 0.78, 0.62]),
            AnimalKind::Boar => ([0.30, 0.24, 0.20], [0.88, 0.85, 0.75]),
            AnimalKind::Fox => ([0.82, 0.38, 0.12], [0.96, 0.94, 0.9]),
            AnimalKind::Bear => ([0.28, 0.20, 0.15], [0.5, 0.42, 0.35]),
            AnimalKind::Fowl => ([0.9, 0.88, 0.82], [0.9, 0.35, 0.2]),
        }
    }

    pub const fn from_u8(value: u8) -> Self {
        match value {
            1 => AnimalKind::Deer,
            2 => AnimalKind::Boar,
            3 => AnimalKind::Fox,
            4 => AnimalKind::Bear,
            5 => AnimalKind::Fowl,
            _ => AnimalKind::Rabbit,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            AnimalKind::Rabbit => "Lapin",
            AnimalKind::Deer => "Cerf",
            AnimalKind::Boar => "Sanglier",
            AnimalKind::Fox => "Renard",
            AnimalKind::Bear => "Ours",
            AnimalKind::Fowl => "Volaille",
        }
    }
}

/// Position compacte d'un animal, comme pour les zombies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnimalState {
    pub id: u32,
    /// Position en 1/32 de mètre.
    pub x: u16,
    pub y: u16,
    pub z: u16,
    /// Orientation sur 256 pas.
    pub yaw: u8,
    pub kind: u8,
    /// Bits d'état : 1 = court, 2 = vient d'être touché.
    pub flags: u8,
}

impl AnimalState {
    pub const RUNNING: u8 = 1;
    pub const HURT: u8 = 2;

    pub fn new(id: u32, position: Vec3, yaw: f32, kind: AnimalKind, flags: u8) -> Self {
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

    pub fn kind(&self) -> AnimalKind {
        AnimalKind::from_u8(self.kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn animal_state_round_trip() {
        let s = AnimalState::new(
            3,
            Vec3::new(10.5, 20.25, 30.0),
            1.0,
            AnimalKind::Boar,
            AnimalState::HURT,
        );
        assert!(s.position().distance(Vec3::new(10.5, 20.25, 30.0)) < 0.05);
        assert_eq!(s.kind(), AnimalKind::Boar);
        assert!((s.yaw() - 1.0).abs() < 0.03);
    }

    #[test]
    fn every_kind_round_trips_and_has_meat() {
        for kind in AnimalKind::ALL {
            assert_eq!(AnimalKind::from_u8(kind as u8), kind, "{}", kind.name());
            let stats = kind.stats();
            assert!(stats.meat.0 >= 1 && stats.meat.1 >= stats.meat.0);
            assert!(!kind.name().is_empty());
        }
        // L'ours est le plus coriace et le plus dangereux.
        assert!(AnimalKind::Bear.stats().damage > AnimalKind::Boar.stats().damage);
        assert!(AnimalKind::Bear.stats().hides >= 2);
    }
}
