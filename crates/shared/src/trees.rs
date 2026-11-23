//! Essences d'arbres : chêne, bouleau, sapin, pommier et cactus. La forme d'un arbre ne
//! dépend que de son essence et de sa hauteur ; elle sert au générateur du monde comme aux
//! pousses plantées qui grandissent.

use bevy_math::IVec3;

use crate::block::Block;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TreeKind {
    Oak,
    Birch,
    Pine,
    Apple,
    Cactus,
}

impl TreeKind {
    pub const fn log(self) -> Block {
        match self {
            TreeKind::Oak | TreeKind::Apple => Block::Wood,
            TreeKind::Birch => Block::BirchWood,
            TreeKind::Pine => Block::PineWood,
            TreeKind::Cactus => Block::Cactus,
        }
    }

    pub const fn leaves(self) -> Option<Block> {
        match self {
            TreeKind::Oak => Some(Block::Leaves),
            TreeKind::Birch => Some(Block::BirchLeaves),
            TreeKind::Pine => Some(Block::PineLeaves),
            TreeKind::Apple => Some(Block::FruitLeaves),
            TreeKind::Cactus => None,
        }
    }

    pub const fn sapling(self) -> Option<Block> {
        match self {
            TreeKind::Oak => Some(Block::OakSapling),
            TreeKind::Birch => Some(Block::BirchSapling),
            TreeKind::Pine => Some(Block::PineSapling),
            TreeKind::Apple => Some(Block::AppleSapling),
            TreeKind::Cactus => None,
        }
    }

    /// Essence d'une pousse.
    pub const fn of_sapling(block: Block) -> Option<TreeKind> {
        match block {
            Block::OakSapling => Some(TreeKind::Oak),
            Block::BirchSapling => Some(TreeKind::Birch),
            Block::PineSapling => Some(TreeKind::Pine),
            Block::AppleSapling => Some(TreeKind::Apple),
            _ => None,
        }
    }

    /// Essence d'un feuillage.
    pub const fn of_leaves(block: Block) -> Option<TreeKind> {
        match block {
            Block::Leaves => Some(TreeKind::Oak),
            Block::BirchLeaves => Some(TreeKind::Birch),
            Block::PineLeaves => Some(TreeKind::Pine),
            Block::FruitLeaves | Block::RipeFruitLeaves => Some(TreeKind::Apple),
            _ => None,
        }
    }

    /// Hauteur du tronc, d'après un nombre tiré au sort.
    pub const fn height(self, roll: u32) -> i32 {
        match self {
            TreeKind::Oak | TreeKind::Apple => 4 + (roll % 2) as i32,
            TreeKind::Birch => 5 + (roll % 2) as i32,
            TreeKind::Pine => 6 + (roll % 2) as i32,
            TreeKind::Cactus => 2 + (roll % 2) as i32,
        }
    }

    /// Rayon horizontal et hauteur totale (tronc et feuillage) d'un arbre.
    pub const fn extent(self, height: i32) -> (i32, i32) {
        match self {
            TreeKind::Cactus => (0, height),
            _ => (2, height + 2),
        }
    }

    /// Bloc de l'arbre à un décalage de son pied, ou `None` hors de l'arbre.
    pub fn part(self, height: i32, o: IVec3) -> Option<Block> {
        let (ax, az) = (o.x.abs(), o.z.abs());
        if o.x == 0 && o.z == 0 && (0..height).contains(&o.y) {
            return Some(self.log());
        }
        let leaves = self.leaves()?;
        let top = height - 1;
        let square = |r: i32| ax <= r && az <= r && !(r == 2 && ax == 2 && az == 2);
        let plus = ax + az <= 1;
        let inside = match (self, o.y - top) {
            // Chêne et pommier : une grosse boule.
            (TreeKind::Oak | TreeKind::Apple, -2..=0) => square(2),
            (TreeKind::Oak | TreeKind::Apple, 1) => square(1),
            // Bouleau : un feuillage étroit et haut.
            (TreeKind::Birch, -2 | 0) => square(1),
            (TreeKind::Birch, -1) => square(2),
            (TreeKind::Birch, 1) => plus,
            // Sapin : des étages de plus en plus petits vers la pointe.
            (TreeKind::Pine, -4 | -2) => square(2),
            (TreeKind::Pine, -3 | -1) => square(1),
            (TreeKind::Pine, 0) => plus,
            (TreeKind::Pine, 1) => ax == 0 && az == 0,
            _ => false,
        };
        inside.then_some(leaves)
    }

    /// Tous les blocs de l'arbre, avec leur décalage par rapport au pied.
    pub fn blocks(self, height: i32) -> Vec<(IVec3, Block)> {
        let (r, h) = self.extent(height);
        let mut out = Vec::new();
        for y in 0..h {
            for x in -r..=r {
                for z in -r..=r {
                    let o = IVec3::new(x, y, z);
                    if let Some(b) = self.part(height, o) {
                        out.push((o, b));
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_a_trunk_and_a_crown() {
        for kind in [TreeKind::Oak, TreeKind::Birch, TreeKind::Pine, TreeKind::Apple] {
            let h = kind.height(0);
            let blocks = kind.blocks(h);
            assert_eq!(blocks.iter().filter(|(_, b)| *b == kind.log()).count(), h as usize);
            let leaves = blocks.iter().filter(|(_, b)| Some(*b) == kind.leaves()).count();
            assert!(leaves >= 15, "{kind:?} : {leaves} feuilles");
            assert_eq!(TreeKind::of_sapling(kind.sapling().unwrap()), Some(kind));
            assert_eq!(TreeKind::of_leaves(kind.leaves().unwrap()), Some(kind));
        }
    }

    #[test]
    fn a_pine_is_pointed() {
        let h = TreeKind::Pine.height(0);
        let top = TreeKind::Pine.blocks(h).into_iter().map(|(o, _)| o.y).max().unwrap();
        let tip: Vec<_> = TreeKind::Pine
            .blocks(h)
            .into_iter()
            .filter(|(o, _)| o.y == top)
            .collect();
        assert_eq!(tip.len(), 1);
    }
}
