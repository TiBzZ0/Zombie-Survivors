//! Code partagé entre le client et le serveur.
//!
//! Tout ce qui doit donner le même résultat des deux côtés vit ici : types de
//! blocs, stockage des chunks, génération du monde à partir d'une graine,
//! constantes de simulation et de réseau.

pub mod block;
pub mod chunk;
pub mod config;
pub mod worldgen;

/// Fréquence de simulation du serveur, en ticks par seconde.
pub const TICK_RATE_HZ: f64 = 30.0;

/// Port UDP par défaut du serveur dédié.
pub const DEFAULT_PORT: u16 = 7777;
