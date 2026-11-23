//! Effets temporaires (« buffs ») accordés par certains consommables : un café rend vif,
//! une bière donne du courage, une tisane apaise, un ragoût tient au corps. Un même effet
//! est soit présent avec un temps restant, soit absent ; en boire à nouveau prolonge sa durée.

use serde::{Deserialize, Serialize};

/// Un effet temporaire.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Buff {
    /// Café : plus vif de ses jambes et de ses mains.
    Energized,
    /// Bière : frappe plus fort, mais un peu moins leste.
    Emboldened,
    /// Tisane : se soigne doucement et résiste à l'infection.
    Soothed,
    /// Ragoût : rassasié longtemps, la faim vient lentement.
    WellFed,
}

/// Tous les effets, dans l'ordre d'affichage.
pub const ALL_BUFFS: [Buff; 4] = [Buff::Energized, Buff::Emboldened, Buff::Soothed, Buff::WellFed];

impl Buff {
    pub const fn name(self) -> &'static str {
        match self {
            Buff::Energized => "Vif",
            Buff::Emboldened => "Courage",
            Buff::Soothed => "Apaisé",
            Buff::WellFed => "Repu",
        }
    }

    /// Ce qui l'a provoqué, pour l'infobulle.
    pub const fn source(self) -> &'static str {
        match self {
            Buff::Energized => "café",
            Buff::Emboldened => "bière",
            Buff::Soothed => "tisane",
            Buff::WellFed => "ragoût",
        }
    }

    /// Durée accordée quand on le reçoit, en secondes.
    pub const fn duration(self) -> f32 {
        match self {
            Buff::Energized => 90.0,
            Buff::Emboldened => 120.0,
            Buff::Soothed => 100.0,
            Buff::WellFed => 180.0,
        }
    }

    /// Couleur de sa pastille (RVB).
    pub const fn color(self) -> [f32; 3] {
        match self {
            Buff::Energized => [0.95, 0.75, 0.2],
            Buff::Emboldened => [0.85, 0.35, 0.2],
            Buff::Soothed => [0.4, 0.8, 0.55],
            Buff::WellFed => [0.8, 0.6, 0.35],
        }
    }
}

/// Somme des effets actifs, appliquée à la simulation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Effects {
    /// Multiplicateur de la vitesse de déplacement.
    pub speed: f32,
    /// Multiplicateur de la vitesse de minage.
    pub mining: f32,
    /// Multiplicateur des dégâts au corps-à-corps.
    pub melee: f32,
    /// Points de vie régénérés par seconde.
    pub regen: f32,
    /// Multiplicateur de la vitesse à laquelle la faim descend.
    pub hunger_rate: f32,
    /// Multiplicateur de la vitesse de progression de l'infection.
    pub infection_rate: f32,
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            speed: 1.0,
            mining: 1.0,
            melee: 1.0,
            regen: 0.0,
            hunger_rate: 1.0,
            infection_rate: 1.0,
        }
    }
}

impl Effects {
    fn with(mut self, buff: Buff) -> Self {
        match buff {
            Buff::Energized => {
                self.speed *= 1.22;
                self.mining *= 1.45;
            }
            Buff::Emboldened => {
                self.melee *= 1.4;
                self.speed *= 0.92;
            }
            Buff::Soothed => {
                self.regen += 1.5;
                self.infection_rate *= 0.3;
            }
            Buff::WellFed => {
                self.regen += 0.5;
                self.hunger_rate *= 0.5;
            }
        }
        self
    }
}

/// Effets actifs d'un joueur, avec le temps qu'il leur reste.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Buffs(pub Vec<(Buff, f32)>);

impl Buffs {
    /// Accorde un effet (ou prolonge sa durée s'il est déjà actif).
    pub fn grant(&mut self, buff: Buff) {
        let left = buff.duration();
        match self.0.iter_mut().find(|(b, _)| *b == buff) {
            Some((_, t)) => *t = t.max(left),
            None => self.0.push((buff, left)),
        }
    }

    /// Fait passer le temps ; renvoie vrai si la liste a changé (effet expiré).
    pub fn tick(&mut self, dt: f32) -> bool {
        let before = self.0.len();
        for (_, t) in &mut self.0 {
            *t -= dt;
        }
        self.0.retain(|(_, t)| *t > 0.0);
        self.0.len() != before
    }

    /// Somme des effets actifs.
    pub fn effects(&self) -> Effects {
        self.0.iter().fold(Effects::default(), |e, (b, _)| e.with(*b))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_buff_lasts_its_duration_then_fades() {
        let mut buffs = Buffs::default();
        buffs.grant(Buff::Energized);
        assert!(buffs.effects().speed > 1.0);
        assert!(!buffs.tick(Buff::Energized.duration() - 1.0));
        assert!(buffs.effects().mining > 1.0);
        assert!(buffs.tick(2.0), "l'effet a expiré");
        assert_eq!(buffs.effects(), Effects::default());
    }

    #[test]
    fn drinking_again_extends_the_timer() {
        let mut buffs = Buffs::default();
        buffs.grant(Buff::Soothed);
        buffs.tick(80.0);
        buffs.grant(Buff::Soothed);
        assert_eq!(buffs.0[0].1, Buff::Soothed.duration());
        assert_eq!(buffs.0.len(), 1, "un seul effet, prolongé");
    }

    #[test]
    fn effects_stack() {
        let mut buffs = Buffs::default();
        buffs.grant(Buff::Energized);
        buffs.grant(Buff::Emboldened);
        let e = buffs.effects();
        assert!(e.melee > 1.0 && e.mining > 1.0);
        // La vitesse combine +22 % puis -8 %.
        assert!((e.speed - 1.22 * 0.92).abs() < 1e-5);
    }
}
