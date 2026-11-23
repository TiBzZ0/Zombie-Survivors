//! Fin de partie par évacuation : une radio alimentée lance un appel de détresse, et un
//! hélicoptère se pose près d'elle au matin, deux nuits plus tard. Les joueurs à bord quand
//! il repart ont gagné (ils peuvent continuer à jouer ensuite).

use bevy_math::IVec3;
use serde::{Deserialize, Serialize};

/// Nuits à tenir entre l'appel et l'arrivée.
pub const RESCUE_NIGHTS: u32 = 2;
/// Moment de la journée où l'hélicoptère se pose (matin).
pub const ARRIVAL_PHASE: f32 = 0.08;
/// Secondes que l'hélicoptère attend au sol.
pub const WAIT_SECS: f32 = 60.0;
/// Distance (à l'horizontale) de la radio pour monter à bord.
pub const BOARD_RADIUS: f32 = 9.0;
/// Distance maximale du joueur à la radio pour lancer l'appel.
pub const CALL_REACH: f32 = 6.0;
/// L'appel attire les morts : zombies de la nuit multipliés d'autant tant qu'il court.
pub const SIGNAL_SPAWNS: f32 = 1.4;
/// Où l'hélicoptère se pose par rapport à la radio.
pub const PAD_OFFSET: IVec3 = IVec3::new(0, 0, 5);

/// Appel de détresse en cours.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RescueState {
    pub radio: IVec3,
    /// Jour d'arrivée (au matin).
    pub day: u32,
    /// Posé : secondes avant de repartir.
    pub landed: Option<f32>,
}

impl RescueState {
    pub fn new(radio: IVec3, today: u32) -> Self {
        Self {
            radio,
            day: today + RESCUE_NIGHTS,
            landed: None,
        }
    }

    /// L'heure d'arrivée est passée.
    pub fn due(&self, day: u32, phase: f32) -> bool {
        day > self.day || (day == self.day && phase >= ARRIVAL_PHASE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_helicopter_comes_after_two_nights() {
        let rescue = RescueState::new(IVec3::ZERO, 3);
        assert!(!rescue.due(3, 0.9));
        assert!(!rescue.due(4, 0.5));
        assert!(!rescue.due(5, 0.01));
        assert!(rescue.due(5, ARRIVAL_PHASE));
        assert!(rescue.due(6, 0.0));
    }
}
