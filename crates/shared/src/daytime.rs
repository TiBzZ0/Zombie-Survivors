//! Cycle jour/nuit, identique côté client et serveur.

/// Durée d'un jour complet (jour + nuit), en secondes réelles.
pub const DAY_LENGTH_SECS: f32 = 16.0 * 60.0;
/// Part du cycle passée de jour (le reste est la nuit).
pub const DAYLIGHT_FRACTION: f32 = 0.62;
/// Une nuit sur `HORDE_EVERY` est une nuit de grande horde.
pub const HORDE_EVERY: u32 = 7;

/// Heure de la partie : nombre de jours écoulés et avancement dans la journée (0 à 1).
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GameTime {
    pub day: u32,
    /// 0 = aube, `DAYLIGHT_FRACTION` = crépuscule, 1 = aube suivante.
    pub phase: f32,
}

impl GameTime {
    /// Le premier jour commence un peu après l'aube.
    pub const START: GameTime = GameTime { day: 1, phase: 0.05 };

    pub fn advance(&mut self, secs: f32) {
        self.phase += secs / DAY_LENGTH_SECS;
        while self.phase >= 1.0 {
            self.phase -= 1.0;
            self.day += 1;
        }
    }

    pub fn is_night(&self) -> bool {
        self.phase >= DAYLIGHT_FRACTION
    }

    /// Vrai si la nuit en cours (ou la prochaine) est une nuit de grande horde.
    pub fn is_horde_night(&self) -> bool {
        self.day.is_multiple_of(HORDE_EVERY)
    }

    /// Secondes réelles avant la tombée de la nuit (0 s'il fait déjà nuit).
    pub fn secs_until_night(&self) -> f32 {
        ((DAYLIGHT_FRACTION - self.phase).max(0.0)) * DAY_LENGTH_SECS
    }

    /// Secondes réelles avant l'aube.
    pub fn secs_until_dawn(&self) -> f32 {
        (1.0 - self.phase) * DAY_LENGTH_SECS
    }

    /// Hauteur du soleil, de -1 (minuit) à 1 (midi). Positive le jour.
    pub fn sun_height(&self) -> f32 {
        if self.is_night() {
            let t = (self.phase - DAYLIGHT_FRACTION) / (1.0 - DAYLIGHT_FRACTION);
            -(t * std::f32::consts::PI).sin()
        } else {
            let t = self.phase / DAYLIGHT_FRACTION;
            (t * std::f32::consts::PI).sin()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_roll_over() {
        let mut time = GameTime { day: 1, phase: 0.9 };
        time.advance(DAY_LENGTH_SECS * 0.2);
        assert_eq!(time.day, 2);
        assert!((time.phase - 0.1).abs() < 1e-4);
    }

    #[test]
    fn night_and_horde() {
        let night = GameTime {
            day: 7,
            phase: DAYLIGHT_FRACTION + 0.01,
        };
        assert!(night.is_night());
        assert!(night.is_horde_night());
        assert!(night.sun_height() < 0.0);
        assert!(!GameTime::START.is_night());
        assert!(GameTime::START.sun_height() > 0.0);
    }
}
