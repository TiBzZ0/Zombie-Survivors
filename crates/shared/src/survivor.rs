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
    /// Fermier : récolte le blé mûr et le ressème, sème sur la terre nue autour de son poste.
    Farm,
    /// Cuisinier : cuit la viande et fait le pain au feu de camp près de son poste.
    Cook,
    /// Chasseur : traque le gibier autour de son poste le jour et rapporte viande et peaux.
    Hunt,
    /// Médecin : soigne les blessés autour de son poste et fait des bandages avec les herbes du coffre.
    Heal,
    /// Ingénieur : recharge les tourelles et le générateur proches avec le coffre du poste.
    Engineer,
}

impl Order {
    /// Ordre suivant quand le chef parle au survivant.
    pub fn next(self) -> Self {
        match self {
            Order::Idle | Order::Engineer => Order::Follow,
            Order::Follow => Order::Guard,
            Order::Guard => Order::Gather,
            Order::Gather => Order::Repair,
            Order::Repair => Order::Farm,
            Order::Farm => Order::Cook,
            Order::Cook => Order::Hunt,
            Order::Hunt => Order::Heal,
            Order::Heal => Order::Engineer,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Order::Idle => "sans chef",
            Order::Follow => "vous suit",
            Order::Guard => "monte la garde",
            Order::Gather => "récolte",
            Order::Repair => "répare les murs",
            Order::Farm => "cultive les champs",
            Order::Cook => "fait la cuisine",
            Order::Hunt => "chasse",
            Order::Heal => "soigne les blessés",
            Order::Engineer => "entretient les défenses",
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
            Order::Farm => "Je m'occupe du blé. Laisse des graines dans le coffre, je sèmerai sur la terre nue.",
            Order::Cook => "Il me faut un feu de camp près d'ici, et de la viande ou du blé dans le coffre.",
            Order::Hunt => "Je pars chasser le jour. Je rapporterai la viande et les peaux au coffre.",
            Order::Heal => {
                "Je soigne ceux qui passent près de moi. Laisse des herbes dans le coffre, j'en ferai des bandages."
            }
            Order::Engineer => "Je recharge les tourelles et le générateur d'ici avec ce qu'il y a dans le coffre.",
        }
    }

    /// Un travail fait autour d'un poste (le survivant y est posté).
    pub const fn is_job(self) -> bool {
        matches!(
            self,
            Order::Gather | Order::Repair | Order::Farm | Order::Cook | Order::Hunt | Order::Heal | Order::Engineer
        )
    }

    pub const fn from_u8(value: u8) -> Self {
        match value {
            1 => Order::Follow,
            2 => Order::Guard,
            3 => Order::Gather,
            4 => Order::Repair,
            5 => Order::Farm,
            6 => Order::Cook,
            7 => Order::Hunt,
            8 => Order::Heal,
            9 => Order::Engineer,
            _ => Order::Idle,
        }
    }
}

const NAMES: &[&str] = &[
    "Marie", "Lucas", "Inès", "Hugo", "Camille", "Nora", "Théo", "Jade", "Malik", "Léa", "Sacha", "Yanis", "Chloé",
    "Bastien", "Amina", "Romain", "Zoé", "Karim", "Elsa", "Victor", "Lina", "Tom", "Manon", "Idriss",
];

/// Marché proposé par un marchand : `give` exemplaires de `want` contre `get` exemplaires de `offer`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Offer {
    pub want: Item,
    pub give: u32,
    pub offer: Item,
    pub get: u32,
}

impl Offer {
    pub fn describe(self) -> String {
        format!(
            "{} {} contre {} {}",
            self.give,
            self.want.name().to_lowercase(),
            self.get,
            self.offer.name().to_lowercase()
        )
    }
}

/// Marchés possibles ; un marchand en propose trois au hasard.
pub const TRADES: [Offer; 8] = [
    Offer {
        want: Item::Hide,
        give: 4,
        offer: Item::Ammo,
        get: 12,
    },
    Offer {
        want: Item::Wheat,
        give: 6,
        offer: Item::Antidote,
        get: 1,
    },
    Offer {
        want: Item::IronIngot,
        give: 3,
        offer: Item::Grenade,
        get: 2,
    },
    Offer {
        want: Item::CookedMeat,
        give: 4,
        offer: Item::Bolt,
        get: 10,
    },
    Offer {
        want: Item::Coal,
        give: 6,
        offer: Item::IronIngot,
        get: 3,
    },
    Offer {
        want: Item::RawMeat,
        give: 6,
        offer: Item::Bandage,
        get: 3,
    },
    Offer {
        want: Item::Apple,
        give: 10,
        offer: Item::Seeds,
        get: 6,
    },
    Offer {
        want: Item::Bread,
        give: 3,
        offer: Item::Ammo,
        get: 8,
    },
];

/// Moral de départ d'un survivant.
pub const MORALE_START: f32 = 60.0;
/// Sous ce moral, le survivant traîne des pieds ; au-dessus de `MORALE_HIGH`, il se donne à fond.
pub const MORALE_LOW: f32 = 30.0;
pub const MORALE_HIGH: f32 = 70.0;
/// Sous ce moral trop longtemps, il abandonne son chef.
pub const MORALE_BREAKING: f32 = 10.0;

/// Ce qui pèse sur le moral d'un survivant recruté.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MoraleFactors {
    /// Il a de quoi manger.
    pub fed: bool,
    /// Il meurt de faim.
    pub starving: bool,
    /// Un feu de camp près de son poste (ou de lui).
    pub campfire: bool,
    /// Il a une arme.
    pub armed: bool,
    /// Il porte une armure.
    pub armored: bool,
    /// Il vient d'être blessé.
    pub hurt: bool,
    /// Nuit de grande horde.
    pub horde: bool,
    /// Compagnons tombés récemment près de lui (s'estompe avec le temps).
    pub grief: f32,
}

impl MoraleFactors {
    /// Moral vers lequel le survivant tend, de 0 à 100.
    pub fn target(self) -> f32 {
        let mut m: f32 = 50.0;
        m += if self.starving {
            -35.0
        } else if self.fed {
            15.0
        } else {
            -5.0
        };
        if self.campfire {
            m += 10.0;
        }
        if self.armed {
            m += 10.0;
        }
        if self.armored {
            m += 5.0;
        }
        if self.hurt {
            m -= 10.0;
        }
        if self.horde {
            m -= 10.0;
        }
        m -= self.grief;
        m.clamp(0.0, 100.0)
    }
}

/// Effet du moral sur la vitesse de travail et les dégâts.
pub fn morale_multiplier(morale: f32) -> f32 {
    if morale >= MORALE_HIGH {
        1.25
    } else if morale < MORALE_LOW {
        0.6
    } else {
        1.0
    }
}

/// Mot qui décrit le moral, pour l'interface.
pub fn morale_label(morale: f32) -> &'static str {
    if morale >= MORALE_HIGH {
        "moral au beau fixe"
    } else if morale >= MORALE_LOW {
        "moral correct"
    } else if morale >= MORALE_BREAKING {
        "moral en berne"
    } else {
        "sur le point de partir"
    }
}

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
        for _ in 0..9 {
            order = order.next();
            seen.push(order);
            assert_eq!(Order::from_u8(order as u8), order);
        }
        assert_eq!(
            seen,
            [
                Order::Follow,
                Order::Guard,
                Order::Gather,
                Order::Repair,
                Order::Farm,
                Order::Cook,
                Order::Hunt,
                Order::Heal,
                Order::Engineer
            ]
        );
        assert_eq!(order.next(), Order::Follow);
    }

    #[test]
    fn a_fed_armed_survivor_by_the_fire_is_happy_and_a_starving_one_breaks() {
        let happy = MoraleFactors {
            fed: true,
            campfire: true,
            armed: true,
            ..Default::default()
        };
        assert!(happy.target() >= MORALE_HIGH);
        let miserable = MoraleFactors {
            starving: true,
            hurt: true,
            grief: 20.0,
            ..Default::default()
        };
        assert!(miserable.target() < MORALE_BREAKING);
        assert!(morale_multiplier(happy.target()) > morale_multiplier(miserable.target()));
    }
}
