//! Blessures : saignements (morsures) et entorses (chutes). Un bandage soigne les deux.

/// Chance qu'une morsure qui traverse l'armure fasse saigner.
pub const BLEED_CHANCE: f32 = 0.22;
/// Durée d'un saignement et vie perdue par seconde.
pub const BLEED_SECS: f32 = 30.0;
pub const BLEED_DPS: f32 = 0.6;

/// Hauteur de chute sans dommage, en blocs.
pub const SAFE_FALL: f32 = 4.0;
/// Vie perdue par bloc de chute au-delà.
pub const FALL_DAMAGE_PER_BLOCK: f32 = 7.0;
/// À partir de cette hauteur, la chute foule la cheville.
pub const SPRAIN_FALL: f32 = 6.0;
/// Durée d'une entorse, et vitesse qu'il reste (pas de course).
pub const SPRAIN_SECS: f32 = 45.0;
pub const SPRAIN_SPEED: f32 = 0.6;
/// Plus haute chute prise en compte (le client l'annonce, le serveur la borne).
pub const MAX_FALL: f32 = 64.0;

/// Dégâts d'une chute de `height` blocs.
pub fn fall_damage(height: f32) -> f32 {
    (height.min(MAX_FALL) - SAFE_FALL).max(0.0) * FALL_DAMAGE_PER_BLOCK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_falls_are_harmless_and_big_ones_hurt() {
        assert_eq!(fall_damage(3.0), 0.0);
        assert_eq!(fall_damage(SAFE_FALL), 0.0);
        assert!(fall_damage(7.0) > 15.0);
        assert!(fall_damage(20.0) >= 100.0, "une chute de 20 blocs tue");
        assert_eq!(fall_damage(1000.0), fall_damage(MAX_FALL));
    }
}
