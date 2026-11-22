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
cargo run -p client --release                              # menu principal
cargo run -p client --release -- --solo                    # partie locale directe
cargo run -p client --release -- --connect 1.2.3.4:7777    # rejoindre un serveur
cargo run -p client --release -- --name Pseudo             # choisir son pseudo
```

En partie locale, le client héberge le serveur : les joueurs du même réseau peuvent rejoindre avec ton adresse IP locale (port UDP 7777).

| Touche | Action |
| --- | --- |
| `ZQSD` (AZERTY) ou `WASD` (QWERTY) | Se déplacer |
| Espace | Sauter |
| Maj gauche | Courir |
| Souris | Regarder (clic dans la fenêtre pour capturer la souris, Échap pour la libérer) |
| Clic gauche (maintenu) | Frapper un zombie, ou miner le bloc visé |
| Clic droit | Poser le bloc en main, ou manger |
| `1` à `9`, molette | Choisir l'objet en main |
| `C` ou `Tab` | Ouvrir le panneau de fabrication |
| Entrée | Réapparaître après la mort |

### Survivre

- Le jour, récolte : le bois des arbres, la pierre et le minerai de fer sous la surface. Les feuilles donnent parfois des pommes.
- Fabrique des planches, des bâtons, puis des pioches et des épées (bois, pierre, fer). Le minerai de fer et les murs renforcés se cassent seulement à la pioche.
- La faim baisse avec le temps. Sans manger, la vie ne remonte plus, puis diminue. Trois pommes font une ration ; feuilles et bâton font un bandage.
- La nuit, les zombies sortent et cassent les murs pour t'atteindre. Toutes les 7 nuits arrive une grande horde. Les murs renforcés tiennent longtemps, les piques blessent les zombies qui les frappent ou marchent dessus.

Sous Linux, Bevy demande `libasound2-dev`, `libudev-dev`, `libwayland-dev` et `libxkbcommon-dev`.

## Serveur dédié

Sur ton serveur (Docker et Docker Compose installés) :

```sh
git clone https://github.com/TiBzZ0/Zombie-Survivors.git
cd Zombie-Survivors
docker compose up -d          # construit l'image et lance le serveur
docker compose logs -f        # suivre le journal
```

Ouvre le port **UDP 7777** dans le pare-feu (et sur la box si le serveur est chez toi). Les joueurs rejoignent avec `adresse-du-serveur:7777` depuis le menu.

Mettre à jour : `git pull && docker compose up -d --build`. La partie est conservée.

L'image est aussi publiée par la CI à chaque fusion sur `main` : `ghcr.io/tibzz0/zombie-survivors-server:latest` (voir le commentaire dans `compose.yaml`).

Variables d'environnement :

| Variable | Défaut | Rôle |
| --- | --- | --- |
| `PORT` | `7777` | Port UDP |
| `MAX_PLAYERS` | `8` | Joueurs maximum (1 à 8) |
| `WORLD_SEED` | `42` | Graine du monde |
| `WORLD_SIZE_CHUNKS` | `32` | Côté de la carte en chunks de 32 blocs (4 à 256) |
| `DATA_DIR` | `/data` | Dossier des sauvegardes, monté en volume |

### Sauvegarde

La partie (blocs modifiés, heure, joueurs avec leur inventaire) est enregistrée dans `DATA_DIR/world.sav` toutes les minutes et à l'arrêt (`docker stop`, Ctrl+C). Au redémarrage, la graine et la taille de la sauvegarde l'emportent sur les variables d'environnement. Pour repartir de zéro, supprimer le volume : `docker compose down -v`.

Copie de secours : `docker compose cp server:/data/world.sav ./world.sav`.

En partie locale, la sauvegarde est dans `%APPDATA%\ZombieSurvivors\local` (Windows), `~/Library/Application Support/ZombieSurvivors/local` (macOS) ou `~/.local/share/zombie-survivors/local` (Linux).

Un joueur qui revient avec le même pseudo retrouve sa position, sa santé et son inventaire.

## Tests

```sh
cargo test --workspace
```
