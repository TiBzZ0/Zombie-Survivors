//! Lancer de rayon dans la grille (algorithme DDA) : quel bloc le joueur vise-t-il ?

use bevy_math::{IVec3, Vec3};

/// Bloc touché et face par laquelle le rayon est entré.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RayHit {
    pub block: IVec3,
    /// Normale de la face touchée : poser un bloc se fait en `block + normal`.
    pub normal: IVec3,
}

/// Premier bloc solide le long du rayon, jusqu'à `max_distance`.
pub fn raycast(origin: Vec3, direction: Vec3, max_distance: f32, solid: impl Fn(IVec3) -> bool) -> Option<RayHit> {
    let dir = direction.normalize_or_zero();
    if dir == Vec3::ZERO {
        return None;
    }
    let mut block = origin.floor().as_ivec3();
    let step = IVec3::new(dir.x.signum() as i32, dir.y.signum() as i32, dir.z.signum() as i32);
    let t_delta = Vec3::new(
        if dir.x != 0.0 {
            (1.0 / dir.x).abs()
        } else {
            f32::INFINITY
        },
        if dir.y != 0.0 {
            (1.0 / dir.y).abs()
        } else {
            f32::INFINITY
        },
        if dir.z != 0.0 {
            (1.0 / dir.z).abs()
        } else {
            f32::INFINITY
        },
    );
    let boundary = |o: f32, b: i32, d: f32| {
        if d > 0.0 {
            (b as f32 + 1.0 - o) / d
        } else if d < 0.0 {
            (o - b as f32) / -d
        } else {
            f32::INFINITY
        }
    };
    let mut t_max = Vec3::new(
        boundary(origin.x, block.x, dir.x),
        boundary(origin.y, block.y, dir.y),
        boundary(origin.z, block.z, dir.z),
    );
    let mut normal = IVec3::ZERO;
    let mut t = 0.0;
    while t <= max_distance {
        if solid(block) {
            return Some(RayHit { block, normal });
        }
        if t_max.x < t_max.y && t_max.x < t_max.z {
            block.x += step.x;
            t = t_max.x;
            t_max.x += t_delta.x;
            normal = IVec3::new(-step.x, 0, 0);
        } else if t_max.y < t_max.z {
            block.y += step.y;
            t = t_max.y;
            t_max.y += t_delta.y;
            normal = IVec3::new(0, -step.y, 0);
        } else {
            block.z += step.z;
            t = t_max.z;
            t_max.z += t_delta.z;
            normal = IVec3::new(0, 0, -step.z);
        }
    }
    None
}

/// Distance d'entrée d'un rayon dans une boîte alignée sur les axes, s'il la touche.
/// `dir` doit être normalisé pour que la distance soit en mètres.
pub fn ray_aabb(origin: Vec3, dir: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
    let inv = dir.recip();
    let t1 = (min - origin) * inv;
    let t2 = (max - origin) * inv;
    let near = t1.min(t2).max_element();
    let far = t1.max(t2).min_element();
    (far >= near.max(0.0)).then_some(near.max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hits_block_straight_ahead() {
        let hit = raycast(Vec3::new(0.5, 0.5, 0.5), Vec3::X, 10.0, |p| p == IVec3::new(4, 0, 0)).unwrap();
        assert_eq!(hit.block, IVec3::new(4, 0, 0));
        assert_eq!(hit.normal, IVec3::NEG_X);
    }

    #[test]
    fn hits_ground_looking_down() {
        let hit = raycast(Vec3::new(0.5, 11.6, 0.5), Vec3::new(0.3, -1.0, 0.2), 6.0, |p| p.y < 10).unwrap();
        assert_eq!(hit.block.y, 9);
        assert_eq!(hit.normal, IVec3::Y);
    }

    #[test]
    fn misses_beyond_range() {
        assert!(raycast(Vec3::ZERO, Vec3::X, 3.0, |p| p.x == 10).is_none());
    }

    #[test]
    fn ray_hits_box_in_front_only() {
        let min = Vec3::new(2.0, 0.0, -0.5);
        let max = Vec3::new(3.0, 2.0, 0.5);
        assert_eq!(ray_aabb(Vec3::new(0.0, 1.0, 0.0), Vec3::X, min, max), Some(2.0));
        assert_eq!(ray_aabb(Vec3::new(0.0, 1.0, 0.0), Vec3::NEG_X, min, max), None);
        assert_eq!(ray_aabb(Vec3::new(0.0, 5.0, 0.0), Vec3::X, min, max), None);
    }
}
