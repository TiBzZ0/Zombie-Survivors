//! Configuration du serveur, lue dans les variables d'environnement (pratique avec Docker).

use std::path::PathBuf;

use bevy::prelude::Resource;
use shared::config::WorldConfig;

#[derive(Resource, Clone, Debug)]
pub struct ServerConfig {
    pub port: u16,
    pub max_players: u8,
    /// Dossier des sauvegardes (monté en volume dans Docker).
    pub data_dir: PathBuf,
    pub world: WorldConfig,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: shared::DEFAULT_PORT,
            max_players: 8,
            data_dir: PathBuf::from("data"),
            world: WorldConfig::default(),
        }
    }
}

impl ServerConfig {
    /// Configuration d'une partie locale lancée par le client : sauvegarde dans le dossier
    /// de données de l'utilisateur, pour la retrouver d'une session à l'autre.
    pub fn local() -> Self {
        Self {
            data_dir: user_data_dir(),
            ..Self::default()
        }
    }

    /// Lit `PORT`, `MAX_PLAYERS`, `DATA_DIR`, `WORLD_SEED` et `WORLD_SIZE_CHUNKS`.
    /// Une variable absente garde sa valeur par défaut ; une valeur invalide est une erreur.
    pub fn from_env() -> Result<Self, String> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        fn parse<T: std::str::FromStr>(key: &str, value: Option<String>, default: T) -> Result<T, String> {
            match value {
                None => Ok(default),
                Some(v) => v.trim().parse().map_err(|_| format!("{key} invalide : « {v} »")),
            }
        }
        let d = Self::default();
        let config = Self {
            port: parse("PORT", get("PORT"), d.port)?,
            max_players: parse("MAX_PLAYERS", get("MAX_PLAYERS"), d.max_players)?,
            data_dir: get("DATA_DIR").map(PathBuf::from).unwrap_or(d.data_dir),
            world: WorldConfig {
                seed: parse("WORLD_SEED", get("WORLD_SEED"), d.world.seed)?,
                size_chunks: parse("WORLD_SIZE_CHUNKS", get("WORLD_SIZE_CHUNKS"), d.world.size_chunks)?,
                height_chunks: d.world.height_chunks,
                generation: d.world.generation,
            },
        };
        if !(1..=8).contains(&config.max_players) {
            return Err("MAX_PLAYERS doit être entre 1 et 8".into());
        }
        if !(4..=256).contains(&config.world.size_chunks) {
            return Err("WORLD_SIZE_CHUNKS doit être entre 4 et 256".into());
        }
        Ok(config)
    }
}

/// Dossier de données de l'utilisateur, selon le système.
fn user_data_dir() -> PathBuf {
    let env = |key: &str| std::env::var_os(key).filter(|v| !v.is_empty()).map(PathBuf::from);
    let dir = if cfg!(windows) {
        env("APPDATA").map(|d| d.join("ZombieSurvivors"))
    } else if cfg!(target_os = "macos") {
        env("HOME").map(|h| h.join("Library/Application Support/ZombieSurvivors"))
    } else {
        env("XDG_DATA_HOME")
            .or_else(|| env("HOME").map(|h| h.join(".local/share")))
            .map(|d| d.join("zombie-survivors"))
    };
    dir.unwrap_or_else(|| PathBuf::from("data")).join("local")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_variables_keep_defaults() {
        let config = ServerConfig::from_lookup(|_| None).unwrap();
        assert_eq!(config.port, shared::DEFAULT_PORT);
        assert_eq!(config.world, WorldConfig::default());
    }

    #[test]
    fn variables_override_defaults() {
        let config = ServerConfig::from_lookup(|key| match key {
            "MAX_PLAYERS" => Some("4".into()),
            "WORLD_SEED" => Some("-12".into()),
            _ => None,
        })
        .unwrap();
        assert_eq!(config.max_players, 4);
        assert_eq!(config.world.seed, -12);
    }

    #[test]
    fn invalid_values_are_rejected() {
        assert!(ServerConfig::from_lookup(|k| (k == "PORT").then(|| "abc".into())).is_err());
        assert!(ServerConfig::from_lookup(|k| (k == "MAX_PLAYERS").then(|| "20".into())).is_err());
    }
}
