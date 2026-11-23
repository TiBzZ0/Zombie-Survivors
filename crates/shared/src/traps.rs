//! Pièges posés au sol : la mine explose sous un zombie, le piège à mâchoires le retient.

/// Explosion d'une mine : rayon et dégâts au centre.
pub const MINE_RADIUS: f32 = 3.2;
pub const MINE_DAMAGE: f32 = 95.0;
/// Temps pendant lequel le piège à mâchoires retient un zombie (moins pour les géants),
/// dégâts de la morsure, usure du piège à chaque prise.
pub const TRAP_HOLD: f32 = 5.0;
pub const TRAP_HOLD_GIANT: f32 = 1.5;
pub const TRAP_DAMAGE: f32 = 25.0;
pub const TRAP_WEAR: f32 = 20.0;
/// Après s'être libéré, un zombie ne se refait pas prendre tout de suite.
pub const TRAP_IMMUNITY: f32 = 3.0;
