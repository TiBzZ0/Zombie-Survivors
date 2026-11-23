//! Code partagé entre le client et le serveur.
//!
//! Tout ce qui doit donner le même résultat des deux côtés vit ici : types de
//! blocs, stockage des chunks, génération du monde à partir d'une graine,
//! constantes de simulation et de réseau.

pub mod achievements;
pub mod animal;
pub mod block;
pub mod buff;
pub mod chunk;
pub mod climate;
pub mod config;
pub mod crafting;
pub mod daytime;
pub mod fishing;
pub mod injury;
pub mod item;
pub mod multiblock;
pub mod pathfinding;
pub mod physics;
pub mod protocol;
pub mod raycast;
pub mod rescue;
pub mod skills;
pub mod stamina;
pub mod structure;
pub mod survivor;
pub mod swim;
pub mod traps;
pub mod trees;
pub mod weather;
pub mod world;
pub mod worldgen;
pub mod zombie;

/// Fréquence de simulation du serveur, en ticks par seconde.
pub const TICK_RATE_HZ: f64 = 30.0;

/// Port UDP par défaut du serveur dédié.
pub const DEFAULT_PORT: u16 = 7777;
