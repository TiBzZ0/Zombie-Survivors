//! Transforme un chunk en maillages, un par type de bloc (chacun a sa texture répétée).
//!
//! Faces cachées retirées, puis fusion gloutonne (« greedy meshing ») : les faces voisines
//! d'un même bloc tournées du même côté deviennent un seul rectangle. Un sol plat de
//! 16 × 16 blocs coûte ainsi 1 rectangle au lieu de 256.

use shared::block::Block;
use shared::chunk::{CHUNK_SIZE, Chunk};

/// Données brutes d'un maillage, prêtes à être converties en `Mesh` Bevy.
#[derive(Default, Debug)]
pub struct MeshData {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 4]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

impl MeshData {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Ajoute un rectangle : quatre coins du cube unité, étirés de `extent` et décalés de `offset`.
    /// `uv_size` donne le nombre de répétitions de la texture sur la face.
    fn push_quad(
        &mut self,
        normal: [i32; 3],
        corners: [[f32; 3]; 4],
        map: impl Fn([f32; 3]) -> [f32; 3],
        color: [f32; 4],
        uv_scale: [f32; 2],
    ) {
        let base = self.positions.len() as u32;
        for c in corners {
            // Coordonnées de texture prises dans le plan de la face.
            let (u, v) = match normal {
                [x, 0, 0] if x != 0 => (c[2], c[1]),
                [0, y, 0] if y != 0 => (c[0], c[2]),
                _ => (c[0], c[1]),
            };
            self.uvs.push([u * uv_scale[0], (1.0 - v) * uv_scale[1]]);
            self.positions.push(map(c));
            self.normals.push(normal.map(|n| n as f32));
            self.colors.push(color);
        }
        self.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

/// Maillages d'un chunk, un par type de bloc présent.
#[derive(Default, Debug)]
pub struct ChunkMeshes {
    pub parts: Vec<(Block, MeshData)>,
}

impl ChunkMeshes {
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.parts.iter().all(|(_, m)| m.is_empty())
    }

    fn part(&mut self, block: Block) -> &mut MeshData {
        let i = match self.parts.iter().position(|(b, _)| *b == block) {
            Some(i) => i,
            None => {
                self.parts.push((block, MeshData::default()));
                self.parts.len() - 1
            }
        };
        &mut self.parts[i].1
    }

    /// Nombre total de rectangles.
    #[cfg(test)]
    pub fn quads(&self) -> usize {
        self.parts.iter().map(|(_, m)| m.indices.len() / 6).sum()
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

/// Ombrage fixe par orientation, faute de vraie lumière ambiante.
fn face_color(block: Block, normal: [i32; 3]) -> [f32; 4] {
    let shade = match normal {
        [0, 1, 0] => 1.0,
        [0, -1, 0] => 0.55,
        [_, 0, 0] => 0.8,
        _ => 0.7,
    };
    let [r, g, b] = block.color();
    srgb_to_linear([r * shade, g * shade, b * shade])
}

/// Axes du plan d'une face : (axe de la normale, axe de u, axe de v), comme pour la texture.
fn axes(normal: [i32; 3]) -> (usize, usize, usize) {
    if normal[0] != 0 {
        (0, 2, 1)
    } else if normal[1] != 0 {
        (1, 0, 2)
    } else {
        (2, 0, 1)
    }
}

/// Construit les maillages d'un chunk.
///
/// `solid_outside` répond pour les blocs voisins hors du chunk (coordonnées locales
/// pouvant valoir -1 ou `CHUNK_SIZE`), pour ne pas dessiner les faces collées au chunk d'à côté.
pub fn build_chunk_mesh(chunk: &Chunk, solid_outside: impl Fn(i32, i32, i32) -> bool) -> ChunkMeshes {
    let size = CHUNK_SIZE as i32;
    let solid = |x: i32, y: i32, z: i32| {
        if (0..size).contains(&x) && (0..size).contains(&y) && (0..size).contains(&z) {
            chunk.get(x as usize, y as usize, z as usize).is_opaque()
        } else {
            solid_outside(x, y, z)
        }
    };
    let n = CHUNK_SIZE;
    let mut out = ChunkMeshes::default();

    // Formes partielles (porte ouverte, barbelés, cultures) : toutes leurs faces, sans fusion.
    for y in 0..n {
        for z in 0..n {
            for x in 0..n {
                let block = chunk.get(x, y, z);
                let Some((min, max)) = block.partial_shape() else {
                    continue;
                };
                let part = out.part(block);
                for (normal, corners) in FACES {
                    let offset = [x as f32, y as f32, z as f32];
                    part.push_quad(
                        normal,
                        corners,
                        |c| [0, 1, 2].map(|i| offset[i] + min[i] + c[i] * (max[i] - min[i])),
                        face_color(block, normal),
                        [1.0, 1.0],
                    );
                }
            }
        }
    }

    // Blocs pleins : un masque par tranche et par direction, puis fusion en rectangles.
    let mut mask: Vec<Option<Block>> = vec![None; n * n];
    for (normal, corners) in FACES {
        let (d, u, v) = axes(normal);
        for slice in 0..n {
            for pv in 0..n {
                for pu in 0..n {
                    let mut p = [0usize; 3];
                    p[d] = slice;
                    p[u] = pu;
                    p[v] = pv;
                    let block = chunk.get(p[0], p[1], p[2]);
                    let visible = block != Block::Air
                        && block.partial_shape().is_none()
                        && !solid(
                            p[0] as i32 + normal[0],
                            p[1] as i32 + normal[1],
                            p[2] as i32 + normal[2],
                        );
                    mask[pv * n + pu] = visible.then_some(block);
                }
            }
            for pv in 0..n {
                let mut pu = 0;
                while pu < n {
                    let Some(block) = mask[pv * n + pu] else {
                        pu += 1;
                        continue;
                    };
                    // Largeur, puis hauteur tant que toute la rangée correspond.
                    let mut w = 1;
                    while pu + w < n && mask[pv * n + pu + w] == Some(block) {
                        w += 1;
                    }
                    let mut h = 1;
                    'grow: while pv + h < n {
                        for k in 0..w {
                            if mask[(pv + h) * n + pu + k] != Some(block) {
                                break 'grow;
                            }
                        }
                        h += 1;
                    }
                    for dv in 0..h {
                        for du in 0..w {
                            mask[(pv + dv) * n + pu + du] = None;
                        }
                    }
                    let mut offset = [0.0f32; 3];
                    offset[d] = slice as f32;
                    offset[u] = pu as f32;
                    offset[v] = pv as f32;
                    let mut extent = [1.0f32; 3];
                    extent[u] = w as f32;
                    extent[v] = h as f32;
                    out.part(block).push_quad(
                        normal,
                        corners,
                        |c| [0, 1, 2].map(|i| offset[i] + c[i] * extent[i]),
                        face_color(block, normal),
                        [w as f32, h as f32],
                    );
                    pu += w;
                }
            }
        }
    }
    out.parts.retain(|(_, m)| !m.is_empty());
    out
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
        assert_eq!(mesh.quads(), 6);
        assert_eq!(mesh.parts[0].1.positions.len(), 6 * 4);
    }

    #[test]
    fn touching_blocks_hide_shared_faces() {
        let mut chunk = Chunk::default();
        chunk.set(5, 5, 5, Block::Stone);
        chunk.set(6, 5, 5, Block::Dirt);
        let mesh = build_chunk_mesh(&chunk, |_, _, _| false);
        assert_eq!(mesh.quads(), 10);
        assert_eq!(mesh.parts.len(), 2);
    }

    #[test]
    fn identical_neighbours_merge_into_one_face() {
        let mut chunk = Chunk::default();
        chunk.set(5, 5, 5, Block::Stone);
        chunk.set(6, 5, 5, Block::Stone);
        let mesh = build_chunk_mesh(&chunk, |_, _, _| false);
        assert_eq!(mesh.quads(), 6);
        // La texture se répète deux fois sur la face du dessus.
        let max_u = mesh.parts[0].1.uvs.iter().map(|uv| uv[0]).fold(0.0, f32::max);
        assert_eq!(max_u, 2.0);
    }

    #[test]
    fn flat_floor_is_a_handful_of_quads() {
        let mut chunk = Chunk::default();
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                chunk.set(x, 0, z, Block::Grass);
            }
        }
        // Dessus seulement : le reste touche des voisins pleins.
        let mesh = build_chunk_mesh(&chunk, |_, y, _| y <= 0);
        assert_eq!(mesh.quads(), 1);
        let mesh = build_chunk_mesh(&chunk, |_, _, _| false);
        assert_eq!(mesh.quads(), 6);
    }

    #[test]
    fn faces_against_solid_neighbour_chunk_are_skipped() {
        let mut chunk = Chunk::default();
        chunk.set(0, 0, 0, Block::Stone);
        let mesh = build_chunk_mesh(&chunk, |_, _, _| true);
        assert_eq!(mesh.quads(), 3);
    }

    #[test]
    fn open_door_is_drawn_but_hides_nothing() {
        let mut chunk = Chunk::default();
        chunk.set(5, 5, 5, Block::DoorOpen);
        chunk.set(6, 5, 5, Block::Stone);
        let mesh = build_chunk_mesh(&chunk, |_, _, _| false);
        // 6 faces pour la porte, 6 pour la pierre (rien n'est caché).
        assert_eq!(mesh.quads(), 12);
        let door = &mesh.parts.iter().find(|(b, _)| *b == Block::DoorOpen).unwrap().1;
        let max_x = door.positions.iter().map(|p| p[0]).fold(0.0, f32::max);
        assert!(max_x < 5.2, "le battant doit être fin");
    }

    #[test]
    fn mixed_blocks_never_merge() {
        let mut chunk = Chunk::default();
        for x in 0..4 {
            chunk.set(x, 0, 0, if x % 2 == 0 { Block::Stone } else { Block::Dirt });
        }
        let mesh = build_chunk_mesh(&chunk, |_, _, _| false);
        // Dessus, dessous, avant, arrière : 4 faces par bloc ; plus les 2 bouts.
        assert_eq!(mesh.quads(), 4 * 4 + 2);
    }

    #[test]
    fn empty_chunk_gives_empty_mesh() {
        assert!(build_chunk_mesh(&Chunk::default(), |_, _, _| false).is_empty());
    }
}
