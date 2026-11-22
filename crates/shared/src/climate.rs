//! Température et infection : règles partagées par le serveur (qui les applique) et
//! le client (qui les explique dans l'interface).
//!
//! La température ambiante dépend du biome, de la nuit, d'un feu de camp proche, d'un
//! toit au-dessus de la tête et de l'eau. Les vêtements la rapprochent du confort. La
//! température du corps suit lentement : sous 35 °C, hypothermie ; au-dessus de 39 °C,
//! coup de chaleur.

use crate::worldgen::Biome;

/// Température normale du corps, en °C.
pub const BODY_NORMAL: f32 = 37.0;
/// En dessous, le joueur perd de la vie (hypothermie).
pub const HYPOTHERMIA: f32 = 35.0;
/// Au-dessus, la soif se creuse deux fois plus vite.
pub const OVERHEATING: f32 = 38.5;
/// Au-dessus, le joueur perd de la vie (coup de chaleur).
pub const HEATSTROKE: f32 = 39.5;
/// Plage d'air ambiant où le corps revient à la normale.
pub const COMFORT: (f32, f32) = (8.0, 27.0);
/// Vie perdue par seconde en hypothermie ou en coup de chaleur.
pub const EXPOSURE_DPS: f32 = 1.2;

/// Chance qu'un coup de zombie qui traverse l'armure infecte le joueur.
pub const INFECTION_CHANCE: f32 = 0.12;
/// Progression de l'infection, en pour cent par seconde (de 1 à 100 en 12 minutes).
pub const INFECTION_PER_SEC: f32 = 100.0 / 720.0;
/// Au-delà, l'infection ronge la vie.
pub const INFECTION_HARMFUL: f32 = 60.0;
/// Vie perdue par seconde au-delà de `INFECTION_HARMFUL` (le double à 100 %).
pub const INFECTION_DPS: f32 = 0.4;

/// Ce qui entoure le joueur.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Surroundings {
    pub night: bool,
    /// Feu de camp à quelques pas.
    pub near_fire: bool,
    /// Un toit au-dessus de la tête.
    pub sheltered: bool,
    /// Dans l'eau.
    pub in_water: bool,
}

/// Température de l'air, en °C.
pub fn ambient(biome: Biome, around: Surroundings) -> f32 {
    let (day, night) = match biome {
        Biome::Snow => (-4.0, -16.0),
        Biome::Plains => (18.0, 9.0),
        Biome::Forest => (16.0, 8.0),
        Biome::Desert => (38.0, 4.0),
    };
    let mut t: f32 = if around.night { night } else { day };
    if around.sheltered {
        // Un abri coupe le vent la nuit et le soleil le jour.
        t += if t < 18.0 { 8.0 } else { -8.0 };
    }
    if around.in_water {
        t -= 10.0;
    }
    if around.near_fire {
        t = f32::max(t, 12.0) + 12.0;
    }
    t
}

/// Air ressenti avec les vêtements : `warmth` remonte le froid, `cooling` adoucit la chaleur,
/// sans jamais dépasser la plage de confort.
pub fn felt(ambient: f32, warmth: f32, cooling: f32) -> f32 {
    if ambient < COMFORT.0 {
        (ambient + warmth).min(COMFORT.0)
    } else if ambient > COMFORT.1 {
        (ambient - cooling).max(COMFORT.1)
    } else {
        ambient
    }
}

/// Fait évoluer la température du corps pendant `dt` secondes dans un air ressenti `felt`.
pub fn step_body(body: f32, felt: f32, dt: f32) -> f32 {
    if felt < COMFORT.0 {
        body - (COMFORT.0 - felt) * 0.0025 * dt
    } else if felt > COMFORT.1 {
        body + (felt - COMFORT.1) * 0.0025 * dt
    } else {
        // Retour vers la normale.
        let back = 0.05 * dt;
        if body < BODY_NORMAL {
            (body + back).min(BODY_NORMAL)
        } else {
            (body - back).max(BODY_NORMAL)
        }
    }
    .clamp(30.0, 43.0)
}

/// Mot qui décrit l'air ressenti, pour l'interface.
pub fn feeling(felt: f32) -> Option<&'static str> {
    if felt < COMFORT.0 - 10.0 {
        Some("Glacial")
    } else if felt < COMFORT.0 {
        Some("Froid")
    } else if felt > COMFORT.1 + 6.0 {
        Some("Brûlant")
    } else if felt > COMFORT.1 {
        Some("Chaud")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::Item;

    fn seconds_until(felt: f32, limit: impl Fn(f32) -> bool) -> f32 {
        let mut body = BODY_NORMAL;
        let mut t = 0.0;
        while !limit(body) && t < 3600.0 {
            body = step_body(body, felt, 0.5);
            t += 0.5;
        }
        t
    }

    #[test]
    fn snowy_night_is_deadly_without_protection_but_not_instantly() {
        let air = ambient(
            Biome::Snow,
            Surroundings {
                night: true,
                ..default()
            },
        );
        let t = seconds_until(felt(air, 0.0, 0.0), |b| b < HYPOTHERMIA);
        assert!((25.0..120.0).contains(&t), "hypothermie après {t} s");
    }

    #[test]
    fn a_coat_or_a_fire_keeps_you_warm() {
        let night = Surroundings {
            night: true,
            ..default()
        };
        let air = ambient(Biome::Snow, night);
        let with_coat = seconds_until(felt(air, Item::FurCoat.warmth(), 0.0), |b| b < HYPOTHERMIA);
        assert!(with_coat > 300.0, "{with_coat} s avec un manteau");
        let by_fire = ambient(
            Biome::Snow,
            Surroundings {
                near_fire: true,
                ..night
            },
        );
        assert!(by_fire >= COMFORT.0);
        let at_home = ambient(
            Biome::Plains,
            Surroundings {
                sheltered: true,
                ..night
            },
        );
        assert!(at_home >= COMFORT.0);
    }

    #[test]
    fn desert_day_overheats_and_a_hat_helps() {
        let air = ambient(Biome::Desert, Surroundings::default());
        let bare = seconds_until(felt(air, 0.0, 0.0), |b| b > HEATSTROKE);
        let hat = seconds_until(felt(air, 0.0, Item::StrawHat.cooling()), |b| b > HEATSTROKE);
        assert!(
            bare < 300.0 && hat > bare * 2.0,
            "{bare} s tête nue, {hat} s avec un chapeau"
        );
    }

    #[test]
    fn temperate_biomes_are_comfortable_by_day() {
        for biome in [Biome::Plains, Biome::Forest] {
            assert_eq!(feeling(ambient(biome, Surroundings::default())), None);
        }
    }

    #[test]
    fn body_returns_to_normal_in_comfort() {
        let mut body = 35.5;
        for _ in 0..200 {
            body = step_body(body, 18.0, 0.5);
        }
        assert_eq!(body, BODY_NORMAL);
    }

    fn default() -> Surroundings {
        Surroundings::default()
    }
}
