//! Nage : où en est le corps par rapport à l'eau, souffle et noyade.
//! Partagé entre le client (déplacement, interface) et le serveur (souffle qui fait foi).

use bevy_math::{IVec3, Vec3};

use crate::block::Block;
use crate::physics::PLAYER_EYE_HEIGHT;

/// Souffle d'un joueur sans compétence, en secondes sous l'eau.
pub const BREATH_SECS: f32 = 20.0;
/// Le souffle revient ce nombre de fois plus vite qu'il ne se perd.
pub const BREATH_RECOVERY: f32 = 4.0;
/// Vie perdue par seconde une fois le souffle épuisé.
pub const DROWN_DPS: f32 = 6.0;

/// Hauteur (depuis les pieds) où le nageur flotte : l'eau arrive sous le menton.
pub const FLOAT_HEIGHT: f32 = 1.3;

/// Position du corps par rapport à l'eau.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Immersion {
    /// Au sec.
    #[default]
    Dry,
    /// De l'eau aux chevilles ou aux genoux : on marche, un peu ralenti.
    Wading,
    /// Le corps est dans l'eau, la tête dehors : nage en surface.
    Surface,
    /// Les yeux sont sous l'eau : plongée, le souffle diminue.
    Under,
}

impl Immersion {
    /// Le joueur nage (en surface ou sous l'eau).
    pub fn swimming(self) -> bool {
        matches!(self, Immersion::Surface | Immersion::Under)
    }
}

/// Immersion d'un corps dont les pieds sont en `feet`.
pub fn immersion(feet: Vec3, block_at: impl Fn(IVec3) -> Block) -> Immersion {
    let wet = |y: f32| block_at((feet + Vec3::Y * y).floor().as_ivec3()).is_water();
    if wet(PLAYER_EYE_HEIGHT) {
        Immersion::Under
    } else if wet(0.9) {
        Immersion::Surface
    } else if wet(0.05) {
        Immersion::Wading
    } else {
        Immersion::Dry
    }
}

/// Hauteur de la surface de l'eau au-dessus de `feet` (le haut du dernier bloc d'eau de la colonne).
pub fn surface_above(feet: Vec3, block_at: impl Fn(IVec3) -> Block) -> Option<f32> {
    let mut p = feet.floor().as_ivec3();
    if !block_at(p).is_water() {
        return None;
    }
    for _ in 0..64 {
        if !block_at(p + IVec3::Y).is_water() {
            // Le bloc d'eau le plus haut est un peu abaissé à l'affichage.
            return Some(p.y as f32 + 0.9);
        }
        p += IVec3::Y;
    }
    None
}

/// Souffle après `dt` secondes : il baisse sous l'eau et remonte à l'air.
/// `capacity` est le souffle maximal (compétence Nageur comprise).
pub fn step_breath(breath: f32, under: bool, capacity: f32, dt: f32) -> f32 {
    if under {
        (breath - dt).max(0.0)
    } else {
        (breath + dt * BREATH_RECOVERY).min(capacity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un lac dont la surface est à y = 10 (eau de 5 à 9 inclus).
    fn lake(p: IVec3) -> Block {
        match p.y {
            ..5 => Block::Stone,
            5..=9 => Block::Water,
            _ => Block::Air,
        }
    }

    #[test]
    fn immersion_follows_the_depth() {
        assert_eq!(immersion(Vec3::new(0.5, 12.0, 0.5), lake), Immersion::Dry);
        assert_eq!(immersion(Vec3::new(0.5, 9.5, 0.5), lake), Immersion::Wading);
        assert_eq!(immersion(Vec3::new(0.5, 8.8, 0.5), lake), Immersion::Surface);
        assert_eq!(immersion(Vec3::new(0.5, 6.0, 0.5), lake), Immersion::Under);
        assert!(Immersion::Surface.swimming() && !Immersion::Wading.swimming());
    }

    #[test]
    fn the_surface_is_found_from_below() {
        assert_eq!(surface_above(Vec3::new(0.5, 5.5, 0.5), lake), Some(9.9));
        assert_eq!(surface_above(Vec3::new(0.5, 11.0, 0.5), lake), None);
    }

    #[test]
    fn breath_runs_out_and_comes_back_faster() {
        let mut b = BREATH_SECS;
        for _ in 0..25 {
            b = step_breath(b, true, BREATH_SECS, 1.0);
        }
        assert_eq!(b, 0.0);
        for _ in 0..5 {
            b = step_breath(b, false, BREATH_SECS, 1.0);
        }
        assert_eq!(b, BREATH_SECS);
    }
}
