//! Transforme un chunk en maillage : une face par côté de bloc visible.
//!
//! Version simple (faces cachées retirées, pas de fusion). Le greedy meshing
//! viendra quand on mesurera le coût réel.

use shared::block::Block;
use shared::chunk::{CHUNK_SIZE, Chunk};

/// Données brutes d'un maillage, prêtes à être converties en `Mesh` Bevy.
#[derive(Default, Debug)]
pub struct MeshData {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
}

impl MeshData {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }
}

/// Les six directions : normale, puis les quatre coins de la face dans l'ordre anti-horaire vu de l'extérieur.
const FACES: [([i32; 3], [[f32; 3]; 4]); 6] = [
    ([1, 0, 0], [[1., 0., 1.], [1., 0., 0.], [1., 1., 0.], [1., 1., 1.]]),
    ([-1, 0, 0], [[0., 0., 0.], [0., 0., 1.], [0., 1., 1.], [0., 1., 0.]]),
    ([0, 1, 0], [[0., 1., 1.], [1., 1., 1.], [1., 1., 0.], [0., 1., 0.]]),
    ([0, -1, 0], [[0., 0., 0.], [1., 0., 0.], [1., 0., 1.], [0., 0., 1.]]),
    ([0, 0, 1], [[0., 0., 1.], [1., 0., 1.], [1., 1., 1.], [0., 1., 1.]]),
    ([0, 0, -1], [[1., 0., 0.], [0., 0., 0.], [0., 1., 0.], [1., 1., 0.]]),
];

/// Construit le maillage d'un chunk.
///
/// `solid_outside` répond pour les blocs voisins hors du chunk (coordonnées locales
/// pouvant valoir -1 ou `CHUNK_SIZE`), pour ne pas dessiner les faces collées au chunk d'à côté.
pub fn build_chunk_mesh(chunk: &Chunk, solid_outside: impl Fn(i32, i32, i32) -> bool) -> MeshData {
    let size = CHUNK_SIZE as i32;
    let solid = |x: i32, y: i32, z: i32| {
        if (0..size).contains(&x) && (0..size).contains(&y) && (0..size).contains(&z) {
            chunk.get(x as usize, y as usize, z as usize).is_solid()
        } else {
            solid_outside(x, y, z)
        }
    };

    let mut mesh = MeshData::default();
    for y in 0..size {
        for z in 0..size {
            for x in 0..size {
                let block = chunk.get(x as usize, y as usize, z as usize);
                if block == Block::Air {
                    continue;
                }
                let [r, g, b] = block.color();
                for (normal, corners) in FACES {
                    if solid(x + normal[0], y + normal[1], z + normal[2]) {
                        continue;
                    }
                    // Ombrage fixe par orientation, faute de vraie lumière ambiante pour l'instant.
                    let shade = match normal {
                        [0, 1, 0] => 1.0,
                        [0, -1, 0] => 0.55,
                        [_, 0, 0] => 0.8,
                        _ => 0.7,
                    };
                    let base = mesh.positions.len() as u32;
                    for c in corners {
                        mesh.positions.push([x as f32 + c[0], y as f32 + c[1], z as f32 + c[2]]);
                        mesh.normals.push(normal.map(|n| n as f32));
                        mesh.colors.push(srgb_to_linear([r * shade, g * shade, b * shade]));
                    }
                    mesh.indices
                        .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
                }
            }
        }
    }
    mesh
}

fn srgb_to_linear(c: [f32; 3]) -> [f32; 4] {
    let f = |v: f32| {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    [f(c[0]), f(c[1]), f(c[2]), 1.0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_block_has_six_faces() {
        let mut chunk = Chunk::default();
        chunk.set(5, 5, 5, Block::Stone);
        let mesh = build_chunk_mesh(&chunk, |_, _, _| false);
        assert_eq!(mesh.positions.len(), 6 * 4);
        assert_eq!(mesh.indices.len(), 6 * 6);
    }

    #[test]
    fn touching_blocks_hide_shared_faces() {
        let mut chunk = Chunk::default();
        chunk.set(5, 5, 5, Block::Stone);
        chunk.set(6, 5, 5, Block::Dirt);
        let mesh = build_chunk_mesh(&chunk, |_, _, _| false);
        assert_eq!(mesh.indices.len(), 10 * 6);
    }

    #[test]
    fn faces_against_solid_neighbour_chunk_are_skipped() {
        let mut chunk = Chunk::default();
        chunk.set(0, 0, 0, Block::Stone);
        let mesh = build_chunk_mesh(&chunk, |_, _, _| true);
        assert_eq!(mesh.indices.len(), 3 * 6);
    }

    #[test]
    fn empty_chunk_gives_empty_mesh() {
        assert!(build_chunk_mesh(&Chunk::default(), |_, _, _| false).is_empty());
    }
}
