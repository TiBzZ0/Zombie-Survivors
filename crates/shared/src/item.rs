//! Objets : blocs à poser, outils, armes, nourriture. Inventaire.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::block::Block;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Item {
    Block(Block),
    Stick,
    IronIngot,
    /// Minerai de fer brut, récolté sur un filon.
    RawIron,
    Coal,
    Apple,
    Ration,
    Bandage,
    Sword(Tier),
    Pickaxe(Tier),
    Bow,
    Arrow,
    Rifle,
    Ammo,
    /// Lance : longue allonge, gros recul.
    Spear(Tier),
    /// Hache : frappe fort et coupe le bois vite.
    Axe(Tier),
    /// Masse d'armes : lente, mais balaie tous les zombies devant soi.
    Mace,
    Helmet(Tier),
    BodyArmor(Tier),
    /// Grenade : se lance et explose après quelques secondes, sans abîmer les murs.
    Grenade,
    /// Dynamite : explosion plus lente qui détruit aussi les blocs alentour.
    Dynamite,
    /// Arbalète : ses carreaux transpercent plusieurs zombies alignés.
    Crossbow,
    Bolt,
    /// Boulet de canon.
    CannonBall,
    /// Graines de blé : se sèment sur la terre ou l'herbe.
    Seeds,
    Wheat,
    Bread,
    RawMeat,
    CookedMeat,
    /// Gourde vide : se remplit à un lac.
    Flask,
    /// Gourde pleine : se boit n'importe où.
    WaterFlask,
    /// Peau d'un animal chassé.
    Hide,
    /// Manteau de fourrure : protège du froid.
    FurCoat,
    /// Chapeau de paille : protège de la chaleur.
    StrawHat,
    /// Antidote : guérit l'infection.
    Antidote,
    /// Seau vide : se remplit à l'eau.
    Bucket,
    /// Seau plein : se vide au clic droit et crée une source d'eau (douves, champs).
    WaterBucket,
    /// Baies cueillies sur le feuillage : se mangent telles quelles.
    Berries,
    /// Herbe médicinale, cueillie dans les herbes hautes : entre dans les remèdes et la tisane.
    Herb,
    /// Graines de carotte : se sèment comme le blé.
    CarrotSeeds,
    /// Carotte récoltée : nourrissante.
    Carrot,
    /// Café (baies torréfiées) : rend vif.
    Coffee,
    /// Bière (blé fermenté) : donne du courage.
    Beer,
    /// Tisane (herbes infusées) : apaise et soigne.
    HerbalTea,
    /// Ragoût (viande et carottes) : rassasie longtemps.
    Stew,
    /// Canne à pêche : clic droit vers l'eau pour lancer, puis pour ferrer.
    FishingRod,
    RawFish,
    CookedFish,
}

/// Caractéristiques d'un explosif qu'on lance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Throwable {
    /// Secondes avant l'explosion.
    pub fuse: f32,
    /// Rayon de l'explosion, en mètres.
    pub radius: f32,
    /// Dégâts au centre (moitié au bord).
    pub damage: f32,
    /// Rayon dans lequel les blocs sont détruits (0 : aucun).
    pub blast_blocks: f32,
}

/// Caractéristiques d'un coup au corps-à-corps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Melee {
    pub damage: f32,
    /// Allonge en mètres.
    pub reach: f32,
    /// Délai minimal entre deux coups, en secondes.
    pub cooldown: f32,
    /// Force du recul infligé.
    pub knockback: f32,
    /// Touche tous les zombies devant soi au lieu d'un seul.
    pub sweep: bool,
}

/// Emplacement d'une pièce d'armure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArmorSlot {
    Head,
    Body,
}

/// Caractéristiques d'une arme à distance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ranged {
    /// Munition consommée à chaque tir.
    pub ammo: Item,
    pub damage: f32,
    /// Portée en mètres.
    pub range: f32,
    /// Délai minimal entre deux tirs, en secondes.
    pub cooldown: f32,
    /// Nombre de zombies qu'un projectile peut traverser.
    pub pierce: u8,
}

/// Qualité d'un outil.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Tier {
    Wood,
    Stone,
    Iron,
}

impl Tier {
    const fn label(self) -> &'static str {
        match self {
            Tier::Wood => "en bois",
            Tier::Stone => "en pierre",
            Tier::Iron => "en fer",
        }
    }
}

impl Item {
    pub fn name(self) -> String {
        match self {
            Item::Block(b) => b.name().to_string(),
            Item::Stick => "Bâton".into(),
            Item::IronIngot => "Lingot de fer".into(),
            Item::RawIron => "Minerai de fer".into(),
            Item::Coal => "Charbon".into(),
            Item::Apple => "Pomme".into(),
            Item::Ration => "Ration".into(),
            Item::Bandage => "Bandage".into(),
            Item::Sword(t) => format!("Épée {}", t.label()),
            Item::Pickaxe(t) => format!("Pioche {}", t.label()),
            Item::Bow => "Arc".into(),
            Item::Arrow => "Flèche".into(),
            Item::Rifle => "Fusil".into(),
            Item::Ammo => "Cartouche".into(),
            Item::Spear(t) => format!("Lance {}", t.label()),
            Item::Axe(t) => format!("Hache {}", t.label()),
            Item::Mace => "Masse d'armes".into(),
            Item::Helmet(t) => format!("Casque {}", t.label()),
            Item::BodyArmor(t) => format!("Armure {}", t.label()),
            Item::Grenade => "Grenade".into(),
            Item::Dynamite => "Dynamite".into(),
            Item::Crossbow => "Arbalète".into(),
            Item::Bolt => "Carreau".into(),
            Item::CannonBall => "Boulet".into(),
            Item::Seeds => "Graines".into(),
            Item::Wheat => "Blé".into(),
            Item::Bread => "Pain".into(),
            Item::RawMeat => "Viande crue".into(),
            Item::CookedMeat => "Viande cuite".into(),
            Item::Flask => "Gourde vide".into(),
            Item::WaterFlask => "Gourde d'eau".into(),
            Item::Hide => "Peau".into(),
            Item::FurCoat => "Manteau de fourrure".into(),
            Item::StrawHat => "Chapeau de paille".into(),
            Item::Antidote => "Antidote".into(),
            Item::Bucket => "Seau".into(),
            Item::WaterBucket => "Seau d'eau".into(),
            Item::Berries => "Baies".into(),
            Item::Herb => "Herbe médicinale".into(),
            Item::CarrotSeeds => "Graines de carotte".into(),
            Item::Carrot => "Carotte".into(),
            Item::Coffee => "Café".into(),
            Item::Beer => "Bière".into(),
            Item::HerbalTea => "Tisane".into(),
            Item::Stew => "Ragoût".into(),
            Item::FishingRod => "Canne à pêche".into(),
            Item::RawFish => "Poisson cru".into(),
            Item::CookedFish => "Poisson grillé".into(),
        }
    }

    /// Ce que fait l'objet, en une ou deux lignes (infobulles).
    pub fn description(self) -> String {
        let mut lines: Vec<String> = Vec::new();
        if let Some(r) = self.ranged() {
            lines.push(format!(
                "Tir : {:.0} dégâts, portée {:.0} m (munition : {})",
                r.damage,
                r.range,
                r.ammo.name()
            ));
        } else if self.is_melee_weapon() || matches!(self, Item::Pickaxe(_)) {
            let m = Item::melee(Some(self));
            let sweep = if m.sweep { ", frappe en arc" } else { "" };
            lines.push(format!(
                "Corps-à-corps : {:.0} dégâts, allonge {:.1} m{sweep}",
                m.damage, m.reach
            ));
        }
        if let Some(t) = self.throwable() {
            let blocks = if t.blast_blocks > 0.0 {
                ", détruit les blocs"
            } else {
                ""
            };
            lines.push(format!(
                "Explosion : {:.0} dégâts sur {:.1} m{blocks}",
                t.damage, t.radius
            ));
        }
        if let Some((_, part)) = self.armor() {
            lines.push(format!("Armure : absorbe {:.0} % des dégâts", part * 100.0));
        }
        if let Some((hunger, health)) = self.food() {
            let mut parts = Vec::new();
            if hunger > 0.0 {
                parts.push(format!("+{hunger:.0} faim"));
            }
            if health > 0.0 {
                parts.push(format!("+{health:.0} vie"));
            }
            lines.push(parts.join(", "));
        }
        if let Some(thirst) = self.drink() {
            lines.push(format!("+{thirst:.0} soif"));
        }
        if let Some(buff) = self.buff() {
            lines.push(format!("Effet « {} » pendant {:.0} s", buff.name(), buff.duration()));
        }
        if self.warmth() > 0.0 {
            lines.push(format!("Protège du froid (+{:.0} °C)", self.warmth()));
        }
        if self.cooling() > 0.0 {
            lines.push(format!("Protège de la chaleur (−{:.0} °C)", self.cooling()));
        }
        if matches!(self, Item::Antidote) {
            lines.push("Guérit l'infection".into());
        }
        if let Item::Block(block) = self {
            lines.push(format!("Bloc à poser · solidité {}", block.max_health()));
        }
        lines.join("\n")
    }

    /// Coup porté avec cet objet en main (ou à mains nues).
    pub const fn melee(held: Option<Item>) -> Melee {
        const fn m(damage: f32, reach: f32, cooldown: f32, knockback: f32) -> Melee {
            Melee {
                damage,
                reach,
                cooldown,
                knockback,
                sweep: false,
            }
        }
        const fn tier(t: Tier, wood: f32, stone: f32, iron: f32) -> f32 {
            match t {
                Tier::Wood => wood,
                Tier::Stone => stone,
                Tier::Iron => iron,
            }
        }
        match held {
            Some(Item::Sword(t)) => m(tier(t, 18.0, 26.0, 38.0), 3.0, 0.4, 6.0),
            Some(Item::Spear(t)) => m(tier(t, 15.0, 22.0, 32.0), 4.5, 0.55, 9.0),
            Some(Item::Axe(t)) => m(tier(t, 22.0, 32.0, 46.0), 2.8, 0.7, 5.0),
            Some(Item::Mace) => Melee {
                damage: 50.0,
                reach: 3.0,
                cooldown: 1.0,
                knockback: 11.0,
                sweep: true,
            },
            Some(Item::Pickaxe(_)) => m(12.0, 3.0, 0.4, 5.0),
            _ => m(9.0, 3.0, 0.35, 6.0),
        }
    }

    /// Dégâts d'un coup porté avec cet objet en main.
    pub const fn melee_damage(held: Option<Item>) -> f32 {
        Self::melee(held).damage
    }

    /// Explosif qu'on lance au clic gauche.
    pub const fn throwable(self) -> Option<Throwable> {
        match self {
            Item::Grenade => Some(Throwable {
                fuse: 2.0,
                radius: 4.5,
                damage: 90.0,
                blast_blocks: 0.0,
            }),
            Item::Dynamite => Some(Throwable {
                fuse: 3.0,
                radius: 5.0,
                damage: 120.0,
                blast_blocks: 2.6,
            }),
            _ => None,
        }
    }

    /// Arme de corps-à-corps (qui permet aussi de parer).
    pub const fn is_melee_weapon(self) -> bool {
        matches!(self, Item::Sword(_) | Item::Spear(_) | Item::Axe(_) | Item::Mace)
    }

    /// Pièce d'armure : emplacement et part des dégâts absorbée.
    pub const fn armor(self) -> Option<(ArmorSlot, f32)> {
        match self {
            Item::Helmet(Tier::Wood) => Some((ArmorSlot::Head, 0.06)),
            Item::Helmet(Tier::Stone) => Some((ArmorSlot::Head, 0.1)),
            Item::Helmet(Tier::Iron) => Some((ArmorSlot::Head, 0.15)),
            Item::BodyArmor(Tier::Wood) => Some((ArmorSlot::Body, 0.12)),
            Item::BodyArmor(Tier::Stone) => Some((ArmorSlot::Body, 0.2)),
            Item::BodyArmor(Tier::Iron) => Some((ArmorSlot::Body, 0.3)),
            _ => None,
        }
    }

    /// Ce que rapporte un aliment : (faim, santé).
    pub const fn food(self) -> Option<(f32, f32)> {
        match self {
            Item::Apple => Some((20.0, 0.0)),
            Item::Ration => Some((55.0, 5.0)),
            Item::Bandage => Some((0.0, 35.0)),
            Item::Bread => Some((40.0, 0.0)),
            Item::RawMeat => Some((12.0, 0.0)),
            Item::CookedMeat => Some((60.0, 8.0)),
            Item::Berries => Some((12.0, 2.0)),
            Item::Carrot => Some((22.0, 0.0)),
            Item::Stew => Some((70.0, 10.0)),
            Item::RawFish => Some((10.0, 0.0)),
            Item::CookedFish => Some((48.0, 6.0)),
            _ => None,
        }
    }

    /// Effet temporaire accordé par ce consommable.
    pub const fn buff(self) -> Option<crate::buff::Buff> {
        match self {
            Item::Coffee => Some(crate::buff::Buff::Energized),
            Item::Beer => Some(crate::buff::Buff::Emboldened),
            Item::HerbalTea => Some(crate::buff::Buff::Soothed),
            Item::Stew => Some(crate::buff::Buff::WellFed),
            _ => None,
        }
    }

    /// Soif étanchée par une boisson.
    pub const fn drink(self) -> Option<f32> {
        match self {
            Item::WaterFlask => Some(60.0),
            _ => None,
        }
    }

    /// Degrés gagnés contre le froid quand on porte ce vêtement.
    pub const fn warmth(self) -> f32 {
        match self {
            Item::FurCoat => 22.0,
            _ => 0.0,
        }
    }

    /// Degrés gagnés contre la chaleur quand on porte ce vêtement.
    pub const fn cooling(self) -> f32 {
        match self {
            Item::StrawHat => 8.0,
            _ => 0.0,
        }
    }

    /// Objet qu'on consomme au clic droit (aliment, boisson ou remède).
    pub const fn is_consumable(self) -> bool {
        self.food().is_some() || self.drink().is_some() || self.buff().is_some() || matches!(self, Item::Antidote)
    }

    /// Arme qu'on peut confier à un survivant.
    pub const fn is_weapon(self) -> bool {
        self.is_melee_weapon() || matches!(self, Item::Bow | Item::Rifle | Item::Crossbow)
    }

    /// Objet qu'on peut confier à un survivant : arme ou armure.
    pub const fn is_giftable(self) -> bool {
        self.is_weapon() || self.armor().is_some()
    }

    pub const fn is_tool(self) -> bool {
        self.is_weapon() || matches!(self, Item::Pickaxe(_) | Item::FishingRod)
    }

    /// Caractéristiques de tir, si l'objet est une arme à distance.
    pub const fn ranged(self) -> Option<Ranged> {
        match self {
            Item::Bow => Some(Ranged {
                ammo: Item::Arrow,
                damage: 30.0,
                range: 40.0,
                cooldown: 0.8,
                pierce: 1,
            }),
            Item::Rifle => Some(Ranged {
                ammo: Item::Ammo,
                damage: 55.0,
                range: 80.0,
                cooldown: 0.45,
                pierce: 1,
            }),
            Item::Crossbow => Some(Ranged {
                ammo: Item::Bolt,
                damage: 45.0,
                range: 60.0,
                cooldown: 1.2,
                pierce: 4,
            }),
            _ => None,
        }
    }
}

/// Inventaire : nombre d'exemplaires de chaque objet. Les outils ne s'usent pas.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inventory {
    items: BTreeMap<Item, u32>,
}

impl Inventory {
    pub fn count(&self, item: Item) -> u32 {
        self.items.get(&item).copied().unwrap_or(0)
    }

    pub fn has(&self, item: Item, n: u32) -> bool {
        self.count(item) >= n
    }

    pub fn add(&mut self, item: Item, n: u32) {
        if n > 0 {
            *self.items.entry(item).or_insert(0) += n;
        }
    }

    /// Retire `n` exemplaires ; faux (et rien ne change) s'il n'y en a pas assez.
    pub fn remove(&mut self, item: Item, n: u32) -> bool {
        match self.items.get_mut(&item) {
            Some(c) if *c >= n => {
                *c -= n;
                if *c == 0 {
                    self.items.remove(&item);
                }
                true
            }
            _ => false,
        }
    }

    /// Objets possédés, dans un ordre stable (pour la barre d'objets).
    pub fn iter(&self) -> impl Iterator<Item = (Item, u32)> + '_ {
        self.items.iter().map(|(i, c)| (*i, *c))
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Meilleur casque et meilleure armure du sac : ils sont portés d'office.
    pub fn worn_armor(&self) -> (Option<Item>, Option<Item>) {
        let best = |slot: ArmorSlot| {
            self.items
                .keys()
                .filter_map(|i| i.armor().filter(|(s, _)| *s == slot).map(|(_, v)| (*i, v)))
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i)
        };
        (best(ArmorSlot::Head), best(ArmorSlot::Body))
    }

    /// Protection des vêtements portés (le meilleur de chaque sorte) : (contre le froid, contre la chaleur).
    pub fn clothing(&self) -> (f32, f32) {
        let best = |f: fn(Item) -> f32| self.items.keys().map(|i| f(*i)).fold(0.0f32, f32::max);
        (best(Item::warmth), best(Item::cooling))
    }

    /// Part des dégâts absorbée par l'armure portée.
    pub fn armor_reduction(&self) -> f32 {
        let (head, body) = self.worn_armor();
        armor_reduction([head, body])
    }
}

/// Part des dégâts absorbée par un ensemble de pièces d'armure.
pub fn armor_reduction(pieces: [Option<Item>; 2]) -> f32 {
    pieces
        .iter()
        .flatten()
        .filter_map(|i| i.armor())
        .map(|(_, v)| v)
        .sum::<f32>()
        .min(0.6)
}

/// Ce que donne un bloc cassé (la pomme des feuilles est tirée au sort par le serveur).
pub fn drop_for(block: Block) -> Option<Item> {
    match block {
        Block::Air | Block::TallGrass | Block::RottenFruit => None,
        // Tous les troncs donnent du bois, tous les feuillages des feuilles.
        b if b.is_log() => Some(Item::Block(Block::Wood)),
        b if b.is_leaves() => Some(Item::Block(Block::Leaves)),
        Block::FallenFruit => Some(Item::Apple),
        Block::Grass => Some(Item::Block(Block::Dirt)),
        // Les filons donnent leur ressource, jamais le bloc lui-même (qui serait un filon neuf).
        Block::IronOre => Some(Item::RawIron),
        Block::CoalOre => Some(Item::Coal),
        Block::DoorOpen => Some(Item::Block(Block::Door)),
        Block::ElectricDoorOpen => Some(Item::Block(Block::ElectricDoor)),
        // Les cultures rendent leurs graines, le blé mûr du blé (le serveur ajoute des graines).
        Block::Sprout | Block::YoungWheat => Some(Item::Seeds),
        Block::RipeWheat => Some(Item::Wheat),
        Block::CarrotSprout | Block::CarrotPlant => Some(Item::CarrotSeeds),
        Block::RipeCarrot => Some(Item::Carrot),
        other => Some(Item::Block(other)),
    }
}

/// Vitesse de minage, en points de vie de bloc par seconde.
pub fn mining_speed(block: Block, held: Option<Item>) -> f32 {
    let pick = match held {
        Some(Item::Pickaxe(Tier::Wood)) => 2.0,
        Some(Item::Pickaxe(Tier::Stone)) => 3.5,
        Some(Item::Pickaxe(Tier::Iron)) => 5.0,
        _ => 1.0,
    };
    let hard = matches!(
        block,
        Block::Stone | Block::IronOre | Block::CoalOre | Block::ReinforcedWall | Block::Spikes
    );
    let woody = matches!(
        block,
        Block::Wood | Block::Planks | Block::Door | Block::DoorOpen | Block::Chest | Block::Workbench
    ) || block.is_log();
    let axe = match held {
        Some(Item::Axe(Tier::Wood)) => 2.0,
        Some(Item::Axe(Tier::Stone)) => 3.5,
        Some(Item::Axe(Tier::Iron)) => 5.0,
        _ => 1.0,
    };
    if hard {
        40.0 * pick
    } else if woody && axe > 1.0 {
        40.0 * axe
    } else {
        40.0 * pick.sqrt()
    }
}

/// Temps pour casser un bloc, en secondes ; `None` si l'outil ne suffit pas (minerai à la main).
pub fn break_time(block: Block, held: Option<Item>) -> Option<f32> {
    let needs_pick = matches!(block, Block::IronOre | Block::ReinforcedWall);
    if needs_pick && !matches!(held, Some(Item::Pickaxe(_))) {
        return None;
    }
    Some(block.max_health() as f32 / mining_speed(block, held))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_add_and_remove() {
        let mut inv = Inventory::default();
        inv.add(Item::Stick, 3);
        assert!(inv.remove(Item::Stick, 2));
        assert!(!inv.remove(Item::Stick, 2));
        assert_eq!(inv.count(Item::Stick), 1);
        assert!(inv.remove(Item::Stick, 1));
        assert!(inv.is_empty());
    }

    #[test]
    fn pickaxes_mine_stone_faster() {
        let hand = break_time(Block::Stone, None).unwrap();
        let iron = break_time(Block::Stone, Some(Item::Pickaxe(Tier::Iron))).unwrap();
        assert!(iron < hand / 4.0);
        assert!(break_time(Block::IronOre, None).is_none());
        assert!(break_time(Block::Dirt, None).unwrap() < 1.0);
    }

    #[test]
    fn best_armor_is_worn_and_capped() {
        let mut inv = Inventory::default();
        assert_eq!(inv.armor_reduction(), 0.0);
        inv.add(Item::Helmet(Tier::Wood), 1);
        inv.add(Item::Helmet(Tier::Iron), 1);
        inv.add(Item::BodyArmor(Tier::Iron), 1);
        assert_eq!(
            inv.worn_armor(),
            (Some(Item::Helmet(Tier::Iron)), Some(Item::BodyArmor(Tier::Iron)))
        );
        assert!((inv.armor_reduction() - 0.45).abs() < 1e-6);
        assert!(armor_reduction([Some(Item::BodyArmor(Tier::Iron)); 2]) <= 0.6);
    }

    #[test]
    fn melee_weapons_differ() {
        let spear = Item::melee(Some(Item::Spear(Tier::Iron)));
        let axe = Item::melee(Some(Item::Axe(Tier::Iron)));
        let sword = Item::melee(Some(Item::Sword(Tier::Iron)));
        assert!(spear.reach > sword.reach && axe.damage > sword.damage);
        assert!(Item::melee(Some(Item::Mace)).sweep);
        assert!(Item::Mace.is_giftable() && Item::Helmet(Tier::Wood).is_giftable() && !Item::Apple.is_giftable());
        let hand = break_time(Block::Wood, None).unwrap();
        assert!(break_time(Block::Wood, Some(Item::Axe(Tier::Stone))).unwrap() < hand / 2.0);
    }
}
