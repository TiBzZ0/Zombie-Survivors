//! Le monde tel qu'il est : terrain généré plus modifications des joueurs.
//!
//! Les modifications sont stockées bloc par bloc. C'est ce que le serveur envoie
//! aux joueurs qui arrivent, et ce qu'il sauvegarde sur le disque.

use std::collections::HashMap;
use std::sync::Arc;

use bevy_math::{IVec3, Vec3};

use crate::block::Block;
use crate::chunk::{CHUNK_SIZE, ChunkPos};
use crate::config::WorldConfig;
use crate::worldgen::WorldGenerator;

/// Position d'un bloc dans le monde.
pub type BlockPos = IVec3;

/// Chunk qui contient un bloc.
pub fn chunk_of(pos: BlockPos) -> ChunkPos {
    ChunkPos::from_block(pos.x, pos.y, pos.z)
}

/// Bloc qui contient un point.
pub fn block_containing(point: Vec3) -> BlockPos {
    point.floor().as_ivec3()
}

/// Terrain généré et modifications par-dessus.
pub struct WorldState {
    generator: Arc<WorldGenerator>,
    edits: HashMap<BlockPos, Block>,
}

impl WorldState {
    pub fn new(config: WorldConfig) -> Self {
        Self {
            generator: Arc::new(WorldGenerator::new(config)),
            edits: HashMap::new(),
        }
    }

    pub fn config(&self) -> &WorldConfig {
        self.generator.config()
    }

    pub fn generator(&self) -> &Arc<WorldGenerator> {
        &self.generator
    }

    pub fn edits(&self) -> &HashMap<BlockPos, Block> {
        &self.edits
    }

    /// Vrai si le bloc fait partie de la carte.
    pub fn in_bounds(&self, pos: BlockPos) -> bool {
        let c = self.config();
        (0..c.size_blocks()).contains(&pos.x)
            && (0..c.height_blocks()).contains(&pos.y)
            && (0..c.size_blocks()).contains(&pos.z)
    }

    pub fn block(&self, pos: BlockPos) -> Block {
        match self.edits.get(&pos) {
            Some(block) => *block,
            None => self.generator.block_at(pos.x, pos.y, pos.z),
        }
    }

    pub fn is_solid(&self, pos: BlockPos) -> bool {
        self.block(pos).is_solid()
    }

    /// Change un bloc. Renvoie faux (et ne fait rien) hors de la carte.
    pub fn set_block(&mut self, pos: BlockPos, block: Block) -> bool {
        if !self.in_bounds(pos) {
            return false;
        }
        if self.generator.block_at(pos.x, pos.y, pos.z) == block {
            self.edits.remove(&pos);
        } else {
            self.edits.insert(pos, block);
        }
        true
    }

    /// Modifications qui touchent un chunk ou sa bordure d'un bloc (utile au maillage).
    pub fn edits_around_chunk(&self, chunk: ChunkPos) -> HashMap<BlockPos, Block> {
        let min = IVec3::from(chunk.origin()) - IVec3::ONE;
        let max = min + IVec3::splat(CHUNK_SIZE as i32 + 1);
        self.edits
            .iter()
            .filter(|(p, _)| p.cmpge(min).all() && p.cmple(max).all())
            .map(|(p, b)| (*p, *b))
            .collect()
    }

    /// Chunks dont le maillage change quand ce bloc change (le sien, plus ses voisins s'il est au bord).
    pub fn chunks_touched_by(pos: BlockPos) -> Vec<ChunkPos> {
        let mut out = vec![chunk_of(pos)];
        for offset in [
            IVec3::X,
            IVec3::NEG_X,
            IVec3::Y,
            IVec3::NEG_Y,
            IVec3::Z,
            IVec3::NEG_Z,
        ] {
            let c = chunk_of(pos + offset);
            if !out.contains(&c) {
                out.push(c);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_override_generated_terrain() {
        let mut world = WorldState::new(WorldConfig::default());
        let pos = IVec3::new(100, 1, 100);
        assert!(world.is_solid(pos));
        assert!(world.set_block(pos, Block::Air));
        assert!(!world.is_solid(pos));
        assert_eq!(world.edits().len(), 1);
        // Remettre le bloc d'origine efface la modification.
        let original = world.generator().block_at(100, 1, 100);
        world.set_block(pos, original);
        assert!(world.edits().is_empty());
    }

    #[test]
    fn cannot_edit_outside_the_map() {
        let mut world = WorldState::new(WorldConfig::default());
        assert!(!world.set_block(IVec3::new(-1, 5, 5), Block::Stone));
        assert!(world.edits().is_empty());
    }

    #[test]
    fn border_block_touches_neighbour_chunk() {
        let touched = WorldState::chunks_touched_by(IVec3::new(32, 40, 5));
        assert!(touched.contains(&ChunkPos::new(1, 1, 0)));
        assert!(touched.contains(&ChunkPos::new(0, 1, 0)));
        assert_eq!(
            WorldState::chunks_touched_by(IVec3::new(40, 40, 40)).len(),
            1
        );
    }
}
