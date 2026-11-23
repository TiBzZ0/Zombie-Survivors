//! Grandes machines posées sur plusieurs cases : le bloc d'origine (la machine elle-même)
//! et des cases `Block::Part` qui l'agrandissent, couchées le long de X ou de Z.

use bevy_math::IVec3;

use crate::block::Block;

/// Cases occupées en plus du bloc d'origine, pour une machine couchée le long de X.
pub const fn footprint(block: Block) -> &'static [IVec3] {
    const EAST: &[IVec3] = &[IVec3::new(1, 0, 0)];
    const UP: &[IVec3] = &[IVec3::new(0, 1, 0)];
    match block {
        Block::Workbench | Block::Cannon | Block::Catapult => EAST,
        Block::Forge | Block::Turret | Block::Floodlight | Block::TeslaCoil => UP,
        _ => &[],
    }
}

/// Machine qui occupe plusieurs cases.
pub const fn is_multiblock(block: Block) -> bool {
    !footprint(block).is_empty()
}

/// Décalage pour une machine couchée le long de Z plutôt que de X.
pub const fn turned(offset: IVec3, along_z: bool) -> IVec3 {
    if along_z {
        IVec3::new(offset.z, offset.y, offset.x)
    } else {
        offset
    }
}

/// Cases supplémentaires d'une machine posée en `origin`.
pub fn cells(origin: IVec3, block: Block, along_z: bool) -> Vec<IVec3> {
    footprint(block).iter().map(|o| origin + turned(*o, along_z)).collect()
}

/// Sens d'une machine déjà posée : le long de Z si ses cases de ce côté sont occupées.
pub fn along_z(origin: IVec3, block_at: impl Fn(IVec3) -> Block) -> bool {
    let block = block_at(origin);
    let complete = |z: bool| cells(origin, block, z).iter().all(|p| block_at(*p) == Block::Part);
    !complete(false) && complete(true)
}

/// Machine à laquelle appartient la case `pos` : elle-même si c'est l'origine, sinon la machine
/// voisine dont une case est `pos`.
pub fn origin_of(pos: IVec3, block_at: impl Fn(IVec3) -> Block) -> Option<IVec3> {
    let block = block_at(pos);
    if is_multiblock(block) {
        return Some(pos);
    }
    if block != Block::Part {
        return None;
    }
    // Les décalages possibles : ceux de toutes les machines, dans les deux sens.
    const SHAPES: [Block; 2] = [Block::Workbench, Block::Turret];
    for shape in SHAPES {
        for z in [false, true] {
            for offset in footprint(shape) {
                let origin = pos - turned(*offset, z);
                let owner = block_at(origin);
                if is_multiblock(owner) && cells(origin, owner, along_z(origin, &block_at)).contains(&pos) {
                    return Some(origin);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn parts_lead_back_to_their_machine() {
        let mut world = HashMap::new();
        world.insert(IVec3::ZERO, Block::Workbench);
        world.insert(IVec3::Z, Block::Part);
        world.insert(IVec3::new(5, 0, 0), Block::Turret);
        world.insert(IVec3::new(5, 1, 0), Block::Part);
        world.insert(IVec3::new(9, 0, 0), Block::Part);
        let at = |p: IVec3| world.get(&p).copied().unwrap_or(Block::Air);
        assert!(along_z(IVec3::ZERO, at));
        assert_eq!(origin_of(IVec3::Z, at), Some(IVec3::ZERO));
        assert_eq!(origin_of(IVec3::new(5, 1, 0), at), Some(IVec3::new(5, 0, 0)));
        assert_eq!(origin_of(IVec3::new(9, 0, 0), at), None, "case orpheline");
        assert_eq!(cells(IVec3::ZERO, Block::Cannon, true), vec![IVec3::Z]);
        assert!(!is_multiblock(Block::Chest));
    }
}
