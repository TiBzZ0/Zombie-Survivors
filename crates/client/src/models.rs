//! Modèles 3D simples des machines : quelques pavés texturés (pris aux autres blocs) qui
//! remplacent le cube. Coordonnées dans le bloc d'origine, machine couchée le long de X ;
//! les grandes machines débordent sur leurs autres cases (`Block::Part`).

use shared::block::Block;

/// Un pavé du modèle.
#[derive(Clone, Copy, Debug)]
pub struct Piece {
    pub min: [f32; 3],
    pub max: [f32; 3],
    /// Bloc dont la texture habille le pavé.
    pub tex: Block,
    /// Assombrit (< 1) ou éclaircit (> 1) la texture.
    pub tint: f32,
}

const fn p(min: [f32; 3], max: [f32; 3], tex: Block, tint: f32) -> Piece {
    Piece { min, max, tex, tint }
}

use Block::*;

const WORKBENCH: &[Piece] = &[
    // Plateau sur deux cases, pieds, étagère basse, étau et outils posés.
    p([0.03, 0.78, 0.06], [1.97, 0.95, 0.94], Workbench, 1.0),
    p([0.08, 0.0, 0.1], [0.22, 0.78, 0.24], Wood, 0.9),
    p([0.08, 0.0, 0.76], [0.22, 0.78, 0.9], Wood, 0.9),
    p([1.78, 0.0, 0.1], [1.92, 0.78, 0.24], Wood, 0.9),
    p([1.78, 0.0, 0.76], [1.92, 0.78, 0.9], Wood, 0.9),
    p([0.15, 0.28, 0.15], [1.85, 0.36, 0.85], Planks, 0.85),
    p([1.55, 0.95, 0.3], [1.85, 1.15, 0.7], ReinforcedWall, 1.0),
    p([1.62, 1.15, 0.45], [1.78, 1.22, 0.55], ReinforcedWall, 1.2),
    p([0.3, 0.95, 0.35], [0.95, 1.0, 0.45], ReinforcedWall, 1.3),
    p([0.35, 0.95, 0.55], [0.45, 1.05, 0.85], Wood, 1.0),
];

const FORGE: &[Piece] = &[
    // Foyer en pierre, hotte et cheminée, enclume devant.
    p([0.0, 0.0, 0.05], [1.0, 0.9, 0.95], Forge, 1.0),
    p([0.1, 0.9, 0.15], [0.9, 1.25, 0.85], Stone, 0.75),
    p([0.3, 1.25, 0.3], [0.7, 1.95, 0.7], Stone, 0.6),
    p([0.25, 0.9, -0.3], [0.75, 0.98, 0.05], ReinforcedWall, 0.5),
];

const TURRET: &[Piece] = &[
    // Socle, fût, caisson à meurtrière, double canon et caisse de munitions.
    p([0.1, 0.0, 0.1], [0.9, 0.15, 0.9], ReinforcedWall, 0.8),
    p([0.35, 0.15, 0.35], [0.65, 0.85, 0.65], ReinforcedWall, 0.7),
    p([0.15, 0.85, 0.15], [0.85, 1.45, 0.85], Turret, 1.0),
    p([0.3, 1.08, -0.4], [0.42, 1.2, 0.15], Cannon, 0.5),
    p([0.58, 1.08, -0.4], [0.7, 1.2, 0.15], Cannon, 0.5),
    p([0.85, 0.95, 0.3], [1.0, 1.25, 0.7], Chest, 0.9),
    p([0.2, 1.45, 0.2], [0.8, 1.52, 0.8], ReinforcedWall, 1.1),
];

const CANNON: &[Piece] = &[
    // Affût en planches, quatre roues, fût noir et bouche renforcée.
    p([0.1, 0.18, 0.15], [1.9, 0.45, 0.85], Planks, 0.85),
    p([0.25, 0.0, 0.02], [0.75, 0.55, 0.14], Wood, 0.8),
    p([0.25, 0.0, 0.86], [0.75, 0.55, 0.98], Wood, 0.8),
    p([1.25, 0.0, 0.02], [1.75, 0.55, 0.14], Wood, 0.8),
    p([1.25, 0.0, 0.86], [1.75, 0.55, 0.98], Wood, 0.8),
    p([0.05, 0.45, 0.28], [1.85, 0.85, 0.72], Cannon, 0.6),
    p([1.85, 0.4, 0.22], [2.0, 0.9, 0.78], Cannon, 0.45),
    p([0.0, 0.55, 0.42], [0.08, 0.7, 0.58], ReinforcedWall, 0.6),
];

const CATAPULT: &[Piece] = &[
    // Châssis, montants, axe, bras, cuillère et contrepoids.
    p([0.0, 0.0, 0.1], [2.0, 0.22, 0.28], Wood, 0.85),
    p([0.0, 0.0, 0.72], [2.0, 0.22, 0.9], Wood, 0.85),
    p([0.1, 0.22, 0.1], [0.3, 0.32, 0.9], Planks, 0.9),
    p([1.7, 0.22, 0.1], [1.9, 0.32, 0.9], Planks, 0.9),
    p([0.9, 0.22, 0.1], [1.1, 1.2, 0.26], Wood, 0.9),
    p([0.9, 0.22, 0.74], [1.1, 1.2, 0.9], Wood, 0.9),
    p([0.92, 1.02, 0.1], [1.08, 1.14, 0.9], ReinforcedWall, 0.7),
    p([0.2, 1.05, 0.42], [1.8, 1.17, 0.58], Wood, 1.0),
    p([0.02, 1.1, 0.3], [0.4, 1.3, 0.7], Planks, 1.0),
    p([1.55, 0.6, 0.3], [1.95, 1.05, 0.7], Stone, 0.8),
];

const FLOODLIGHT: &[Piece] = &[
    // Pied, mât et tête de projecteur.
    p([0.25, 0.0, 0.25], [0.75, 0.12, 0.75], ReinforcedWall, 0.8),
    p([0.43, 0.12, 0.43], [0.57, 1.45, 0.57], ReinforcedWall, 0.7),
    p([0.15, 1.4, 0.2], [0.85, 1.9, 0.75], ReinforcedWall, 0.6),
    p([0.2, 1.45, 0.05], [0.8, 1.85, 0.2], Floodlight, 1.2),
];

const TESLA: &[Piece] = &[
    // Socle, bobinage cuivré, anneau et sphère.
    p([0.15, 0.0, 0.15], [0.85, 0.3, 0.85], ReinforcedWall, 0.8),
    p([0.3, 0.3, 0.3], [0.7, 1.45, 0.7], TeslaCoil, 1.0),
    p([0.08, 1.45, 0.08], [0.92, 1.65, 0.92], ReinforcedWall, 1.15),
    p([0.32, 1.65, 0.32], [0.68, 1.95, 0.68], ReinforcedWall, 1.3),
];

const MACHINE_GUN: &[Piece] = &[
    // Trépied, culasse, canon et caisse de cartouches.
    p([0.15, 0.0, 0.15], [0.85, 0.1, 0.85], ReinforcedWall, 0.7),
    p([0.42, 0.1, 0.42], [0.58, 0.55, 0.58], ReinforcedWall, 0.6),
    p([0.3, 0.55, 0.2], [0.7, 0.82, 0.85], MachineGun, 1.0),
    p([0.44, 0.62, -0.45], [0.56, 0.74, 0.2], Cannon, 0.5),
    p([0.7, 0.5, 0.35], [0.92, 0.75, 0.7], Chest, 0.8),
    p([0.46, 0.82, 0.6], [0.54, 0.95, 0.75], Cannon, 0.6),
];

const CHEST: &[Piece] = &[
    // Caisse, couvercle et fermoir.
    p([0.06, 0.0, 0.06], [0.94, 0.6, 0.94], Chest, 1.0),
    p([0.04, 0.6, 0.04], [0.96, 0.85, 0.96], Chest, 0.85),
    p([0.44, 0.48, 0.0], [0.56, 0.7, 0.04], ReinforcedWall, 1.3),
];

const GENERATOR: &[Piece] = &[
    // Carter, réservoir, pot d'échappement.
    p([0.05, 0.0, 0.15], [0.95, 0.7, 0.85], Generator, 1.0),
    p([0.15, 0.7, 0.25], [0.6, 0.85, 0.75], Generator, 0.8),
    p([0.7, 0.7, 0.3], [0.85, 1.05, 0.45], Cannon, 0.6),
    p([0.0, 0.1, 0.2], [0.05, 0.5, 0.8], ReinforcedWall, 0.7),
];

/// Modèle d'un bloc dessiné autrement qu'en cube.
pub fn model(block: Block) -> Option<&'static [Piece]> {
    Some(match block {
        Workbench => WORKBENCH,
        Forge => FORGE,
        Turret => TURRET,
        Cannon => CANNON,
        Catapult => CATAPULT,
        Floodlight => FLOODLIGHT,
        TeslaCoil => TESLA,
        MachineGun => MACHINE_GUN,
        Chest => CHEST,
        Generator => GENERATOR,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn models_stay_within_their_footprint() {
        for block in crate::textures::all_blocks() {
            let Some(pieces) = model(block) else { continue };
            let wide = shared::multiblock::footprint(block)
                .iter()
                .fold([1.0f32; 3], |mut size, o| {
                    size[0] = size[0].max(o.x as f32 + 1.0);
                    size[1] = size[1].max(o.y as f32 + 1.0);
                    size[2] = size[2].max(o.z as f32 + 1.0);
                    size
                });
            for piece in pieces {
                for axis in 0..3 {
                    assert!(piece.min[axis] < piece.max[axis], "{block:?}");
                    // Un canon ou un étau peut dépasser un peu de la case, pas plus d'un demi-bloc.
                    assert!(
                        piece.min[axis] >= -0.5 && piece.max[axis] <= wide[axis] + 0.3,
                        "{block:?}"
                    );
                }
            }
        }
    }
}
