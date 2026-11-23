//! Statistiques de chaque joueur et succès qu'elles débloquent.
//!
//! Le serveur tient les compteurs ([`Record`]) et vérifie régulièrement la liste des
//! [`Achievement`] : un succès atteint est débloqué une fois pour toutes et rapporte de
//! l'expérience. Pour en ajouter un, une variante à la fin de l'énumération et une ligne
//! dans [`Achievement::info`] suffisent (les numéros enregistrés ne bougent pas).

use serde::{Deserialize, Serialize};

use crate::zombie::ZombieKind;

/// Compteurs d'un joueur, gardés dans la sauvegarde.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    /// Zombies tués, par type (indice = numéro du type).
    pub kills: Vec<u32>,
    /// Aubes vues vivant.
    pub nights: u32,
    /// Aubes vues vivant depuis la dernière mort.
    pub streak: u32,
    /// Lunes de sang traversées vivant.
    pub blood_moons: u32,
    pub deaths: u32,
    pub crafted: u32,
    pub placed: u32,
    pub mined: u32,
    pub fish: u32,
    pub recruited: u32,
    pub animals: u32,
    /// Dégâts infligés aux zombies.
    pub damage: u32,
    /// Évacuations réussies.
    pub evacuations: u32,
    /// Succès débloqués.
    pub unlocked: Vec<Achievement>,
}

impl Record {
    pub fn total_kills(&self) -> u32 {
        self.kills.iter().sum()
    }

    pub fn kills_of(&self, kind: ZombieKind) -> u32 {
        self.kills.get(kind as usize).copied().unwrap_or(0)
    }

    pub fn add_kill(&mut self, kind: ZombieKind) {
        let i = kind as usize;
        if self.kills.len() <= i {
            self.kills.resize(i + 1, 0);
        }
        self.kills[i] += 1;
    }

    pub fn has(&self, achievement: Achievement) -> bool {
        self.unlocked.contains(&achievement)
    }

    /// Succès atteints mais pas encore débloqués.
    pub fn newly_earned(&self) -> Vec<Achievement> {
        Achievement::ALL
            .iter()
            .copied()
            .filter(|a| !self.has(*a) && a.earned(self))
            .collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Achievement {
    FirstBlood,
    Slayer,
    Exterminator,
    Scourge,
    FirstNight,
    OneWeek,
    Veteran,
    Unbreakable,
    BloodMoon,
    CanOpener,
    GiantKiller,
    Silencer,
    Craftsman,
    Inventor,
    Builder,
    Architect,
    Miner,
    Angler,
    Leader,
    Warlord,
    Hunter,
    Butcher,
    Phoenix,
    Rescued,
}

/// Comment un succès s'affiche et ce qu'il rapporte.
pub struct AchievementInfo {
    pub title: &'static str,
    pub description: &'static str,
    pub xp: u32,
}

impl Achievement {
    pub const ALL: [Achievement; 24] = [
        Achievement::FirstBlood,
        Achievement::Slayer,
        Achievement::Exterminator,
        Achievement::Scourge,
        Achievement::FirstNight,
        Achievement::OneWeek,
        Achievement::Veteran,
        Achievement::Unbreakable,
        Achievement::BloodMoon,
        Achievement::CanOpener,
        Achievement::GiantKiller,
        Achievement::Silencer,
        Achievement::Butcher,
        Achievement::Craftsman,
        Achievement::Inventor,
        Achievement::Builder,
        Achievement::Architect,
        Achievement::Miner,
        Achievement::Angler,
        Achievement::Hunter,
        Achievement::Leader,
        Achievement::Warlord,
        Achievement::Phoenix,
        Achievement::Rescued,
    ];

    pub fn info(self) -> AchievementInfo {
        let (title, description, xp) = match self {
            Achievement::FirstBlood => ("Premier sang", "Tuer un zombie", 5),
            Achievement::Slayer => ("Chasseur de morts", "Tuer 50 zombies", 20),
            Achievement::Exterminator => ("Exterminateur", "Tuer 250 zombies", 60),
            Achievement::Scourge => ("Fléau des morts", "Tuer 1 000 zombies", 150),
            Achievement::FirstNight => ("Première nuit", "Voir l'aube vivant", 10),
            Achievement::OneWeek => ("Une semaine", "Voir 7 aubes", 40),
            Achievement::Veteran => ("Vétéran", "Voir 20 aubes", 100),
            Achievement::Unbreakable => ("Increvable", "Voir 5 aubes d'affilée sans mourir", 60),
            Achievement::BloodMoon => ("Nuit rouge", "Survivre à une lune de sang", 40),
            Achievement::CanOpener => ("Ouvre-boîte", "Tuer 10 blindés", 30),
            Achievement::GiantKiller => ("Tueur de géant", "Abattre un colosse", 80),
            Achievement::Silencer => ("Silence !", "Tuer 5 hurleurs", 25),
            Achievement::Butcher => ("Boucher", "Infliger 10 000 points de dégâts", 50),
            Achievement::Craftsman => ("Artisan", "Fabriquer 25 objets", 10),
            Achievement::Inventor => ("Inventeur", "Fabriquer 150 objets", 40),
            Achievement::Builder => ("Bâtisseur", "Poser 200 blocs", 15),
            Achievement::Architect => ("Architecte", "Poser 1 500 blocs", 60),
            Achievement::Miner => ("Mineur", "Casser 500 blocs", 20),
            Achievement::Angler => ("Pêcheur", "Attraper 10 poissons", 20),
            Achievement::Hunter => ("Trappeur", "Tuer 15 animaux", 20),
            Achievement::Leader => ("Chef de camp", "Recruter 3 survivants", 30),
            Achievement::Warlord => ("Seigneur de guerre", "Recruter 10 survivants", 80),
            Achievement::Phoenix => ("Phénix", "Mourir 5 fois et revenir", 10),
            Achievement::Rescued => ("Évacué", "Monter dans l'hélicoptère de secours", 200),
        };
        AchievementInfo { title, description, xp }
    }

    /// Atteint d'après les compteurs.
    pub fn earned(self, r: &Record) -> bool {
        match self {
            Achievement::FirstBlood => r.total_kills() >= 1,
            Achievement::Slayer => r.total_kills() >= 50,
            Achievement::Exterminator => r.total_kills() >= 250,
            Achievement::Scourge => r.total_kills() >= 1000,
            Achievement::FirstNight => r.nights >= 1,
            Achievement::OneWeek => r.nights >= 7,
            Achievement::Veteran => r.nights >= 20,
            Achievement::Unbreakable => r.streak >= 5,
            Achievement::BloodMoon => r.blood_moons >= 1,
            Achievement::CanOpener => r.kills_of(ZombieKind::Armored) >= 10,
            Achievement::GiantKiller => r.kills_of(ZombieKind::Colossus) >= 1,
            Achievement::Silencer => r.kills_of(ZombieKind::Howler) >= 5,
            Achievement::Butcher => r.damage >= 10_000,
            Achievement::Craftsman => r.crafted >= 25,
            Achievement::Inventor => r.crafted >= 150,
            Achievement::Builder => r.placed >= 200,
            Achievement::Architect => r.placed >= 1500,
            Achievement::Miner => r.mined >= 500,
            Achievement::Angler => r.fish >= 10,
            Achievement::Hunter => r.animals >= 15,
            Achievement::Leader => r.recruited >= 3,
            Achievement::Warlord => r.recruited >= 10,
            Achievement::Phoenix => r.deaths >= 5,
            Achievement::Rescued => r.evacuations >= 1,
        }
    }

    /// Avancement vers le succès (valeur, objectif), pour la barre du carnet.
    pub fn progress(self, r: &Record) -> (u32, u32) {
        let goal = |value: u32, goal: u32| (value.min(goal), goal);
        match self {
            Achievement::FirstBlood => goal(r.total_kills(), 1),
            Achievement::Slayer => goal(r.total_kills(), 50),
            Achievement::Exterminator => goal(r.total_kills(), 250),
            Achievement::Scourge => goal(r.total_kills(), 1000),
            Achievement::FirstNight => goal(r.nights, 1),
            Achievement::OneWeek => goal(r.nights, 7),
            Achievement::Veteran => goal(r.nights, 20),
            Achievement::Unbreakable => goal(r.streak, 5),
            Achievement::BloodMoon => goal(r.blood_moons, 1),
            Achievement::CanOpener => goal(r.kills_of(ZombieKind::Armored), 10),
            Achievement::GiantKiller => goal(r.kills_of(ZombieKind::Colossus), 1),
            Achievement::Silencer => goal(r.kills_of(ZombieKind::Howler), 5),
            Achievement::Butcher => goal(r.damage, 10_000),
            Achievement::Craftsman => goal(r.crafted, 25),
            Achievement::Inventor => goal(r.crafted, 150),
            Achievement::Builder => goal(r.placed, 200),
            Achievement::Architect => goal(r.placed, 1500),
            Achievement::Miner => goal(r.mined, 500),
            Achievement::Angler => goal(r.fish, 10),
            Achievement::Hunter => goal(r.animals, 15),
            Achievement::Leader => goal(r.recruited, 3),
            Achievement::Warlord => goal(r.recruited, 10),
            Achievement::Phoenix => goal(r.deaths, 5),
            Achievement::Rescued => goal(r.evacuations, 1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_achievement_is_listed_once() {
        for a in Achievement::ALL {
            assert_eq!(Achievement::ALL.iter().filter(|b| **b == a).count(), 1, "{a:?}");
            assert!(a.info().xp > 0);
            // Rien n'est atteint au départ, et l'avancement colle à la condition.
            let fresh = Record::default();
            assert!(!a.earned(&fresh));
            let (value, goal) = a.progress(&fresh);
            assert!(value < goal);
        }
    }

    #[test]
    fn counters_unlock_achievements() {
        let mut r = Record::default();
        r.add_kill(ZombieKind::Armored);
        assert_eq!(r.newly_earned(), vec![Achievement::FirstBlood]);
        r.unlocked.push(Achievement::FirstBlood);
        for _ in 0..9 {
            r.add_kill(ZombieKind::Armored);
        }
        assert_eq!(r.newly_earned(), vec![Achievement::CanOpener]);
        assert_eq!(Achievement::CanOpener.progress(&r), (10, 10));
    }
}
