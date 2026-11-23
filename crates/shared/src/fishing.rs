//! Pêche : on lance la ligne dans l'eau, on attend que le bouchon plonge, on ferre à temps.
//! Le serveur tire l'attente et la prise ; le client montre le bouchon.

use crate::item::Item;

/// Distance maximale du lancer, depuis les yeux.
pub const CAST_RANGE: f32 = 14.0;
/// Au-delà de cette distance du bouchon, la ligne casse.
pub const LINE_LENGTH: f32 = 18.0;
/// Attente avant une touche, en secondes (tirée entre les deux).
pub const BITE_MIN: f32 = 5.0;
pub const BITE_MAX: f32 = 14.0;
/// Temps pour ferrer une fois que le bouchon plonge.
pub const BITE_WINDOW: f32 = 1.2;
/// Expérience d'une prise.
pub const FISH_XP: u32 = 4;

/// Ce que remonte la ligne, d'après deux tirages entre 0 et 1 : surtout du poisson,
/// parfois un débris, rarement un trésor tombé à l'eau.
pub fn catch(roll: f32, pick: f32) -> (Item, u32) {
    if roll < 0.75 {
        (Item::RawFish, 1 + (pick > 0.8) as u32)
    } else if roll < 0.9 {
        // Débris : branches, vieilles peaux, une gourde perdue.
        let junk = [(Item::Stick, 2), (Item::Hide, 1), (Item::Flask, 1)];
        junk[((pick * junk.len() as f32) as usize).min(junk.len() - 1)]
    } else {
        let treasure = [
            (Item::IronIngot, 2),
            (Item::Ammo, 6),
            (Item::Coal, 3),
            (Item::Antidote, 1),
        ];
        treasure[((pick * treasure.len() as f32) as usize).min(treasure.len() - 1)]
    }
}

/// Attente avant la prochaine touche, d'après un tirage entre 0 et 1.
pub fn wait(roll: f32) -> f32 {
    BITE_MIN + (BITE_MAX - BITE_MIN) * roll.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mostly_fish_sometimes_junk_rarely_treasure() {
        let mut fish = 0;
        let mut other = std::collections::BTreeSet::new();
        for i in 0..100 {
            for j in 0..10 {
                let (item, n) = catch(i as f32 / 100.0, j as f32 / 10.0);
                assert!(n >= 1);
                if item == Item::RawFish {
                    fish += 1;
                } else {
                    other.insert(format!("{item:?}"));
                }
            }
        }
        assert_eq!(fish, 750);
        assert!(other.len() >= 6, "{other:?}");
        assert_eq!(wait(0.0), BITE_MIN);
        assert_eq!(wait(2.0), BITE_MAX);
    }
}
