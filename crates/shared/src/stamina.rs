//! Endurance : courir, nager vite, sauter et frapper la vident ; elle revient au repos.
//! Calculée par le client (comme les déplacements) ; le serveur borne la force des coups.

/// Endurance d'un joueur sans compétence.
pub const MAX_STAMINA: f32 = 100.0;
/// Coût par seconde de course ou de crawl.
pub const SPRINT_COST: f32 = 14.0;
/// Coût d'un saut.
pub const JUMP_COST: f32 = 8.0;
/// Coût d'un coup normal.
pub const ATTACK_COST: f32 = 6.0;
/// Coût d'un coup puissant (garde levée + clic gauche).
pub const POWER_COST: f32 = 25.0;
/// Récupération par seconde, une fois reposé.
pub const REGEN: f32 = 22.0;
/// Repos nécessaire avant que l'endurance remonte.
pub const REST_DELAY: f32 = 0.8;
/// À bout de souffle, il faut remonter jusque-là avant de courir à nouveau.
pub const RECOVERED: f32 = 30.0;

/// Force d'un coup : affaibli à bout de souffle, normal, ou puissant.
pub const TIRED_STRENGTH: f32 = 0.5;
pub const POWER_STRENGTH: f32 = 1.8;
/// Le coup puissant repousse beaucoup plus loin.
pub const POWER_KNOCKBACK: f32 = 2.5;

/// Endurance d'un joueur et son état.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stamina {
    pub value: f32,
    /// À bout de souffle : plus de course ni de coup puissant jusqu'à `RECOVERED`.
    pub exhausted: bool,
    /// Secondes depuis la dernière dépense.
    pub rest: f32,
}

impl Default for Stamina {
    fn default() -> Self {
        Self {
            value: MAX_STAMINA,
            exhausted: false,
            rest: 0.0,
        }
    }
}

impl Stamina {
    /// Dépense `amount` si possible (vrai) ; sinon vide la jauge et met à bout de souffle (faux).
    pub fn spend(&mut self, amount: f32) -> bool {
        self.rest = 0.0;
        if self.exhausted || self.value < amount {
            self.value = (self.value - amount).max(0.0);
            if self.value <= 0.0 {
                self.exhausted = true;
            }
            return false;
        }
        self.value -= amount;
        if self.value <= 0.0 {
            self.exhausted = true;
        }
        true
    }

    /// Peut-on dépenser `amount` (sans être à bout de souffle) ?
    pub fn can(&self, amount: f32) -> bool {
        !self.exhausted && self.value >= amount
    }

    /// Récupère avec le temps ; `max` compte les compétences, `rate` ralentit dans l'eau.
    pub fn recover(&mut self, dt: f32, max: f32, rate: f32) {
        self.rest += dt;
        if self.rest >= REST_DELAY {
            self.value = (self.value + REGEN * rate * dt).min(max);
        }
        if self.exhausted && self.value >= RECOVERED {
            self.exhausted = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn running_dry_exhausts_until_recovered() {
        let mut s = Stamina::default();
        while s.spend(SPRINT_COST) {}
        assert!(s.exhausted && !s.can(1.0));
        s.recover(REST_DELAY, MAX_STAMINA, 1.0);
        for _ in 0..10 {
            s.recover(0.1, MAX_STAMINA, 1.0);
        }
        assert!(s.value >= RECOVERED && !s.exhausted);
        assert!(s.can(POWER_COST) || s.value < POWER_COST);
    }
}
