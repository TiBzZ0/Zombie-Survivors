//! Le monde tel qu'il est : terrain généré plus modifications des joueurs.
//!
//! Les modifications sont stockées bloc par bloc. C'est ce que le serveur envoie
//! aux joueurs qui arrivent, et ce qu'il sauvegarde sur le disque.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use bevy_math::{IVec3, Vec3};

use crate::block::Block;
use crate::chunk::{CHUNK_SIZE, Chunk, ChunkPos};
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
    /// Chunks déjà générés (modifications comprises), pour des lectures rapides.
    cache: Mutex<HashMap<ChunkPos, Arc<Chunk>>>,
    /// Blocs changés depuis le dernier `take_changes`, si le suivi est activé (serveur).
    changes: Option<Vec<BlockPos>>,
    /// Blocs porteurs disparus depuis le dernier `take_removals`, si le suivi est activé.
    removals: Option<Vec<BlockPos>>,
    /// Troncs disparus depuis le dernier `take_felled`, si le suivi est activé.
    felled: Option<Vec<BlockPos>>,
}

impl WorldState {
    pub fn new(config: WorldConfig) -> Self {
        Self {
            generator: Arc::new(WorldGenerator::new(config)),
            edits: HashMap::new(),
            cache: Mutex::new(HashMap::new()),
            changes: None,
            removals: None,
            felled: None,
        }
    }

    /// Active le suivi des blocs changés (lus avec `take_changes`).
    pub fn track_changes(&mut self) {
        self.changes.get_or_insert_with(Vec::new);
    }

    /// Blocs changés depuis le dernier appel (vide si le suivi n'est pas activé).
    pub fn take_changes(&mut self) -> Vec<BlockPos> {
        self.changes.as_mut().map(std::mem::take).unwrap_or_default()
    }

    /// Active le suivi des troncs abattus (lus avec `take_felled`).
    pub fn track_felled(&mut self) {
        self.felled.get_or_insert_with(Vec::new);
    }

    /// Troncs disparus depuis le dernier appel : leur feuillage risque de tomber.
    pub fn take_felled(&mut self) -> Vec<BlockPos> {
        self.felled.as_mut().map(std::mem::take).unwrap_or_default()
    }

    /// Active le suivi des blocs porteurs disparus (lus avec `take_removals`).
    pub fn track_removals(&mut self) {
        self.removals.get_or_insert_with(Vec::new);
    }

    /// Blocs porteurs disparus depuis le dernier appel (cassés, explosés, portes ouvertes...).
    pub fn take_removals(&mut self) -> Vec<BlockPos> {
        self.removals.as_mut().map(std::mem::take).unwrap_or_default()
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
        if !self.in_bounds(pos) {
            return Block::Air;
        }
        let chunk = self.chunk(chunk_of(pos));
        let local = pos - IVec3::from(chunk_of(pos).origin());
        chunk.get(local.x as usize, local.y as usize, local.z as usize)
    }

    /// Le chunk avec ses modifications, généré et mis en cache au premier accès.
    pub fn chunk(&self, pos: ChunkPos) -> Arc<Chunk> {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache
            .entry(pos)
            .or_insert_with(|| {
                let mut chunk = self.generator.generate_chunk(pos);
                let origin = IVec3::from(pos.origin());
                for (p, b) in &self.edits {
                    if chunk_of(*p) == pos {
                        let l = *p - origin;
                        chunk.set(l.x as usize, l.y as usize, l.z as usize, *b);
                    }
                }
                Arc::new(chunk)
            })
            .clone()
    }

    pub fn is_solid(&self, pos: BlockPos) -> bool {
        self.block(pos).is_solid()
    }

    /// Change un bloc. Renvoie faux (et ne fait rien) hors de la carte.
    pub fn set_block(&mut self, pos: BlockPos, block: Block) -> bool {
        if !self.in_bounds(pos) {
            return false;
        }
        if let Some(changes) = self.changes.as_mut() {
            changes.push(pos);
        }
        if self.removals.is_some()
            && crate::structure::bears_load(self.block(pos))
            && !crate::structure::bears_load(block)
            && let Some(removals) = self.removals.as_mut()
        {
            removals.push(pos);
        }
        if self.felled.is_some()
            && self.block(pos).is_log()
            && !block.is_log()
            && let Some(felled) = self.felled.as_mut()
        {
            felled.push(pos);
        }
        if self.generator.block_at(pos.x, pos.y, pos.z) == block {
            self.edits.remove(&pos);
        } else {
            self.edits.insert(pos, block);
        }
        let cp = chunk_of(pos);
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(chunk) = cache.get_mut(&cp) {
            let l = pos - IVec3::from(cp.origin());
            Arc::make_mut(chunk).set(l.x as usize, l.y as usize, l.z as usize, block);
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
        for offset in [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z] {
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
        assert_eq!(WorldState::chunks_touched_by(IVec3::new(40, 40, 40)).len(), 1);
    }
}
