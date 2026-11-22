# Zombie Survivors

Jeu de survie coopératif en voxel, à la première personne, écrit en Rust avec Bevy : récolter des ressources, construire sa base et tenir face à des hordes de zombies. De 2 à 8 joueurs sur un serveur dédié (Docker) ou en mode local. Nom provisoire.

## Jouer

Télécharger `ZombieSurvivors-windows.zip` dans les [versions](https://github.com/TiBzZ0/Zombie-Survivors/releases), décompresser, lancer `ZombieSurvivors.exe`. Dans le menu : « Jouer en solo / héberger » ou « Rejoindre » avec l'adresse du serveur.

Publier une version : `git tag v0.1.0 && git push origin v0.1.0` (la CI construit le .zip et l'image du serveur).

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
| Clic gauche, arc ou fusil en main | Tirer |
| Clic droit | Parler à un survivant, ouvrir une porte, un coffre ou une tourelle, sinon poser le bloc en main ou manger |
| Maj + clic droit | Renvoyer un de vos survivants |
| Ctrl + clic droit | Confier l'arme ou la pièce d'armure tenue à un de vos survivants (l'ancienne vous revient) |
| Clic droit maintenu (arme de mêlée) | Lever la garde : les coups de face sont très réduits, et parés s'ils arrivent juste après |
| F11 | Plein écran |
| `T` | Écrire dans la discussion (Entrée pour envoyer) |
| `1` à `9`, molette | Choisir l'objet en main |
| `C` ou `Tab` | Ouvrir le panneau de fabrication |
| Entrée | Réapparaître après la mort |

### Survivre

- Le jour, récolte : le bois des arbres, la pierre et le minerai de fer sous la surface. Les feuilles donnent parfois des pommes.
- Fabrique des planches, des bâtons, puis des pioches et des épées (bois, pierre, fer). Le minerai de fer et les murs renforcés se cassent seulement à la pioche.
- Au corps-à-corps, chaque arme a son style : l'**épée** est équilibrée, la **lance** frappe de plus loin et repousse fort, la **hache** tape dur (et coupe le bois bien plus vite), la **masse d'armes** (forge) est lente mais touche tous les zombies devant toi. Clic droit maintenu avec une arme de mêlée : tu lèves la garde, qui ne laisse passer qu'un tiers des coups de face ; levée juste avant le coup, elle le **pare** et étourdit le zombie.
- **Explosifs** : la **grenade** (forge) se lance au clic gauche et explose au bout de 2 s ; la **dynamite** (établi : charbon, sable, bâton) met 3 s mais détruit aussi les blocs alentour, sauf les murs renforcés, les coffres et les tourelles. Attention, l'explosion blesse aussi les joueurs et les survivants trop proches. L'**arbalète** (forge) tire des carreaux qui traversent jusqu'à 4 zombies alignés.
- **Armes fixes** à piloter (clic droit dessus pour prendre les commandes, clic gauche pour tirer, clic droit pour descendre) : la **mitrailleuse** (forge, avec un fusil) tire très vite avec tes cartouches ; le **canon** (forge) envoie des boulets qui explosent à l'impact ; la **catapulte** (établi) lance tes blocs de pierre en cloche, par-dessus tes propres murs.
- **Armures** : casque et armure en bois, en pierre (établi) ou en fer (forge). Le meilleur casque et la meilleure armure de ton sac sont portés d'office et absorbent jusqu'à 45 % des dégâts.
- Certaines recettes demandent un atelier posé à moins de 4 blocs : l'**établi** (6 planches) pour les outils en pierre, l'arc, les flèches, les portes et les coffres ; la **forge** (10 pierres et 2 charbons, à l'établi) pour fondre le fer, les outils en fer, les murs renforcés, les pics et le fusil.
- Les blocs attaqués par les zombies se fissurent et rougissent avant de céder ; une barre de vie s'affiche au-dessus des zombies blessés.
- La faim baisse avec le temps. Sans manger, la vie ne remonte plus, puis diminue. Trois pommes font une ration ; feuilles et bâton font un bandage.
- Les filons de fer et de charbon (rochers sombres en surface, poches sous terre) se récoltent 20 fois par bloc. Lingot = minerai de fer + charbon.
- Arc et flèches, puis fusil et cartouches, pour tenir les zombies à distance.
- Portes (deux blocs empilés s'ouvrent ensemble) et coffres partagés pour la base. Un coffre cassé par un joueur lui rend son contenu ; cassé par un zombie, le contenu est perdu.
- Des **ruines** (maisons écroulées) parsèment la carte : leur coffre contient de la nourriture, des munitions, parfois une arme. Chaque coffre ne se pille qu'une fois.
- Des **survivants** (losange jaune au-dessus de la tête) errent le jour, souvent près des ruines. Donne-leur une ration ou 3 pommes pour les recruter (autant que tu veux). Chacun a sa force (dégâts), son endurance (vie) et son habileté (vitesse de travail). Clic droit sur un de tes survivants (losange vert) pour changer son ordre : **suivre**, **garder** l'endroit, **récolter** (bois, pierre, pommes, charbon, rangés dans le coffre le plus proche de son poste, sinon dans ton inventaire ; le jour seulement) ou **réparer** les blocs abîmés autour de lui. Ils se battent contre les zombies proches, mais les zombies s'en prennent aussi à eux. Confie-leur une arme (épée, lance, hache, masse, arc, fusil) et des pièces d'armure : armés d'une arme à distance, ils tirent avec les munitions de ton sac quand ils te suivent, ou avec celles du coffre près de leur poste.
- **Nourriture** : l'herbe donne parfois des **graines** ; sème-les (clic droit sur la terre ou l'herbe) et le blé mûrit en quelques minutes. Récolte-le mûr pour du blé et de nouvelles graines. Le **gibier** (lapins, cerfs, sangliers) se promène dans les prairies et fuit à ton approche ; abats-le pour sa viande (attention, le sanglier blessé charge). Au **feu de camp** (4 bâtons, 3 pierres, 1 charbon), cuis la viande et fais du pain avec 3 blés. Un pain et une viande cuite donnent 2 rations.
- Tes **survivants** ont faim eux aussi : ils mangent ce qu'il y a dans ton sac quand ils te suivent, sinon dans le coffre près de leur poste. Affamés, ils arrêtent de travailler et dépérissent.
- La nuit, les zombies sortent et cassent les murs pour t'atteindre. Toutes les 7 nuits arrive une grande horde. Les murs renforcés tiennent longtemps, les piques blessent les zombies qui les frappent ou marchent dessus. Les **barbelés** (établi : 1 lingot de fer et 2 bâtons pour 4) se traversent, mais ralentissent fortement les zombies et les blessent, en s'usant peu à peu. La **tourelle** (forge : 3 lingots de fer, 4 planches et un arc) s'ouvre comme un coffre : range-y des cartouches (55 dégâts, 24 m) ou des flèches (30 dégâts, 18 m) et elle tire seule sur le zombie visible le plus proche.

Le menu règle la sensibilité de la souris, le volume et le plein écran (enregistrés dans `reglages.txt`, à côté de la sauvegarde locale).

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

La partie (blocs modifiés, heure, joueurs avec leur inventaire, coffres, survivants recrutés) est enregistrée dans `DATA_DIR/world.sav` toutes les minutes et à l'arrêt (`docker stop`, Ctrl+C). Au redémarrage, la graine et la taille de la sauvegarde l'emportent sur les variables d'environnement. Pour repartir de zéro, supprimer le volume : `docker compose down -v`.

Copie de secours : `docker compose cp server:/data/world.sav ./world.sav`.

En partie locale, la sauvegarde est dans `%APPDATA%\ZombieSurvivors\local` (Windows), `~/Library/Application Support/ZombieSurvivors/local` (macOS) ou `~/.local/share/zombie-survivors/local` (Linux).

Un joueur qui revient avec le même pseudo retrouve sa position, sa santé et son inventaire.

## Tests

```sh
cargo test --workspace
```
