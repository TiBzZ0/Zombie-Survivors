//! Stockage d'un chunk : un cube de `CHUNK_SIZE`³ blocs.

use serde::{Deserialize, Serialize};

use crate::block::Block;

/// Côté d'un chunk, en blocs.
pub const CHUNK_SIZE: usize = 32;
/// Nombre de blocs dans un chunk.
pub const CHUNK_VOLUME: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE;

/// Position d'un chunk dans la grille des chunks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ChunkPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl ChunkPos {
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    /// Chunk qui contient le bloc aux coordonnées monde données.
    pub const fn from_block(x: i32, y: i32, z: i32) -> Self {
        let s = CHUNK_SIZE as i32;
        Self::new(x.div_euclid(s), y.div_euclid(s), z.div_euclid(s))
    }

    /// Coordonnées monde du coin minimal du chunk.
    pub const fn origin(self) -> [i32; 3] {
        let s = CHUNK_SIZE as i32;
        [self.x * s, self.y * s, self.z * s]
    }
}

/// Les blocs d'un chunk, rangés en X puis Z puis Y.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chunk {
    blocks: Vec<Block>,
}

impl Default for Chunk {
    fn default() -> Self {
        Self {
            blocks: vec![Block::Air; CHUNK_VOLUME],
        }
    }
}

impl std::fmt::Debug for Chunk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let solid = self.blocks.iter().filter(|b| b.is_solid()).count();
        f.debug_struct("Chunk").field("solid_blocks", &solid).finish()
    }
}

impl Chunk {
    #[inline]
    const fn index(x: usize, y: usize, z: usize) -> usize {
        x + z * CHUNK_SIZE + y * CHUNK_SIZE * CHUNK_SIZE
    }

    /// Bloc en coordonnées locales au chunk (0 à `CHUNK_SIZE` - 1).
    #[inline]
    pub fn get(&self, x: usize, y: usize, z: usize) -> Block {
        self.blocks[Self::index(x, y, z)]
    }

    /// Remplace le bloc en coordonnées locales au chunk.
    #[inline]
    pub fn set(&mut self, x: usize, y: usize, z: usize, block: Block) {
        self.blocks[Self::index(x, y, z)] = block;
    }

    /// Vrai si le chunk ne contient que de l'air : rien à afficher ni à envoyer.
    pub fn is_empty(&self) -> bool {
        self.blocks.iter().all(|b| !b.is_solid())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_then_get_returns_block() {
        let mut chunk = Chunk::default();
        chunk.set(1, 2, 3, Block::Stone);
        assert_eq!(chunk.get(1, 2, 3), Block::Stone);
        assert_eq!(chunk.get(3, 2, 1), Block::Air);
        assert!(!chunk.is_empty());
    }

    #[test]
    fn chunk_pos_handles_negative_coordinates() {
        assert_eq!(ChunkPos::from_block(-1, 0, 31), ChunkPos::new(-1, 0, 0));
        assert_eq!(ChunkPos::from_block(32, -33, 0), ChunkPos::new(1, -2, 0));
    }
}
