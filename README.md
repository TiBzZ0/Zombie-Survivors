# Zombie Survivors

Jeu de survie coopératif en voxel, à la première personne, écrit en Rust avec Bevy : récolter des ressources, construire sa base et tenir face à des hordes de zombies. De 2 à 8 joueurs sur un serveur dédié (Docker) ou en mode local. Nom provisoire.

## Organisation

| Crate | Rôle |
| --- | --- |
| `crates/shared` | Code commun : blocs, chunks de 32³, génération du monde depuis une graine, constantes |
| `crates/server` | Serveur autoritaire sans rendu, simulation à 30 ticks/s. Binaire pour Docker, bibliothèque pour le mode local |
| `crates/client` | Rendu, entrées, interface. Lance le serveur dans un thread en mode local |

## Lancer le jeu en local

Prérequis : Rust stable récent (1.97.1 minimum, exigé par Bevy 0.20).

```sh
cargo run -p client --release
```

Pour l'instant, une caméra libre survole le terrain : clic droit maintenu ou `M` pour capturer la souris, `ZQSD`/`WASD` pour avancer, `A`/`E` pour monter et descendre, `Maj` pour aller plus vite.

Sous Linux, Bevy demande `libasound2-dev`, `libudev-dev`, `libwayland-dev` et `libxkbcommon-dev`.

## Serveur dédié

```sh
docker compose up -d
```

Variables d'environnement :

| Variable | Défaut | Rôle |
| --- | --- | --- |
| `PORT` | `7777` | Port UDP |
| `MAX_PLAYERS` | `8` | Joueurs maximum (1 à 8) |
| `WORLD_SEED` | `42` | Graine du monde |
| `WORLD_SIZE_CHUNKS` | `32` | Côté de la carte en chunks de 32 blocs (4 à 256) |
| `DATA_DIR` | `/data` | Dossier des sauvegardes, monté en volume |

Le serveur s'arrête proprement sur `docker stop` (SIGTERM).

## Tests

```sh
cargo test --workspace
```
