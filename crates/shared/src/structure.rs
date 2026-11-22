//! Solidité des constructions. Un bloc posé par un joueur tient s'il repose, de proche en
//! proche, sur le terrain naturel :
//! - un bloc posé sur le sol (ou accroché à la roche) a toute sa portée ;
//! - un bloc posé sur un autre transmet la même solidité vers le haut ;
//! - chaque bloc en porte-à-faux (sur le côté ou suspendu dessous) en perd un.
//!
//! Un bloc qui n'a plus aucune solidité s'effondre. La portée dépend du matériau : des
//! planches tiennent en porte-à-faux sur 4 blocs, un mur renforcé sur 7, la terre sur 1.

use std::collections::{HashMap, HashSet};

use bevy_math::IVec3;

use crate::block::Block;

/// Au-delà de ce nombre de blocs reliés, la construction est jugée solide (calcul borné).
pub const MAX_COMPONENT: usize = 4096;

const NEIGHBOURS: [IVec3; 6] = [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z, IVec3::Y, IVec3::NEG_Y];

/// Un bloc qui porte ce qui est posé dessus (une porte ouverte garde son linteau).
pub const fn bears_load(block: Block) -> bool {
    block.is_solid() || matches!(block, Block::DoorOpen | Block::ElectricDoorOpen)
}

/// Solidité d'un bloc posé sur le sol : nombre de blocs en porte-à-faux qu'il tolère, plus un.
pub const fn span(block: Block) -> u8 {
    match block {
        Block::Grass | Block::Dirt | Block::Sand | Block::Snow | Block::Leaves | Block::Cactus => 2,
        Block::Stone | Block::IronOre | Block::CoalOre => 4,
        Block::Wood | Block::Planks => 5,
        Block::ReinforcedWall => 8,
        _ => 3,
    }
}

/// Solidité de chaque bloc posé relié à `seeds`, ou `None` si la construction est trop
/// grande pour être évaluée (elle est alors jugée solide). `placed` dit si un bloc a été
/// posé par un joueur ; les autres blocs porteurs sont le terrain naturel, toujours stable.
pub fn evaluate(
    seeds: &[IVec3],
    block_at: impl Fn(IVec3) -> Block,
    placed: impl Fn(IVec3) -> bool,
) -> Option<HashMap<IVec3, u8>> {
    let is_built = |p: IVec3| placed(p) && bears_load(block_at(p));
    // Les blocs posés reliés aux points de départ.
    let mut component = HashSet::new();
    let mut stack: Vec<IVec3> = seeds.iter().copied().filter(|p| is_built(*p)).collect();
    while let Some(p) = stack.pop() {
        if !component.insert(p) {
            continue;
        }
        if component.len() > MAX_COMPONENT {
            return None;
        }
        stack.extend(
            NEIGHBOURS
                .iter()
                .map(|d| p + *d)
                .filter(|n| !component.contains(n) && is_built(*n)),
        );
    }
    // Les blocs qui touchent le terrain naturel ont toute leur portée.
    let mut support: HashMap<IVec3, u8> = HashMap::new();
    let mut buckets: Vec<Vec<IVec3>> = vec![Vec::new(); span(Block::ReinforcedWall) as usize + 1];
    for p in &component {
        let anchored = p.y <= 0
            || NEIGHBOURS.iter().any(|d| {
                let n = *p + *d;
                !placed(n) && bears_load(block_at(n))
            });
        if anchored {
            let s = span(block_at(*p));
            support.insert(*p, s);
            buckets[s as usize].push(*p);
        }
    }
    // Propagation, des plus solides aux moins solides.
    for level in (1..buckets.len()).rev() {
        while let Some(p) = buckets[level].pop() {
            if support.get(&p).copied() != Some(level as u8) {
                continue;
            }
            for d in NEIGHBOURS {
                let n = p + d;
                if !component.contains(&n) {
                    continue;
                }
                let carried = if d == IVec3::Y { level as u8 } else { level as u8 - 1 };
                let value = carried.min(span(block_at(n)));
                if value > support.get(&n).copied().unwrap_or(0) {
                    support.insert(n, value);
                    buckets[value as usize].push(n);
                }
            }
        }
    }
    for p in component {
        support.entry(p).or_insert(0);
    }
    Some(support)
}

/// Blocs posés reliés à `seeds` qui ne tiennent plus.
pub fn unsupported(seeds: &[IVec3], block_at: impl Fn(IVec3) -> Block, placed: impl Fn(IVec3) -> bool) -> Vec<IVec3> {
    let Some(support) = evaluate(seeds, block_at, placed) else {
        return Vec::new();
    };
    let mut fallen: Vec<IVec3> = support.into_iter().filter(|(_, s)| *s == 0).map(|(p, _)| p).collect();
    // Du bas vers le haut : chacun tombe sur ceux d'en dessous déjà retombés.
    fallen.sort_by_key(|p| (p.y, p.x, p.z));
    fallen
}

/// Le bloc `block` tiendrait-il s'il était posé en `pos` ?
pub fn would_hold(pos: IVec3, block: Block, block_at: impl Fn(IVec3) -> Block, placed: impl Fn(IVec3) -> bool) -> bool {
    if !bears_load(block) {
        return true;
    }
    let block_at = |p: IVec3| if p == pos { block } else { block_at(p) };
    let placed = |p: IVec3| p == pos || placed(p);
    evaluate(&[pos], block_at, placed).is_none_or(|s| s.get(&pos).copied().unwrap_or(0) > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un monde d'essai : sol naturel sous y = 0, blocs posés au-dessus.
    struct Site(HashMap<IVec3, Block>);

    impl Site {
        fn new() -> Self {
            Self(HashMap::new())
        }
        fn put(&mut self, p: IVec3, b: Block) {
            self.0.insert(p, b);
        }
        fn block(&self, p: IVec3) -> Block {
            if p.y < 1 {
                Block::Stone
            } else {
                self.0.get(&p).copied().unwrap_or(Block::Air)
            }
        }
        fn fallen(&self, seeds: &[IVec3]) -> Vec<IVec3> {
            unsupported(seeds, |p| self.block(p), |p| self.0.contains_key(&p))
        }
        fn holds(&self, p: IVec3, b: Block) -> bool {
            would_hold(p, b, |q| self.block(q), |q| self.0.contains_key(&q))
        }
    }

    #[test]
    fn a_pillar_on_the_ground_stands_however_tall() {
        let mut site = Site::new();
        for y in 1..=30 {
            site.put(IVec3::new(0, y, 0), Block::Dirt);
        }
        assert!(site.fallen(&[IVec3::new(0, 15, 0)]).is_empty());
    }

    #[test]
    fn planks_overhang_four_blocks_but_not_five() {
        let mut site = Site::new();
        for y in 1..=3 {
            site.put(IVec3::new(0, y, 0), Block::Planks);
        }
        for x in 1..=4 {
            site.put(IVec3::new(x, 3, 0), Block::Planks);
        }
        assert!(site.fallen(&[IVec3::new(0, 3, 0)]).is_empty());
        assert!(site.holds(IVec3::new(3, 2, 0), Block::Planks), "suspendu sous le 3e");
        assert!(!site.holds(IVec3::new(4, 2, 0), Block::Planks), "suspendu sous le 4e");
        assert!(!site.holds(IVec3::new(5, 3, 0), Block::Planks));
        assert!(!site.holds(IVec3::new(5, 3, 0), Block::ReinforcedWall));
    }

    #[test]
    fn removing_the_pillar_drops_the_beam() {
        let mut site = Site::new();
        site.put(IVec3::new(0, 1, 0), Block::Planks);
        site.put(IVec3::new(0, 2, 0), Block::Planks);
        for x in 1..=3 {
            site.put(IVec3::new(x, 2, 0), Block::Planks);
        }
        // Le pied du pilier est cassé.
        site.0.remove(&IVec3::new(0, 1, 0));
        let fallen = site.fallen(&[IVec3::new(0, 2, 0)]);
        assert_eq!(fallen.len(), 4);
        assert_eq!(fallen[0].y, 2);
    }

    #[test]
    fn a_bridge_between_two_pillars_spans_eight_blocks() {
        let mut site = Site::new();
        for x in [0, 9] {
            site.put(IVec3::new(x, 1, 0), Block::Planks);
        }
        for x in 0..=9 {
            site.put(IVec3::new(x, 2, 0), Block::Planks);
        }
        assert!(site.fallen(&[IVec3::new(5, 2, 0)]).is_empty());
        // Un des piliers cède : la moitié lointaine tombe.
        site.0.remove(&IVec3::new(9, 1, 0));
        let fallen = site.fallen(&[IVec3::new(9, 2, 0)]);
        assert_eq!(fallen.len(), 5, "{fallen:?}");
        assert!(fallen.iter().all(|p| p.x >= 5));
    }

    #[test]
    fn a_block_against_a_cliff_holds() {
        let mut site = Site::new();
        site.put(IVec3::new(0, 5, 0), Block::Dirt);
        let cliff = |p: IVec3| if p.x == -1 { Block::Stone } else { site.block(p) };
        assert!(would_hold(IVec3::new(0, 5, 0), Block::Dirt, cliff, |q| site
            .0
            .contains_key(&q)));
        assert!(!site.holds(IVec3::new(0, 5, 0), Block::Dirt));
    }

    #[test]
    fn an_open_door_still_carries_the_lintel() {
        let mut site = Site::new();
        site.put(IVec3::new(0, 1, 0), Block::DoorOpen);
        site.put(IVec3::new(0, 2, 0), Block::DoorOpen);
        site.put(IVec3::new(0, 3, 0), Block::Dirt);
        assert!(site.fallen(&[IVec3::new(0, 3, 0)]).is_empty());
    }

    #[test]
    fn non_load_bearing_blocks_never_hold_or_fall() {
        let site = Site::new();
        assert!(site.holds(IVec3::new(0, 10, 0), Block::BarbedWire));
        assert!(!site.holds(IVec3::new(0, 10, 0), Block::Stone));
    }
}
