//! Déplacement d'une boîte (joueur, zombie) contre la grille de blocs.
//!
//! Collisions AABB axe par axe : simple, stable et bien assez rapide pour des
//! centaines d'entités, sans moteur physique.

use bevy_math::{IVec3, Vec3};

/// Gravité, en m/s².
pub const GRAVITY: f32 = 28.0;
/// Vitesse de chute maximale, en m/s.
pub const TERMINAL_VELOCITY: f32 = 50.0;

/// Taille d'une entité : demi-largeur et hauteur. La position est au milieu des pieds.
#[derive(Clone, Copy, Debug)]
pub struct BodySize {
    pub half_width: f32,
    pub height: f32,
}

pub const PLAYER_SIZE: BodySize = BodySize {
    half_width: 0.3,
    height: 1.8,
};
/// Hauteur des yeux du joueur au-dessus de ses pieds.
pub const PLAYER_EYE_HEIGHT: f32 = 1.62;

/// Résultat d'un déplacement.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MoveResult {
    pub position: Vec3,
    pub velocity: Vec3,
    pub on_ground: bool,
    /// Vrai si un mur a bloqué le mouvement horizontal (un zombie s'en sert pour taper ou sauter).
    pub hit_wall: bool,
}

/// Vrai si la boîte posée en `pos` touche un bloc solide.
pub fn overlaps_solid(pos: Vec3, size: BodySize, solid: &impl Fn(IVec3) -> bool) -> bool {
    let eps = 1e-4;
    let min = Vec3::new(
        pos.x - size.half_width + eps,
        pos.y + eps,
        pos.z - size.half_width + eps,
    );
    let max = Vec3::new(
        pos.x + size.half_width - eps,
        pos.y + size.height - eps,
        pos.z + size.half_width - eps,
    );
    let (lo, hi) = (min.floor().as_ivec3(), max.floor().as_ivec3());
    for x in lo.x..=hi.x {
        for y in lo.y..=hi.y {
            for z in lo.z..=hi.z {
                if solid(IVec3::new(x, y, z)) {
                    return true;
                }
            }
        }
    }
    false
}

/// Applique la gravité puis déplace la boîte de `velocity * dt` en glissant le long des blocs.
pub fn step_body(
    position: Vec3,
    mut velocity: Vec3,
    size: BodySize,
    dt: f32,
    solid: &impl Fn(IVec3) -> bool,
) -> MoveResult {
    velocity.y = (velocity.y - GRAVITY * dt).max(-TERMINAL_VELOCITY);
    let mut pos = position;
    let mut on_ground = false;
    let mut hit_wall = false;

    // Découpe en sous-pas pour ne jamais traverser un bloc à grande vitesse.
    let delta = velocity * dt;
    let steps = (delta.abs().max_element() / 0.4).ceil().max(1.0) as usize;
    let sub = delta / steps as f32;

    for _ in 0..steps {
        for axis in 0..3 {
            let mut next = pos;
            next[axis] += sub[axis];
            if overlaps_solid(next, size, solid) {
                if axis == 1 {
                    if sub.y < 0.0 {
                        on_ground = true;
                        // Colle les pieds sur le dessus du bloc.
                        next.y = next.y.floor() + 1.0;
                        if overlaps_solid(next, size, solid) {
                            next.y = pos.y;
                        }
                        pos.y = next.y;
                    }
                    velocity.y = 0.0;
                } else {
                    hit_wall = true;
                    velocity[axis] = 0.0;
                }
            } else {
                pos = next;
            }
        }
    }
    MoveResult {
        position: pos,
        velocity,
        on_ground,
        hit_wall,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sol plat : tout ce qui est sous y = 10 est solide.
    fn flat(p: IVec3) -> bool {
        p.y < 10
    }

    #[test]
    fn falls_and_lands_on_the_ground() {
        let mut pos = Vec3::new(0.5, 15.0, 0.5);
        let mut vel = Vec3::ZERO;
        let mut grounded = false;
        for _ in 0..120 {
            let r = step_body(pos, vel, PLAYER_SIZE, 1.0 / 30.0, &flat);
            pos = r.position;
            vel = r.velocity;
            grounded = r.on_ground;
        }
        assert!(grounded);
        assert!((pos.y - 10.0).abs() < 1e-3, "pieds à {}", pos.y);
    }

    #[test]
    fn walls_stop_horizontal_movement() {
        // Un mur en x = 3.
        let wall = |p: IVec3| p.y < 10 || p.x == 3;
        let mut pos = Vec3::new(0.5, 10.0, 0.5);
        let mut hit = false;
        for _ in 0..60 {
            let r = step_body(pos, Vec3::new(5.0, 0.0, 0.0), PLAYER_SIZE, 1.0 / 30.0, &wall);
            pos = r.position;
            hit |= r.hit_wall;
        }
        assert!(hit);
        assert!(pos.x <= 3.0 - PLAYER_SIZE.half_width + 1e-3);
    }

    #[test]
    fn high_speed_does_not_tunnel_through_floor() {
        let r = step_body(
            Vec3::new(0.5, 11.0, 0.5),
            Vec3::new(0.0, -200.0, 0.0),
            PLAYER_SIZE,
            0.1,
            &flat,
        );
        assert!(r.position.y >= 10.0 - 1e-3);
    }
}
