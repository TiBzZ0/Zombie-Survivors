//! Progression du joueur : expérience gagnée en tuant des zombies, niveaux, points de
//! compétence, arbre de compétences en cinq branches et paliers technologiques.
//!
//! Tout est décrit par des tables (`SKILLS`, `TECHS`) : ajouter une compétence revient à
//! ajouter une variante, une ligne dans `info` et son effet dans `SkillEffects::with`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::block::Block;
use crate::item::Item;
use crate::zombie::ZombieKind;

/// Niveau maximal.
pub const MAX_LEVEL: u32 = 50;

/// Expérience pour passer du niveau `level` au suivant.
pub const fn xp_for_next(level: u32) -> u32 {
    60 + 40 * level + 4 * level * level
}

/// Expérience rapportée par un zombie tué.
pub const fn zombie_xp(kind: ZombieKind) -> u32 {
    match kind {
        ZombieKind::Walker => 10,
        ZombieKind::Runner => 12,
        ZombieKind::Spitter => 20,
        ZombieKind::Brute => 40,
        ZombieKind::Armored => 30,
        ZombieKind::Howler => 25,
        ZombieKind::Bloater => 18,
        ZombieKind::Colossus => 300,
    }
}

/// Expérience gagnée par chaque joueur en vie à l'aube.
pub const NIGHT_SURVIVED_XP: u32 = 60;

/// Branche de l'arbre.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Branch {
    Combat,
    Survival,
    Movement,
    Gathering,
    Technology,
}

pub const BRANCHES: [Branch; 5] = [
    Branch::Combat,
    Branch::Survival,
    Branch::Movement,
    Branch::Gathering,
    Branch::Technology,
];

impl Branch {
    pub const fn name(self) -> &'static str {
        match self {
            Branch::Combat => "Combat",
            Branch::Survival => "Survie",
            Branch::Movement => "Mouvement",
            Branch::Gathering => "Récolte",
            Branch::Technology => "Technologie",
        }
    }

    /// Couleur de la branche (RVB), pour l'interface.
    pub const fn color(self) -> [f32; 3] {
        match self {
            Branch::Combat => [0.85, 0.35, 0.3],
            Branch::Survival => [0.4, 0.8, 0.45],
            Branch::Movement => [0.4, 0.65, 0.95],
            Branch::Gathering => [0.85, 0.7, 0.35],
            Branch::Technology => [0.7, 0.55, 0.95],
        }
    }
}

/// Paliers de compétences : niveau de joueur requis pour le palier 1, 2, 3, 4.
pub const TIER_LEVEL: [u32; 4] = [1, 3, 6, 10];

/// Nom affiché d'un palier.
pub const fn tier_name(tier: u8) -> &'static str {
    match tier {
        1 => "I",
        2 => "II",
        3 => "III",
        _ => "IV",
    }
}

/// Une compétence de l'arbre.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Skill {
    // Combat
    Brawler,
    Marksman,
    SteadyGuard,
    Precision,
    GiantSlayer,
    Rage,
    // Survie
    Toughness,
    IronStomach,
    Sober,
    Recovery,
    Immunity,
    ThickSkin,
    SecondWind,
    // Mouvement
    Stamina,
    Swimmer,
    Leaper,
    Escapist,
    // Récolte
    Lumberjack,
    Miner,
    Forager,
    Hunter,
    Prospector,
    // Technologie
    Metallurgy,
    Mechanics,
    Gunpowder,
    Electricity,
    Thrifty,
    Gunner,
}

/// Fiche d'une compétence.
#[derive(Clone, Copy, Debug)]
pub struct SkillInfo {
    pub name: &'static str,
    pub branch: Branch,
    /// Palier (1 à 4) : il faut le niveau `TIER_LEVEL[tier - 1]`.
    pub tier: u8,
    pub max_rank: u8,
    /// Compétences (et rang) à avoir déjà.
    pub requires: &'static [(Skill, u8)],
    /// Effet d'un rang.
    pub effect: &'static str,
}

/// Toutes les compétences, dans l'ordre d'affichage.
pub const SKILLS: [Skill; 28] = [
    Skill::Brawler,
    Skill::Marksman,
    Skill::SteadyGuard,
    Skill::Precision,
    Skill::GiantSlayer,
    Skill::Rage,
    Skill::Toughness,
    Skill::IronStomach,
    Skill::Sober,
    Skill::Recovery,
    Skill::Immunity,
    Skill::ThickSkin,
    Skill::SecondWind,
    Skill::Stamina,
    Skill::Swimmer,
    Skill::Leaper,
    Skill::Escapist,
    Skill::Lumberjack,
    Skill::Miner,
    Skill::Forager,
    Skill::Hunter,
    Skill::Prospector,
    Skill::Metallurgy,
    Skill::Mechanics,
    Skill::Gunpowder,
    Skill::Electricity,
    Skill::Thrifty,
    Skill::Gunner,
];

impl Skill {
    pub const fn info(self) -> SkillInfo {
        const fn s(
            name: &'static str,
            branch: Branch,
            tier: u8,
            max_rank: u8,
            requires: &'static [(Skill, u8)],
            effect: &'static str,
        ) -> SkillInfo {
            SkillInfo {
                name,
                branch,
                tier,
                max_rank,
                requires,
                effect,
            }
        }
        use Branch::*;
        match self {
            Skill::Brawler => s("Bagarreur", Combat, 1, 5, &[], "+8 % de dégâts au corps-à-corps"),
            Skill::Marksman => s("Tireur", Combat, 1, 5, &[], "+8 % de dégâts à distance"),
            Skill::SteadyGuard => s(
                "Garde solide",
                Combat,
                2,
                3,
                &[(Skill::Brawler, 2)],
                "Garde levée : -15 % de dégâts reçus en plus, parade plus facile",
            ),
            Skill::Precision => s(
                "Coups précis",
                Combat,
                2,
                3,
                &[(Skill::Brawler, 1)],
                "+6 % de chances de coup critique (dégâts doublés)",
            ),
            Skill::GiantSlayer => s(
                "Tueur de géants",
                Combat,
                3,
                3,
                &[(Skill::Precision, 1)],
                "+15 % de dégâts contre les brutes et les colosses",
            ),
            Skill::Rage => s(
                "Rage",
                Combat,
                4,
                1,
                &[(Skill::GiantSlayer, 1)],
                "+35 % de dégâts au corps-à-corps sous 35 % de vie",
            ),
            Skill::Toughness => s("Robustesse", Survival, 1, 5, &[], "+10 points de vie maximum"),
            Skill::IronStomach => s("Estomac solide", Survival, 1, 3, &[], "La faim descend 10 % moins vite"),
            Skill::Sober => s("Sobriété", Survival, 1, 3, &[], "La soif monte 10 % moins vite"),
            Skill::Recovery => s(
                "Récupération",
                Survival,
                2,
                3,
                &[(Skill::Toughness, 2)],
                "+0,3 point de vie regagné par seconde",
            ),
            Skill::Immunity => s("Immunité", Survival, 2, 3, &[], "L'infection progresse 20 % moins vite"),
            Skill::ThickSkin => s(
                "Peau épaisse",
                Survival,
                3,
                3,
                &[(Skill::Toughness, 3)],
                "-5 % de dégâts reçus",
            ),
            Skill::SecondWind => s(
                "Second souffle",
                Survival,
                4,
                1,
                &[(Skill::ThickSkin, 1)],
                "Un coup mortel laisse à 25 points de vie (une fois toutes les 5 minutes)",
            ),
            Skill::Stamina => s("Endurance", Movement, 1, 3, &[], "+4 % de vitesse de course"),
            Skill::Swimmer => s(
                "Nageur",
                Movement,
                1,
                3,
                &[],
                "+15 % de vitesse dans l'eau, +25 % de souffle",
            ),
            Skill::Leaper => s(
                "Bondissant",
                Movement,
                2,
                2,
                &[(Skill::Stamina, 1)],
                "Sauts 8 % plus hauts",
            ),
            Skill::Escapist => s(
                "Fuyard",
                Movement,
                3,
                1,
                &[(Skill::Stamina, 2)],
                "+15 % de vitesse sous 35 % de vie",
            ),
            Skill::Lumberjack => s("Bûcheron", Gathering, 1, 3, &[], "Coupe le bois 15 % plus vite"),
            Skill::Miner => s(
                "Mineur",
                Gathering,
                1,
                3,
                &[],
                "Mine la pierre et le minerai 15 % plus vite",
            ),
            Skill::Forager => s(
                "Cueilleur",
                Gathering,
                2,
                3,
                &[],
                "+25 % de chances de trouver graines, herbes et baies",
            ),
            Skill::Hunter => s(
                "Chasseur",
                Gathering,
                2,
                3,
                &[],
                "33 % de chances d'un morceau de viande en plus sur le gibier",
            ),
            Skill::Prospector => s(
                "Prospecteur",
                Gathering,
                3,
                3,
                &[(Skill::Miner, 2)],
                "15 % de chances d'un minerai en plus sur les filons",
            ),
            Skill::Metallurgy => s(
                "Métallurgie",
                Technology,
                1,
                1,
                &[],
                "Débloque le palier technologique Métallurgie (forge, fer)",
            ),
            Skill::Mechanics => s(
                "Mécanique",
                Technology,
                2,
                1,
                &[(Skill::Metallurgy, 1)],
                "Débloque le palier Mécanique (tourelles, arbalète, catapulte...)",
            ),
            Skill::Gunpowder => s(
                "Poudre noire",
                Technology,
                3,
                1,
                &[(Skill::Mechanics, 1)],
                "Débloque le palier Poudre (fusil, explosifs, canon...)",
            ),
            Skill::Electricity => s(
                "Électricité",
                Technology,
                4,
                1,
                &[(Skill::Gunpowder, 1)],
                "Débloque le palier Électricité (générateur, projecteurs, bobines...)",
            ),
            Skill::Thrifty => s(
                "Économe",
                Technology,
                2,
                3,
                &[],
                "10 % de chances de récupérer chaque composant en fabriquant",
            ),
            Skill::Gunner => s(
                "Artilleur",
                Technology,
                3,
                3,
                &[(Skill::Mechanics, 1)],
                "+15 % de dégâts avec les armes fixes pilotées",
            ),
        }
    }
}

/// Palier technologique d'une recette.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Tech {
    Primitive,
    Metallurgy,
    Mechanics,
    Gunpowder,
    Electricity,
}

pub const TECHS: [Tech; 5] = [
    Tech::Primitive,
    Tech::Metallurgy,
    Tech::Mechanics,
    Tech::Gunpowder,
    Tech::Electricity,
];

impl Tech {
    pub const fn name(self) -> &'static str {
        match self {
            Tech::Primitive => "Primitif",
            Tech::Metallurgy => "Métallurgie",
            Tech::Mechanics => "Mécanique",
            Tech::Gunpowder => "Poudre",
            Tech::Electricity => "Électricité",
        }
    }

    /// Compétence qui débloque ce palier (aucune pour le palier primitif).
    pub const fn skill(self) -> Option<Skill> {
        match self {
            Tech::Primitive => None,
            Tech::Metallurgy => Some(Skill::Metallurgy),
            Tech::Mechanics => Some(Skill::Mechanics),
            Tech::Gunpowder => Some(Skill::Gunpowder),
            Tech::Electricity => Some(Skill::Electricity),
        }
    }

    /// Palier d'un objet fabriqué.
    pub fn of(item: Item) -> Tech {
        use crate::item::Tier;
        match item {
            Item::Block(block) => match block {
                Block::Generator | Block::Floodlight | Block::ElectricDoor | Block::TeslaCoil | Block::Radio => {
                    Tech::Electricity
                }
                Block::Cannon | Block::MachineGun | Block::Landmine => Tech::Gunpowder,
                Block::Turret | Block::Catapult => Tech::Mechanics,
                Block::Forge | Block::ReinforcedWall | Block::BarbedWire | Block::BearTrap => Tech::Metallurgy,
                _ => Tech::Primitive,
            },
            Item::Rifle | Item::Ammo | Item::Grenade | Item::Dynamite | Item::CannonBall => Tech::Gunpowder,
            Item::Crossbow | Item::Bolt | Item::Mace | Item::Bucket => Tech::Mechanics,
            Item::IronIngot
            | Item::SteelIngot
            | Item::Sword(Tier::Iron | Tier::Steel)
            | Item::Pickaxe(Tier::Iron | Tier::Steel)
            | Item::Spear(Tier::Iron | Tier::Steel)
            | Item::Axe(Tier::Iron | Tier::Steel)
            | Item::Helmet(Tier::Iron | Tier::Steel)
            | Item::BodyArmor(Tier::Iron | Tier::Steel) => Tech::Metallurgy,
            _ => Tech::Primitive,
        }
    }
}

/// Progression d'un joueur : expérience totale et rangs des compétences apprises.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    pub xp: u32,
    pub skills: BTreeMap<Skill, u8>,
}

/// Pourquoi une compétence ne peut pas être apprise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Locked {
    Maxed,
    NoPoints,
    Level(u32),
    Requires(Skill, u8),
}

impl Locked {
    pub fn describe(self) -> String {
        match self {
            Locked::Maxed => "Rang maximal atteint".into(),
            Locked::NoPoints => "Aucun point de compétence disponible".into(),
            Locked::Level(l) => format!("Niveau {l} requis"),
            Locked::Requires(skill, rank) => format!("Demande {} rang {rank}", skill.info().name),
        }
    }
}

impl Progress {
    /// Niveau atteint et expérience déjà gagnée dans ce niveau.
    pub fn level(&self) -> (u32, u32) {
        let mut level = 1;
        let mut left = self.xp;
        while level < MAX_LEVEL && left >= xp_for_next(level) {
            left -= xp_for_next(level);
            level += 1;
        }
        (level, left)
    }

    /// Ajoute de l'expérience ; renvoie le nombre de niveaux gagnés.
    pub fn add_xp(&mut self, xp: u32) -> u32 {
        let before = self.level().0;
        self.xp = self.xp.saturating_add(xp);
        self.level().0 - before
    }

    pub fn rank(&self, skill: Skill) -> u8 {
        self.skills.get(&skill).copied().unwrap_or(0)
    }

    pub fn spent(&self) -> u32 {
        self.skills.values().map(|r| *r as u32).sum()
    }

    /// Points de compétence encore à dépenser : un par niveau.
    pub fn points(&self) -> u32 {
        self.level().0.saturating_sub(self.spent())
    }

    pub fn can_learn(&self, skill: Skill) -> Result<(), Locked> {
        let info = skill.info();
        if self.rank(skill) >= info.max_rank {
            return Err(Locked::Maxed);
        }
        let needed = TIER_LEVEL[(info.tier as usize).clamp(1, 4) - 1];
        if self.level().0 < needed {
            return Err(Locked::Level(needed));
        }
        if let Some((s, r)) = info.requires.iter().find(|(s, r)| self.rank(*s) < *r) {
            return Err(Locked::Requires(*s, *r));
        }
        if self.points() == 0 {
            return Err(Locked::NoPoints);
        }
        Ok(())
    }

    /// Apprend un rang de la compétence si c'est permis.
    pub fn learn(&mut self, skill: Skill) -> bool {
        if self.can_learn(skill).is_err() {
            return false;
        }
        *self.skills.entry(skill).or_insert(0) += 1;
        true
    }

    /// Rend tous les points (l'expérience est gardée).
    pub fn reset(&mut self) {
        self.skills.clear();
    }

    pub fn has_tech(&self, tech: Tech) -> bool {
        tech.skill().is_none_or(|s| self.rank(s) > 0)
    }

    /// Somme des effets des compétences apprises.
    pub fn effects(&self) -> SkillEffects {
        self.skills
            .iter()
            .fold(SkillEffects::default(), |e, (s, r)| e.with(*s, *r as f32))
    }
}

/// Effets cumulés des compétences, appliqués par la simulation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkillEffects {
    pub melee: f32,
    pub ranged: f32,
    /// Part supplémentaire des dégâts absorbée par la garde levée.
    pub guard: f32,
    pub crit_chance: f32,
    /// Bonus de dégâts contre les brutes et les colosses.
    pub giant_slayer: f32,
    /// Bonus de dégâts au corps-à-corps quand la vie est basse.
    pub rage: f32,
    pub max_health: f32,
    pub hunger_rate: f32,
    pub thirst_rate: f32,
    pub regen: f32,
    pub infection_rate: f32,
    pub damage_taken: f32,
    pub second_wind: bool,
    pub speed: f32,
    pub swim_speed: f32,
    pub breath: f32,
    pub jump: f32,
    /// Bonus de vitesse quand la vie est basse.
    pub escape: f32,
    pub wood_mining: f32,
    pub stone_mining: f32,
    pub forage: f32,
    pub extra_meat: f32,
    pub extra_ore: f32,
    pub thrifty: f32,
    pub gunner: f32,
}

impl Default for SkillEffects {
    fn default() -> Self {
        Self {
            melee: 1.0,
            ranged: 1.0,
            guard: 0.0,
            crit_chance: 0.0,
            giant_slayer: 1.0,
            rage: 1.0,
            max_health: 100.0,
            hunger_rate: 1.0,
            thirst_rate: 1.0,
            regen: 0.0,
            infection_rate: 1.0,
            damage_taken: 1.0,
            second_wind: false,
            speed: 1.0,
            swim_speed: 1.0,
            breath: 1.0,
            jump: 1.0,
            escape: 1.0,
            wood_mining: 1.0,
            stone_mining: 1.0,
            forage: 1.0,
            extra_meat: 0.0,
            extra_ore: 0.0,
            thrifty: 0.0,
            gunner: 1.0,
        }
    }
}

/// Seuil de vie (part du maximum) sous lequel Rage et Fuyard s'activent.
pub const LOW_HEALTH: f32 = 0.35;

impl SkillEffects {
    fn with(mut self, skill: Skill, r: f32) -> Self {
        match skill {
            Skill::Brawler => self.melee += 0.08 * r,
            Skill::Marksman => self.ranged += 0.08 * r,
            Skill::SteadyGuard => self.guard += 0.15 * r,
            Skill::Precision => self.crit_chance += 0.06 * r,
            Skill::GiantSlayer => self.giant_slayer += 0.15 * r,
            Skill::Rage => self.rage += 0.35 * r,
            Skill::Toughness => self.max_health += 10.0 * r,
            Skill::IronStomach => self.hunger_rate -= 0.1 * r,
            Skill::Sober => self.thirst_rate -= 0.1 * r,
            Skill::Recovery => self.regen += 0.3 * r,
            Skill::Immunity => self.infection_rate -= 0.2 * r,
            Skill::ThickSkin => self.damage_taken -= 0.05 * r,
            Skill::SecondWind => self.second_wind = true,
            Skill::Stamina => self.speed += 0.04 * r,
            Skill::Swimmer => {
                self.swim_speed += 0.15 * r;
                self.breath += 0.25 * r;
            }
            Skill::Leaper => self.jump += 0.08 * r,
            Skill::Escapist => self.escape += 0.15 * r,
            Skill::Lumberjack => self.wood_mining += 0.15 * r,
            Skill::Miner => self.stone_mining += 0.15 * r,
            Skill::Forager => self.forage += 0.25 * r,
            Skill::Hunter => self.extra_meat += 0.33 * r,
            Skill::Prospector => self.extra_ore += 0.15 * r,
            Skill::Thrifty => self.thrifty += 0.1 * r,
            Skill::Gunner => self.gunner += 0.15 * r,
            Skill::Metallurgy | Skill::Mechanics | Skill::Gunpowder | Skill::Electricity => {}
        }
        self
    }

    /// Multiplicateur de vitesse de minage d'un bloc.
    pub fn mining(&self, block: Block) -> f32 {
        if block.is_log() {
            self.wood_mining
        } else if matches!(block, Block::Stone | Block::IronOre | Block::CoalOre | Block::Snow) {
            self.stone_mining
        } else {
            1.0
        }
    }

    /// Multiplicateur de vitesse de déplacement selon la vie restante.
    pub fn move_speed(&self, health_part: f32) -> f32 {
        self.speed * if health_part < LOW_HEALTH { self.escape } else { 1.0 }
    }

    /// Multiplicateur de dégâts au corps-à-corps (hors critique) contre ce zombie.
    pub fn melee_against(&self, kind: Option<ZombieKind>, health_part: f32) -> f32 {
        let giant = matches!(kind, Some(ZombieKind::Brute | ZombieKind::Colossus));
        self.melee
            * if giant { self.giant_slayer } else { 1.0 }
            * if health_part < LOW_HEALTH { self.rage } else { 1.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_follow_the_curve_and_give_one_point_each() {
        let mut p = Progress::default();
        assert_eq!(p.level(), (1, 0));
        assert_eq!(p.points(), 1);
        let gained = p.add_xp(xp_for_next(1) + xp_for_next(2) + 5);
        assert_eq!(gained, 2);
        assert_eq!(p.level(), (3, 5));
        assert_eq!(p.points(), 3);
        p.add_xp(u32::MAX);
        assert_eq!(p.level().0, MAX_LEVEL);
    }

    #[test]
    fn tiers_prerequisites_and_points_gate_learning() {
        let mut p = Progress::default();
        assert_eq!(p.can_learn(Skill::Mechanics), Err(Locked::Level(TIER_LEVEL[1])));
        assert!(p.learn(Skill::Metallurgy));
        assert!(p.has_tech(Tech::Metallurgy) && !p.has_tech(Tech::Mechanics));
        assert_eq!(p.can_learn(Skill::Brawler), Err(Locked::NoPoints));
        p.add_xp(10_000);
        assert!(p.learn(Skill::Mechanics));
        assert_eq!(
            p.can_learn(Skill::Electricity),
            Err(Locked::Requires(Skill::Gunpowder, 1))
        );
        assert_eq!(p.can_learn(Skill::Metallurgy), Err(Locked::Maxed));
        p.reset();
        assert_eq!(p.spent(), 0);
        assert!(!p.has_tech(Tech::Metallurgy));
    }

    #[test]
    fn effects_add_up_per_rank() {
        let mut p = Progress::default();
        p.add_xp(100_000);
        for _ in 0..3 {
            p.learn(Skill::Brawler);
            p.learn(Skill::Toughness);
        }
        let e = p.effects();
        assert!((e.melee - 1.24).abs() < 1e-5);
        assert_eq!(e.max_health, 130.0);
        assert_eq!(SkillEffects::default().mining(Block::Wood), 1.0);
    }

    #[test]
    fn the_table_is_consistent() {
        for skill in SKILLS {
            let info = skill.info();
            assert!((1..=4).contains(&info.tier) && info.max_rank > 0, "{}", info.name);
            for (req, rank) in info.requires {
                let r = req.info();
                assert!(
                    r.tier <= info.tier,
                    "{} demande une compétence d'un palier supérieur",
                    info.name
                );
                assert!(*rank <= r.max_rank);
            }
        }
        for tech in TECHS {
            assert!(tech.skill().is_none_or(|s| SKILLS.contains(&s)));
        }
        assert_eq!(Tech::of(Item::Rifle), Tech::Gunpowder);
        assert_eq!(Tech::of(Item::Block(Block::Workbench)), Tech::Primitive);
    }
}
