//! Recettes de fabrication.

use bevy_math::IVec3;

use crate::block::Block;
use crate::item::{Inventory, Item, Tier};

pub struct Recipe {
    pub output: Item,
    pub count: u32,
    pub inputs: &'static [(Item, u32)],
    /// Atelier qu'il faut avoir à côté de soi (`None` : à la main).
    pub station: Option<Block>,
}

/// Distance maximale (en blocs, sur chaque axe) entre le joueur et l'atelier.
pub const STATION_RANGE: i32 = 4;

/// Vrai si un bloc `station` se trouve près de `center`.
pub fn station_near(center: IVec3, station: Block, block_at: impl Fn(IVec3) -> Block) -> bool {
    let r = STATION_RANGE;
    (-r..=r).any(|x| (-r..=r).any(|y| (-r..=r).any(|z| block_at(center + IVec3::new(x, y, z)) == station)))
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
        station: None,
    },
    Recipe {
        output: Item::Stick,
        count: 4,
        inputs: &[(b(Block::Planks), 2)],
        station: None,
    },
    Recipe {
        output: b(Block::Workbench),
        count: 1,
        inputs: &[(b(Block::Planks), 6)],
        station: None,
    },
    Recipe {
        output: b(Block::Forge),
        count: 1,
        inputs: &[(b(Block::Stone), 10), (Item::Coal, 2)],
        station: Some(Block::Workbench),
    },
    Recipe {
        output: Item::Pickaxe(Tier::Wood),
        count: 1,
        inputs: &[(b(Block::Planks), 3), (Item::Stick, 2)],
        station: None,
    },
    Recipe {
        output: Item::Sword(Tier::Wood),
        count: 1,
        inputs: &[(b(Block::Planks), 2), (Item::Stick, 1)],
        station: None,
    },
    Recipe {
        output: Item::Pickaxe(Tier::Stone),
        count: 1,
        inputs: &[(b(Block::Stone), 3), (Item::Stick, 2)],
        station: Some(Block::Workbench),
    },
    Recipe {
        output: Item::Sword(Tier::Stone),
        count: 1,
        inputs: &[(b(Block::Stone), 2), (Item::Stick, 1)],
        station: Some(Block::Workbench),
    },
    Recipe {
        output: Item::IronIngot,
        count: 1,
        inputs: &[(Item::RawIron, 1), (Item::Coal, 1)],
        station: Some(Block::Forge),
    },
    Recipe {
        output: Item::Pickaxe(Tier::Iron),
        count: 1,
        inputs: &[(Item::IronIngot, 3), (Item::Stick, 2)],
        station: Some(Block::Forge),
    },
    Recipe {
        output: Item::Sword(Tier::Iron),
        count: 1,
        inputs: &[(Item::IronIngot, 2), (Item::Stick, 1)],
        station: Some(Block::Forge),
    },
    Recipe {
        output: b(Block::ReinforcedWall),
        count: 4,
        inputs: &[(Item::IronIngot, 1), (b(Block::Stone), 4)],
        station: Some(Block::Forge),
    },
    Recipe {
        output: b(Block::Spikes),
        count: 2,
        inputs: &[(Item::IronIngot, 1), (Item::Stick, 3)],
        station: Some(Block::Forge),
    },
    Recipe {
        output: Item::Bow,
        count: 1,
        inputs: &[(Item::Stick, 3), (b(Block::Leaves), 3)],
        station: Some(Block::Workbench),
    },
    Recipe {
        output: Item::Arrow,
        count: 8,
        inputs: &[(Item::Stick, 2), (b(Block::Stone), 1)],
        station: Some(Block::Workbench),
    },
    Recipe {
        output: Item::Rifle,
        count: 1,
        inputs: &[(Item::IronIngot, 5), (b(Block::Planks), 2)],
        station: Some(Block::Forge),
    },
    Recipe {
        output: Item::Ammo,
        count: 12,
        inputs: &[(Item::IronIngot, 1), (Item::Coal, 2)],
        station: Some(Block::Forge),
    },
    Recipe {
        output: b(Block::Door),
        count: 2,
        inputs: &[(b(Block::Planks), 6)],
        station: Some(Block::Workbench),
    },
    Recipe {
        output: b(Block::Chest),
        count: 1,
        inputs: &[(b(Block::Planks), 8)],
        station: Some(Block::Workbench),
    },
    Recipe {
        output: Item::Ration,
        count: 1,
        inputs: &[(Item::Apple, 3)],
        station: None,
    },
    Recipe {
        output: Item::Bandage,
        count: 1,
        inputs: &[(b(Block::Leaves), 4), (Item::Stick, 1)],
        station: None,
    },
    Recipe {
        output: b(Block::BarbedWire),
        count: 4,
        inputs: &[(Item::IronIngot, 1), (Item::Stick, 2)],
        station: Some(Block::Workbench),
    },
    Recipe {
        output: b(Block::Turret),
        count: 1,
        inputs: &[(Item::IronIngot, 3), (b(Block::Planks), 4), (Item::Bow, 1)],
        station: Some(Block::Forge),
    },
    Recipe {
        output: Item::Spear(Tier::Wood),
        count: 1,
        inputs: &[(Item::Stick, 3), (b(Block::Planks), 1)],
        station: None,
    },
    Recipe {
        output: Item::Axe(Tier::Wood),
        count: 1,
        inputs: &[(b(Block::Planks), 3), (Item::Stick, 2)],
        station: None,
    },
    Recipe {
        output: Item::Spear(Tier::Stone),
        count: 1,
        inputs: &[(Item::Stick, 3), (b(Block::Stone), 1)],
        station: Some(Block::Workbench),
    },
    Recipe {
        output: Item::Axe(Tier::Stone),
        count: 1,
        inputs: &[(b(Block::Stone), 3), (Item::Stick, 2)],
        station: Some(Block::Workbench),
    },
    Recipe {
        output: Item::Spear(Tier::Iron),
        count: 1,
        inputs: &[(Item::Stick, 3), (Item::IronIngot, 1)],
        station: Some(Block::Forge),
    },
    Recipe {
        output: Item::Axe(Tier::Iron),
        count: 1,
        inputs: &[(Item::IronIngot, 3), (Item::Stick, 2)],
        station: Some(Block::Forge),
    },
    Recipe {
        output: Item::Mace,
        count: 1,
        inputs: &[(Item::IronIngot, 4), (Item::Stick, 2), (b(Block::Stone), 2)],
        station: Some(Block::Forge),
    },
    Recipe {
        output: Item::Helmet(Tier::Wood),
        count: 1,
        inputs: &[(b(Block::Planks), 5)],
        station: Some(Block::Workbench),
    },
    Recipe {
        output: Item::BodyArmor(Tier::Wood),
        count: 1,
        inputs: &[(b(Block::Planks), 8), (Item::Stick, 2)],
        station: Some(Block::Workbench),
    },
    Recipe {
        output: Item::Helmet(Tier::Stone),
        count: 1,
        inputs: &[(b(Block::Stone), 5), (Item::Stick, 1)],
        station: Some(Block::Workbench),
    },
    Recipe {
        output: Item::BodyArmor(Tier::Stone),
        count: 1,
        inputs: &[(b(Block::Stone), 8), (Item::Stick, 2)],
        station: Some(Block::Workbench),
    },
    Recipe {
        output: Item::Helmet(Tier::Iron),
        count: 1,
        inputs: &[(Item::IronIngot, 5)],
        station: Some(Block::Forge),
    },
    Recipe {
        output: Item::BodyArmor(Tier::Iron),
        count: 1,
        inputs: &[(Item::IronIngot, 8)],
        station: Some(Block::Forge),
    },
    Recipe {
        output: Item::Crossbow,
        count: 1,
        inputs: &[(Item::IronIngot, 2), (b(Block::Planks), 3), (Item::Stick, 2)],
        station: Some(Block::Forge),
    },
    Recipe {
        output: Item::Bolt,
        count: 6,
        inputs: &[(Item::IronIngot, 1), (Item::Stick, 3)],
        station: Some(Block::Forge),
    },
    Recipe {
        output: Item::Grenade,
        count: 2,
        inputs: &[(Item::IronIngot, 1), (Item::Coal, 3)],
        station: Some(Block::Forge),
    },
    Recipe {
        output: Item::Dynamite,
        count: 2,
        inputs: &[(Item::Coal, 3), (b(Block::Sand), 2), (Item::Stick, 1)],
        station: Some(Block::Workbench),
    },
    Recipe {
        output: b(Block::MachineGun),
        count: 1,
        inputs: &[(Item::IronIngot, 6), (Item::Rifle, 1)],
        station: Some(Block::Forge),
    },
    Recipe {
        output: b(Block::Cannon),
        count: 1,
        inputs: &[(Item::IronIngot, 8), (Item::Coal, 4)],
        station: Some(Block::Forge),
    },
    Recipe {
        output: b(Block::Catapult),
        count: 1,
        inputs: &[(b(Block::Planks), 10), (Item::Stick, 6), (b(Block::Stone), 2)],
        station: Some(Block::Workbench),
    },
    Recipe {
        output: Item::CannonBall,
        count: 4,
        inputs: &[(Item::IronIngot, 2), (Item::Coal, 2)],
        station: Some(Block::Forge),
    },
    Recipe {
        output: b(Block::Campfire),
        count: 1,
        inputs: &[(Item::Stick, 4), (b(Block::Stone), 3), (Item::Coal, 1)],
        station: None,
    },
    Recipe {
        output: Item::Bread,
        count: 1,
        inputs: &[(Item::Wheat, 3)],
        station: Some(Block::Campfire),
    },
    Recipe {
        output: Item::CookedMeat,
        count: 1,
        inputs: &[(Item::RawMeat, 1)],
        station: Some(Block::Campfire),
    },
    Recipe {
        output: Item::Flask,
        count: 1,
        inputs: &[(Item::IronIngot, 1), (Item::Stick, 1)],
        station: Some(Block::Workbench),
    },
    Recipe {
        output: Item::FurCoat,
        count: 1,
        inputs: &[(Item::Hide, 4)],
        station: Some(Block::Workbench),
    },
    Recipe {
        output: Item::StrawHat,
        count: 1,
        inputs: &[(Item::Wheat, 4)],
        station: None,
    },
    Recipe {
        output: Item::Antidote,
        count: 1,
        inputs: &[(Item::Coal, 1), (Item::Apple, 2), (Item::Bandage, 1)],
        station: Some(Block::Workbench),
    },
    Recipe {
        output: Item::Ration,
        count: 2,
        inputs: &[(Item::Bread, 1), (Item::CookedMeat, 1)],
        station: None,
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

    fn recipe(output: Item) -> &'static Recipe {
        RECIPES.iter().find(|r| r.output == output).unwrap()
    }

    #[test]
    fn wood_to_pickaxe() {
        let mut inv = Inventory::default();
        inv.add(b(Block::Wood), 2);
        let planks = recipe(b(Block::Planks));
        assert!(planks.craft(&mut inv)); // 4 planches
        assert!(planks.craft(&mut inv)); // 8 planches
        assert!(recipe(Item::Stick).craft(&mut inv)); // 4 bâtons, 6 planches
        assert!(recipe(Item::Pickaxe(Tier::Wood)).craft(&mut inv)); // pioche
        assert_eq!(inv.count(Item::Pickaxe(Tier::Wood)), 1);
        assert_eq!(inv.count(b(Block::Planks)), 3);
        assert!(!recipe(Item::Pickaxe(Tier::Iron)).craft(&mut inv), "pas de fer");
    }

    #[test]
    fn stations_are_found_nearby_only() {
        let at = |p: IVec3| {
            if p == IVec3::new(3, 0, -2) {
                Block::Forge
            } else {
                Block::Air
            }
        };
        assert!(station_near(IVec3::ZERO, Block::Forge, at));
        assert!(!station_near(IVec3::new(10, 0, 0), Block::Forge, at));
        assert!(!station_near(IVec3::ZERO, Block::Workbench, at));
        // Le fer se travaille à la forge, l'établi se fabrique à la main.
        assert_eq!(recipe(Item::IronIngot).station, Some(Block::Forge));
        assert_eq!(recipe(b(Block::Workbench)).station, None);
    }
}
