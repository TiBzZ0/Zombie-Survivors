//! Recherche de chemin à pied (A*) pour les survivants : marcher, monter une marche d'un bloc
//! en sautant, descendre jusqu'à trois blocs, nager.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use bevy::math::{IVec3, Vec3};
use shared::block::Block;

/// Nombre maximal de cases explorées par recherche (borne le coût).
pub const MAX_NODES: usize = 1500;
/// Hauteur de chute acceptée.
const MAX_DROP: i32 = 3;

/// Une case où un corps de deux blocs de haut peut se tenir.
fn standable(p: IVec3, block_at: &impl Fn(IVec3) -> Block) -> bool {
    let free = |q: IVec3| !block_at(q).is_solid();
    if !free(p) || !free(p + IVec3::Y) {
        return false;
    }
    let below = block_at(p - IVec3::Y);
    below.is_solid() || block_at(p).is_water() || below.is_water()
}

/// Cherche un point d'appui dans la colonne voisine : une marche, le même niveau ou une descente.
fn landing(from: IVec3, column: IVec3, block_at: &impl Fn(IVec3) -> Block) -> Option<IVec3> {
    // Monter d'un bloc demande de la place au-dessus de la tête pour sauter.
    let up = IVec3::new(column.x, from.y + 1, column.z);
    if standable(up, block_at) && !block_at(from + IVec3::Y * 2).is_solid() {
        return Some(up);
    }
    for dy in 0..=MAX_DROP {
        let p = IVec3::new(column.x, from.y - dy, column.z);
        if dy > 0 && block_at(p + IVec3::Y * 2).is_solid() {
            return None; // un plafond bloque la descente
        }
        if standable(p, block_at) {
            return Some(p);
        }
    }
    None
}

#[derive(PartialEq)]
struct Open {
    f: f32,
    pos: IVec3,
}

impl Eq for Open {}

impl Ord for Open {
    fn cmp(&self, other: &Self) -> Ordering {
        other.f.total_cmp(&self.f)
    }
}

impl PartialOrd for Open {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Case de départ : celle des pieds, ou juste au-dessus si les pieds sont dans une marche.
fn start_cell(feet: Vec3, block_at: &impl Fn(IVec3) -> Block) -> IVec3 {
    let p = feet.floor().as_ivec3();
    [p, p + IVec3::Y, p - IVec3::Y]
        .into_iter()
        .find(|q| standable(*q, block_at))
        .unwrap_or(p)
}

/// Chemin de `from` vers `to` (pieds), sous forme de centres de cases à rejoindre dans l'ordre.
/// Si le but est hors d'atteinte, mène au plus près. `None` si rien ne rapproche.
pub fn find_path(from: Vec3, to: Vec3, block_at: impl Fn(IVec3) -> Block) -> Option<Vec<Vec3>> {
    let start = start_cell(from, &block_at);
    let goal = to.floor().as_ivec3();
    let h = |p: IVec3| (p - goal).as_vec3().length();
    let mut open = BinaryHeap::new();
    let mut came: HashMap<IVec3, IVec3> = HashMap::new();
    let mut cost: HashMap<IVec3, f32> = HashMap::new();
    cost.insert(start, 0.0);
    open.push(Open {
        f: h(start),
        pos: start,
    });
    let mut best = (h(start), start);
    let mut explored = 0;
    while let Some(Open { pos, .. }) = open.pop() {
        let d = h(pos);
        if d < best.0 {
            best = (d, pos);
        }
        // Arrivé : même colonne (ou presque) et même hauteur à un bloc près.
        if pos.x == goal.x && pos.z == goal.z && (pos.y - goal.y).abs() <= 1 {
            best = (0.0, pos);
            break;
        }
        explored += 1;
        if explored > MAX_NODES {
            break;
        }
        let g = cost[&pos];
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
            let column = pos + IVec3::new(dx, 0, dz);
            let diagonal = dx != 0 && dz != 0;
            // En diagonale, les deux côtés doivent être libres (pas de coin coupé).
            if diagonal
                && !(landing(pos, pos + IVec3::new(dx, 0, 0), &block_at).is_some_and(|p| p.y == pos.y)
                    && landing(pos, pos + IVec3::new(0, 0, dz), &block_at).is_some_and(|p| p.y == pos.y))
            {
                continue;
            }
            let Some(next) = landing(pos, column, &block_at) else {
                continue;
            };
            if diagonal && next.y != pos.y {
                continue;
            }
            let step = if diagonal { 1.414 } else { 1.0 } + if next.y > pos.y { 0.6 } else { 0.0 };
            let swim = if block_at(next).is_water() { 0.8 } else { 0.0 };
            let ng = g + step + swim;
            if cost.get(&next).is_none_or(|c| ng < *c) {
                cost.insert(next, ng);
                came.insert(next, pos);
                open.push(Open {
                    f: ng + h(next),
                    pos: next,
                });
            }
        }
    }
    if best.1 == start {
        return None;
    }
    let mut path = vec![best.1];
    let mut cur = best.1;
    while let Some(prev) = came.get(&cur) {
        if *prev == start {
            break;
        }
        path.push(*prev);
        cur = *prev;
    }
    path.reverse();
    Some(
        path.into_iter()
            .map(|p| p.as_vec3() + Vec3::new(0.5, 0.0, 0.5))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sol plat à y = 0 (pieds à y = 1), avec ce que `extra` ajoute.
    fn world(extra: impl Fn(IVec3) -> Option<Block>) -> impl Fn(IVec3) -> Block {
        move |p| extra(p).unwrap_or(if p.y <= 0 { Block::Stone } else { Block::Air })
    }

    #[test]
    fn climbs_a_one_block_step() {
        // Une estrade d'un bloc de haut à partir de x = 3.
        let w = world(|p| (p.x >= 3 && p.y == 1).then_some(Block::Stone));
        let path = find_path(Vec3::new(0.5, 1.0, 0.5), Vec3::new(6.5, 2.0, 0.5), w).unwrap();
        let last = *path.last().unwrap();
        assert_eq!(last.floor().as_ivec3(), IVec3::new(6, 2, 0));
        assert!(path.iter().any(|p| p.y == 2.0));
    }

    #[test]
    fn goes_around_a_wall_too_high_to_climb() {
        // Un mur de trois blocs en x = 3, de z = -5 à z = 2.
        let w = world(|p| (p.x == 3 && (1..=3).contains(&p.y) && (-5..=2).contains(&p.z)).then_some(Block::Stone));
        let path = find_path(Vec3::new(0.5, 1.0, 0.5), Vec3::new(6.5, 1.0, 0.5), w).unwrap();
        assert_eq!(path.last().unwrap().floor().as_ivec3(), IVec3::new(6, 1, 0));
        assert!(path.iter().any(|p| p.z > 2.0));
    }

    #[test]
    fn an_unreachable_goal_leads_as_close_as_possible() {
        // Le but est enfermé dans une cage.
        let w = world(|p| {
            let inside = (9..=11).contains(&p.x) && (-1..=1).contains(&p.z);
            let wall = inside && !(p.x == 10 && p.z == 0);
            (wall && (1..=4).contains(&p.y)).then_some(Block::Stone)
        });
        let path = find_path(Vec3::new(0.5, 1.0, 0.5), Vec3::new(10.5, 1.0, 0.5), w).unwrap();
        let end = *path.last().unwrap();
        assert!(end.distance(Vec3::new(10.5, 1.0, 0.5)) <= 2.0);
    }
}
