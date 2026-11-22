//! Survivants : humains que l'on trouve dans le monde et que l'on peut recruter.

use serde::{Deserialize, Serialize};

use crate::item::Item;
use crate::physics::{BodySize, PLAYER_SIZE};

/// Taille d'un survivant (celle d'un joueur).
pub const SURVIVOR_SIZE: BodySize = PLAYER_SIZE;
/// Vitesse de marche d'un survivant, en m/s.
pub const SURVIVOR_SPEED: f32 = 3.8;
/// Distance maximale pour parler à un survivant.
pub const TALK_RANGE: f32 = 4.0;
/// Ce qu'il faut donner pour recruter : une ration, ou à défaut des pommes.
pub const RECRUIT_COST: [(Item, u32); 2] = [(Item::Ration, 1), (Item::Apple, 3)];

/// Caractéristiques d'un survivant, de 1 à 5.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurvivorStats {
    /// Dégâts au combat.
    pub strength: u8,
    /// Vie maximale.
    pub endurance: u8,
    /// Vitesse de récolte et de réparation.
    pub skill: u8,
}

impl SurvivorStats {
    /// Tire des caractéristiques au hasard à partir d'un nombre aléatoire.
    pub fn from_seed(seed: u32) -> Self {
        let roll = |shift: u32| 1 + ((seed >> shift) % 5) as u8;
        Self {
            strength: roll(0),
            endurance: roll(8),
            skill: roll(16),
        }
    }

    pub fn damage(self) -> f32 {
        8.0 + self.strength as f32 * 4.0
    }

    pub fn max_health(self) -> f32 {
        60.0 + self.endurance as f32 * 15.0
    }

    /// Multiplicateur de vitesse de travail (récolte, réparation).
    pub fn work_speed(self) -> f32 {
        0.5 + self.skill as f32 * 0.25
    }

    pub fn describe(self) -> String {
        format!(
            "Force {} · Endurance {} · Habileté {}",
            self.strength, self.endurance, self.skill
        )
    }
}

/// Ce que fait un survivant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum Order {
    /// Pas encore recruté : il erre près de son camp.
    Idle,
    /// Suit son chef et se bat à ses côtés.
    Follow,
    /// Garde l'endroit où on l'a posté.
    Guard,
    /// Récolte du bois, de la pierre et de la nourriture autour de son poste (le jour).
    Gather,
    /// Répare les blocs abîmés autour de son poste.
    Repair,
}

impl Order {
    /// Ordre suivant quand le chef parle au survivant.
    pub fn next(self) -> Self {
        match self {
            Order::Idle | Order::Repair => Order::Follow,
            Order::Follow => Order::Guard,
            Order::Guard => Order::Gather,
            Order::Gather => Order::Repair,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Order::Idle => "sans chef",
            Order::Follow => "vous suit",
            Order::Guard => "monte la garde",
            Order::Gather => "récolte",
            Order::Repair => "répare les murs",
        }
    }

    /// Ce que dit le survivant quand il reçoit l'ordre.
    pub fn reply(self) -> &'static str {
        match self {
            Order::Idle => "Bon, je me débrouille seul.",
            Order::Follow => "Je te suis.",
            Order::Guard => "Je garde cet endroit.",
            Order::Gather => "Je vais chercher des ressources par ici. Pose un coffre près de moi.",
            Order::Repair => "Je m'occupe des murs abîmés autour d'ici.",
        }
    }

    pub const fn from_u8(value: u8) -> Self {
        match value {
            1 => Order::Follow,
            2 => Order::Guard,
            3 => Order::Gather,
            4 => Order::Repair,
            _ => Order::Idle,
        }
    }
}

const NAMES: &[&str] = &[
    "Marie", "Lucas", "Inès", "Hugo", "Camille", "Nora", "Théo", "Jade", "Malik", "Léa", "Sacha", "Yanis", "Chloé",
    "Bastien", "Amina", "Romain", "Zoé", "Karim", "Elsa", "Victor", "Lina", "Tom", "Manon", "Idriss",
];

/// Prénom d'un survivant à partir d'un nombre aléatoire.
pub fn name_from_seed(seed: u32) -> &'static str {
    NAMES[(seed as usize >> 3) % NAMES.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_stay_in_range() {
        for seed in [0, 1, 7, 12345, u32::MAX, 0xDEAD_BEEF] {
            let s = SurvivorStats::from_seed(seed);
            for v in [s.strength, s.endurance, s.skill] {
                assert!((1..=5).contains(&v));
            }
            assert!(s.max_health() >= 75.0 && s.damage() >= 12.0);
        }
    }

    #[test]
    fn orders_cycle_through_all_jobs() {
        let mut order = Order::Idle;
        let mut seen = Vec::new();
        for _ in 0..4 {
            order = order.next();
            seen.push(order);
            assert_eq!(Order::from_u8(order as u8), order);
        }
        assert_eq!(seen, [Order::Follow, Order::Guard, Order::Gather, Order::Repair]);
        assert_eq!(order.next(), Order::Follow);
    }
}
