//! Gibier : lapins, cerfs et sangliers, chassés pour leur viande.

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
    pub size: BodySize,
}

impl AnimalKind {
    pub const ALL: [AnimalKind; 3] = [AnimalKind::Rabbit, AnimalKind::Deer, AnimalKind::Boar];

    pub const fn stats(self) -> AnimalStats {
        match self {
            AnimalKind::Rabbit => AnimalStats {
                health: 10.0,
                speed: 1.5,
                run_speed: 6.0,
                meat: (1, 1),
                damage: 0.0,
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
                size: BodySize {
                    half_width: 0.45,
                    height: 0.9,
                },
            },
        }
    }

    pub const fn from_u8(value: u8) -> Self {
        match value {
            1 => AnimalKind::Deer,
            2 => AnimalKind::Boar,
            _ => AnimalKind::Rabbit,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            AnimalKind::Rabbit => "Lapin",
            AnimalKind::Deer => "Cerf",
            AnimalKind::Boar => "Sanglier",
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
}
