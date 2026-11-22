//! Recettes de fabrication.

use crate::block::Block;
use crate::item::{Inventory, Item, Tier};

pub struct Recipe {
    pub output: Item,
    pub count: u32,
    pub inputs: &'static [(Item, u32)],
}

const fn b(block: Block) -> Item {
    Item::Block(block)
}

/// Toutes les recettes, dans l'ordre d'affichage. L'indice sert d'identifiant réseau.
pub const RECIPES: &[Recipe] = &[
    Recipe {
        output: b(Block::Planks),
        count: 4,
        inputs: &[(b(Block::Wood), 1)],
    },
    Recipe {
        output: Item::Stick,
        count: 4,
        inputs: &[(b(Block::Planks), 2)],
    },
    Recipe {
        output: Item::Pickaxe(Tier::Wood),
        count: 1,
        inputs: &[(b(Block::Planks), 3), (Item::Stick, 2)],
    },
    Recipe {
        output: Item::Sword(Tier::Wood),
        count: 1,
        inputs: &[(b(Block::Planks), 2), (Item::Stick, 1)],
    },
    Recipe {
        output: Item::Pickaxe(Tier::Stone),
        count: 1,
        inputs: &[(b(Block::Stone), 3), (Item::Stick, 2)],
    },
    Recipe {
        output: Item::Sword(Tier::Stone),
        count: 1,
        inputs: &[(b(Block::Stone), 2), (Item::Stick, 1)],
    },
    Recipe {
        output: Item::IronIngot,
        count: 1,
        inputs: &[(Item::RawIron, 1), (Item::Coal, 1)],
    },
    Recipe {
        output: Item::Pickaxe(Tier::Iron),
        count: 1,
        inputs: &[(Item::IronIngot, 3), (Item::Stick, 2)],
    },
    Recipe {
        output: Item::Sword(Tier::Iron),
        count: 1,
        inputs: &[(Item::IronIngot, 2), (Item::Stick, 1)],
    },
    Recipe {
        output: b(Block::ReinforcedWall),
        count: 4,
        inputs: &[(Item::IronIngot, 1), (b(Block::Stone), 4)],
    },
    Recipe {
        output: b(Block::Spikes),
        count: 2,
        inputs: &[(Item::IronIngot, 1), (Item::Stick, 3)],
    },
    Recipe {
        output: Item::Bow,
        count: 1,
        inputs: &[(Item::Stick, 3), (b(Block::Leaves), 3)],
    },
    Recipe {
        output: Item::Arrow,
        count: 8,
        inputs: &[(Item::Stick, 2), (b(Block::Stone), 1)],
    },
    Recipe {
        output: Item::Rifle,
        count: 1,
        inputs: &[(Item::IronIngot, 5), (b(Block::Planks), 2)],
    },
    Recipe {
        output: Item::Ammo,
        count: 12,
        inputs: &[(Item::IronIngot, 1), (Item::Coal, 2)],
    },
    Recipe {
        output: Item::Ration,
        count: 1,
        inputs: &[(Item::Apple, 3)],
    },
    Recipe {
        output: Item::Bandage,
        count: 1,
        inputs: &[(b(Block::Leaves), 4), (Item::Stick, 1)],
    },
];

impl Recipe {
    pub fn can_craft(&self, inventory: &Inventory) -> bool {
        self.inputs.iter().all(|(item, n)| inventory.has(*item, *n))
    }

    /// Fabrique l'objet si possible.
    pub fn craft(&self, inventory: &mut Inventory) -> bool {
        if !self.can_craft(inventory) {
            return false;
        }
        for (item, n) in self.inputs {
            inventory.remove(*item, *n);
        }
        inventory.add(self.output, self.count);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wood_to_pickaxe() {
        let mut inv = Inventory::default();
        inv.add(b(Block::Wood), 2);
        assert!(RECIPES[0].craft(&mut inv)); // 4 planches
        assert!(RECIPES[0].craft(&mut inv)); // 8 planches
        assert!(RECIPES[1].craft(&mut inv)); // 4 bâtons, 6 planches
        assert!(RECIPES[2].craft(&mut inv)); // pioche
        assert_eq!(inv.count(Item::Pickaxe(Tier::Wood)), 1);
        assert_eq!(inv.count(b(Block::Planks)), 3);
        assert!(!RECIPES[7].craft(&mut inv), "pas de fer");
    }
}
