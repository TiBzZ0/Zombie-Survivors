//! Champ de flux (flow field) pour les hordes.
//!
//! Un seul calcul de Dijkstra, lancé depuis les joueurs, donne à chaque position
//! accessible la case suivante à prendre pour se rapprocher d'eux. Tous les zombies
//! lisent le même champ : le coût ne dépend pas du nombre de zombies.
//!
//! Les murs ne sont pas infranchissables : traverser un bloc coûte en proportion de
//! sa résistance. Si le détour est trop long, la horde choisit de casser le mur.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use bevy_math::IVec3;

/// Déplacements horizontaux possibles (4 directions).
const DIRS: [IVec3; 4] = [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z];
/// Hauteur de chute maximale acceptée par un zombie.
const MAX_DROP: i32 = 3;
/// Coût ajouté par point de vie d'un bloc à casser (1 = une case de marche).
const COST_PER_BLOCK_HEALTH: f32 = 1.0 / 12.0;

/// Une case du champ.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlowNode {
    /// Coût restant jusqu'au joueur le plus proche.
    pub cost: f32,
    /// Case (position des pieds) où aller ensuite.
    pub next: IVec3,
    /// Bloc à casser avant de pouvoir avancer vers `next`.
    pub breaks: Option<IVec3>,
}

/// Champ de flux autour des joueurs.
#[derive(Default, Debug, Clone)]
pub struct FlowField {
    nodes: HashMap<IVec3, FlowNode>,
}

impl FlowField {
    pub fn get(&self, feet: IVec3) -> Option<&FlowNode> {
        self.nodes.get(&feet)
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

#[derive(PartialEq)]
struct Entry {
    cost: f32,
    pos: IVec3,
}

impl Eq for Entry {}

impl Ord for Entry {
    fn cmp(&self, other: &Self) -> Ordering {
        // Tas min : le plus petit coût d'abord.
        other.cost.partial_cmp(&self.cost).unwrap_or(Ordering::Equal)
    }
}

impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Construit le champ depuis `targets` (pieds des joueurs).
///
/// - `solid(p)` : le bloc est-il solide ;
/// - `health(p)` : points de vie d'un bloc solide (coût pour le casser), `None` s'il est incassable ;
/// - `radius` : distance horizontale maximale explorée autour des cibles ;
/// - `max_nodes` : limite de sécurité sur la taille du champ.
pub fn build_flow_field(
    targets: &[IVec3],
    radius: i32,
    max_nodes: usize,
    solid: impl Fn(IVec3) -> bool,
    health: impl Fn(IVec3) -> Option<f32>,
) -> FlowField {
    let standable = |p: IVec3| solid(p - IVec3::Y) && !solid(p) && !solid(p + IVec3::Y);
    let near_target = |p: IVec3| {
        targets
            .iter()
            .any(|t| (p.x - t.x).abs() <= radius && (p.z - t.z).abs() <= radius)
    };

    let mut nodes: HashMap<IVec3, FlowNode> = HashMap::new();
    let mut heap = BinaryHeap::new();
    for &t in targets {
        nodes.insert(
            t,
            FlowNode {
                cost: 0.0,
                next: t,
                breaks: None,
            },
        );
        heap.push(Entry { cost: 0.0, pos: t });
    }

    let relax = |nodes: &mut HashMap<IVec3, FlowNode>, heap: &mut BinaryHeap<Entry>, from: IVec3, node: FlowNode| {
        if !near_target(from) {
            return;
        }
        let better = nodes.get(&from).is_none_or(|n| node.cost < n.cost);
        if better {
            nodes.insert(from, node);
            heap.push(Entry {
                cost: node.cost,
                pos: from,
            });
        }
    };

    while let Some(Entry { cost, pos }) = heap.pop() {
        if nodes.len() >= max_nodes {
            break;
        }
        if nodes.get(&pos).is_some_and(|n| cost > n.cost) {
            continue;
        }
        for dir in DIRS {
            // Qui peut venir jusqu'à `pos` depuis la colonne voisine `pos - dir` ?
            let side = pos - dir;

            // Même hauteur.
            if standable(side) {
                relax(
                    &mut nodes,
                    &mut heap,
                    side,
                    FlowNode {
                        cost: cost + 1.0,
                        next: pos,
                        breaks: None,
                    },
                );
            }
            // Depuis une case plus basse d'un bloc : saut (il faut de la place au-dessus de la tête).
            let below = side - IVec3::Y;
            if standable(below) && !solid(below + IVec3::Y * 2) {
                relax(
                    &mut nodes,
                    &mut heap,
                    below,
                    FlowNode {
                        cost: cost + 1.5,
                        next: pos,
                        breaks: None,
                    },
                );
            }
            // Depuis plus haut : chute de 1 à MAX_DROP blocs (la colonne au-dessus de `pos` doit être libre).
            for h in 1..=MAX_DROP {
                if solid(pos + IVec3::Y * (h + 1)) {
                    break;
                }
                let above = side + IVec3::Y * h;
                if standable(above) {
                    relax(
                        &mut nodes,
                        &mut heap,
                        above,
                        FlowNode {
                            cost: cost + 1.0 + 0.2 * h as f32,
                            next: pos,
                            breaks: None,
                        },
                    );
                    break;
                }
            }
            // Mur entre `pos` et la case d'après : le zombie en `side - dir` casse `side` puis passe.
            if !standable(side) && solid(side - IVec3::Y) && (solid(side) || solid(side + IVec3::Y)) {
                let mut wall_cost = 0.0;
                let mut first = None;
                let mut breakable = true;
                for b in [side, side + IVec3::Y] {
                    if solid(b) {
                        match health(b) {
                            Some(hp) => {
                                wall_cost += hp * COST_PER_BLOCK_HEALTH;
                                first.get_or_insert(b);
                            }
                            None => breakable = false,
                        }
                    }
                }
                let attacker = side - dir;
                if breakable && standable(attacker) {
                    // La case du mur elle-même, pour le zombie qui y entre juste après l'avoir cassé.
                    nodes.entry(side).or_insert(FlowNode {
                        cost: cost + 1.0,
                        next: pos,
                        breaks: None,
                    });
                    relax(
                        &mut nodes,
                        &mut heap,
                        attacker,
                        FlowNode {
                            cost: cost + 2.0 + wall_cost,
                            next: side,
                            breaks: first,
                        },
                    );
                }
            }
        }
    }
    FlowField { nodes }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn follow(field: &FlowField, mut pos: IVec3, steps: usize) -> (IVec3, Vec<IVec3>) {
        let mut broken = Vec::new();
        for _ in 0..steps {
            let Some(node) = field.get(pos) else { break };
            if let Some(b) = node.breaks {
                broken.push(b);
            }
            if node.next == pos {
                break;
            }
            pos = node.next;
        }
        (pos, broken)
    }

    #[test]
    fn leads_to_the_player_on_flat_ground() {
        let solid = |p: IVec3| p.y < 10;
        let target = IVec3::new(0, 10, 0);
        let field = build_flow_field(&[target], 20, 100_000, solid, |_| Some(10.0));
        let (end, broken) = follow(&field, IVec3::new(8, 10, -5), 100);
        assert_eq!(end, target);
        assert!(broken.is_empty());
        assert_eq!(field.get(IVec3::new(8, 10, -5)).unwrap().cost, 13.0);
    }

    #[test]
    fn climbs_steps_and_drops_down() {
        // Une marche d'un bloc en x >= 3.
        let solid = |p: IVec3| p.y < 10 || (p.x >= 3 && p.y < 11);
        let field = build_flow_field(&[IVec3::new(6, 11, 0)], 20, 100_000, solid, |_| Some(10.0));
        let (end, _) = follow(&field, IVec3::new(0, 10, 0), 50);
        assert_eq!(end, IVec3::new(6, 11, 0));
        // Et l'inverse : descendre.
        let field = build_flow_field(&[IVec3::new(0, 10, 0)], 20, 100_000, solid, |_| Some(10.0));
        let (end, _) = follow(&field, IVec3::new(6, 11, 0), 50);
        assert_eq!(end, IVec3::new(0, 10, 0));
    }

    #[test]
    fn breaks_a_wall_around_an_enclosed_base() {
        // Le joueur est enfermé dans une boîte de 2 blocs de haut entre x et z de -2 à 2.
        let wall = |p: IVec3| {
            (p.x.abs() == 2 && p.z.abs() <= 2 || p.z.abs() == 2 && p.x.abs() <= 2) && (10..12).contains(&p.y)
        };
        let solid = move |p: IVec3| p.y < 10 || wall(p);
        let field = build_flow_field(&[IVec3::new(0, 10, 0)], 20, 100_000, solid, |_| Some(30.0));
        let (end, broken) = follow(&field, IVec3::new(10, 10, 0), 100);
        assert_eq!(end, IVec3::new(0, 10, 0));
        assert_eq!(broken.len(), 1, "un seul mur à casser");
        assert!(wall(broken[0]));
    }

    #[test]
    fn prefers_a_short_detour_to_a_strong_wall() {
        // Un mur en x = 3 avec une ouverture en z = 2.
        let solid = |p: IVec3| p.y < 10 || (p.x == 3 && (10..12).contains(&p.y) && p.z != 2);
        let field = build_flow_field(&[IVec3::new(6, 10, 0)], 20, 100_000, solid, |_| Some(500.0));
        let (end, broken) = follow(&field, IVec3::new(0, 10, 0), 100);
        assert_eq!(end, IVec3::new(6, 10, 0));
        assert!(broken.is_empty());
    }

    #[test]
    fn unbreakable_blocks_are_never_targeted() {
        let solid = |p: IVec3| p.y < 10 || (p.x == 3 && (10..14).contains(&p.y));
        let field = build_flow_field(&[IVec3::new(6, 10, 0)], 8, 100_000, solid, |_| None);
        let (_, broken) = follow(&field, IVec3::new(0, 10, 0), 100);
        assert!(broken.is_empty());
    }
}
