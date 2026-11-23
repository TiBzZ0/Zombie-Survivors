//! Pluie et orage : tirés au sort chaque jour par le serveur, montrés par le client.

use serde::{Deserialize, Serialize};

/// Ce qui tombe du ciel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Precipitation {
    #[default]
    Clear,
    Rain,
    /// Pluie battante, éclairs et tonnerre.
    Storm,
}

impl Precipitation {
    pub fn wet(self) -> bool {
        self != Precipitation::Clear
    }

    /// Degrés perdus par quelqu'un qui reste sous la pluie, sans toit.
    pub fn chill(self) -> f32 {
        match self {
            Precipitation::Clear => 0.0,
            Precipitation::Rain => 4.0,
            Precipitation::Storm => 7.0,
        }
    }

    /// Les cultures arrosées poussent plus vite.
    pub fn growth(self) -> f32 {
        if self.wet() { 1.8 } else { 1.0 }
    }
}

/// Chances, chaque jour (à partir du deuxième), de pluie, et qu'elle tourne à l'orage.
pub const RAIN_CHANCE: f32 = 0.35;
pub const STORM_CHANCE: f32 = 0.35;
/// Soif étanchée par seconde en restant sous la pluie, tête nue.
pub const RAIN_DRINK: f32 = 0.35;
/// Délai entre deux éclairs pendant un orage (tiré entre les deux), en secondes.
pub const LIGHTNING_MIN: f32 = 6.0;
pub const LIGHTNING_MAX: f32 = 18.0;
/// Éclair : rayon et dégâts.
pub const LIGHTNING_RADIUS: f32 = 3.0;
pub const LIGHTNING_DAMAGE: f32 = 80.0;
