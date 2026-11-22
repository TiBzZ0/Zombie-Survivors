//! Paramètres d'une partie, partagés par le serveur et le client.

use serde::{Deserialize, Serialize};

use crate::chunk::CHUNK_SIZE;

/// Paramètres d'un monde. Deux mondes avec les mêmes paramètres sont identiques bloc pour bloc.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldConfig {
    /// Graine du générateur.
    pub seed: i32,
    /// Largeur de la carte en chunks, sur X comme sur Z (la carte est carrée et de taille fixe).
    pub size_chunks: i32,
    /// Hauteur de la carte en chunks.
    pub height_chunks: i32,
    /// Version du générateur : 1 = terrain d'origine (anciennes sauvegardes),
    /// 2 = biomes, lacs et grandes ruines.
    pub generation: u32,
}

/// Version du générateur des nouveaux mondes.
pub const CURRENT_GENERATION: u32 = 2;

impl Default for WorldConfig {
    fn default() -> Self {
        Self {
            seed: 42,
            // 32 chunks de 32 blocs : une carte de 1 024 m de côté pour commencer.
            size_chunks: 32,
            height_chunks: 4,
            generation: CURRENT_GENERATION,
        }
    }
}

impl WorldConfig {
    /// Largeur de la carte en blocs.
    pub fn size_blocks(&self) -> i32 {
        self.size_chunks * CHUNK_SIZE as i32
    }

    /// Hauteur de la carte en blocs.
    pub fn height_blocks(&self) -> i32 {
        self.height_chunks * CHUNK_SIZE as i32
    }

    /// Vrai si la position de chunk est à l'intérieur de la carte.
    pub fn contains_chunk(&self, pos: crate::chunk::ChunkPos) -> bool {
        (0..self.size_chunks).contains(&pos.x)
            && (0..self.height_chunks).contains(&pos.y)
            && (0..self.size_chunks).contains(&pos.z)
    }
}
