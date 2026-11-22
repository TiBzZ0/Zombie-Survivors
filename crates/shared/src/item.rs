//! Objets : blocs à poser, outils, armes, nourriture. Inventaire.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::block::Block;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Item {
    Block(Block),
    Stick,
    IronIngot,
    Apple,
    Ration,
    Bandage,
    Sword(Tier),
    Pickaxe(Tier),
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
            Item::Apple => "Pomme".into(),
            Item::Ration => "Ration".into(),
            Item::Bandage => "Bandage".into(),
            Item::Sword(t) => format!("Épée {}", t.label()),
            Item::Pickaxe(t) => format!("Pioche {}", t.label()),
        }
    }

    /// Dégâts d'un coup porté avec cet objet en main.
    pub fn melee_damage(held: Option<Item>) -> f32 {
        match held {
            Some(Item::Sword(Tier::Wood)) => 18.0,
            Some(Item::Sword(Tier::Stone)) => 26.0,
            Some(Item::Sword(Tier::Iron)) => 38.0,
            Some(Item::Pickaxe(_)) => 12.0,
            _ => 9.0,
        }
    }

    /// Ce que rapporte un aliment : (faim, santé).
    pub const fn food(self) -> Option<(f32, f32)> {
        match self {
            Item::Apple => Some((20.0, 0.0)),
            Item::Ration => Some((55.0, 5.0)),
            Item::Bandage => Some((0.0, 35.0)),
            _ => None,
        }
    }

    pub const fn is_tool(self) -> bool {
        matches!(self, Item::Sword(_) | Item::Pickaxe(_))
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
}

/// Ce que donne un bloc cassé (la pomme des feuilles est tirée au sort par le serveur).
pub fn drop_for(block: Block) -> Option<Item> {
    match block {
        Block::Air => None,
        Block::Grass => Some(Item::Block(Block::Dirt)),
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
        Block::Stone | Block::IronOre | Block::ReinforcedWall | Block::Spikes
    );
    if hard { 40.0 * pick } else { 40.0 * pick.sqrt() }
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
}
