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
cargo run -p client --release -- --solo --pvp              # héberger une partie avec combat entre joueurs
```

En partie locale, le client héberge le serveur : les joueurs du même réseau peuvent rejoindre avec ton adresse IP locale (port UDP 7777).

| Touche | Action |
| --- | --- |
| `ZQSD` (AZERTY) ou `WASD` (QWERTY) | Se déplacer |
| Espace | Sauter |
| Maj gauche | Courir (dans l'eau : crawl) |
| Ctrl (dans l'eau) | Plonger ; sous l'eau on nage là où l'on regarde, Espace pour remonter |
| Souris | Regarder (clic dans la fenêtre pour capturer la souris, Échap pour la libérer) |
| Clic gauche (maintenu) | Frapper un zombie, ou miner le bloc visé |
| Clic gauche, arc ou fusil en main | Tirer |
| Clic droit | Parler à un survivant, ouvrir une porte, un coffre ou une tourelle, sinon poser le bloc en main ou manger |
| Maj + clic droit | Renvoyer un de vos survivants |
| Ctrl + clic droit | Confier l'arme ou la pièce d'armure tenue à un de vos survivants (l'ancienne vous revient) |
| Clic droit maintenu (arme de mêlée) | Lever la garde : les coups de face sont très réduits, et parés s'ils arrivent juste après |
| Garde levée + clic gauche | Coup puissant : 1,8 fois les dégâts, repousse loin et touche tous les zombies devant, mais coûte un quart de l'endurance |
| F11 | Plein écran |
| `T` | Écrire dans la discussion (Entrée pour envoyer) |
| `1` à `9`, molette | Choisir l'objet en main |
| `C` ou `Tab` | Fabrication par rubriques (composants, atelier et palier technologique requis) et inventaire : glisser un objet sur la barre, ou le survoler et presser 1 à 9 |
| `K` | Arbre de compétences : 28 compétences en 5 branches et 4 paliers, un point par niveau (l'expérience vient des zombies tués et des nuits survécues) |
| Entrée | Réapparaître après la mort |

### Survivre

- Le jour, récolte : le bois des arbres, la pierre et le minerai de fer sous la surface. Les feuilles donnent parfois des pommes.
- Fabrique des planches, des bâtons, puis des pioches et des épées (bois, pierre, fer). Le minerai de fer et les murs renforcés se cassent seulement à la pioche.
- Au corps-à-corps, chaque arme a son style : l'**épée** est équilibrée, la **lance** frappe de plus loin et repousse fort, la **hache** tape dur (et coupe le bois bien plus vite), la **masse d'armes** (forge) est lente mais touche tous les zombies devant toi. Clic droit maintenu avec une arme de mêlée : tu lèves la garde, qui ne laisse passer qu'un tiers des coups de face ; levée juste avant le coup, elle le **pare** et étourdit le zombie. Garde levée, un clic gauche donne un **coup puissant**.
- L'**endurance** (fine barre jaune au-dessus de la barre d'outils) baisse quand tu cours, crawles, sautes ou frappes, et remonte au repos (moins vite dans l'eau). Vidée, elle clignote en rouge : plus de course ni de coup puissant, et tes coups ne portent qu'à moitié jusqu'à ce qu'elle remonte à 30. La compétence Vitesse l'agrandit.
- **Explosifs** : la **grenade** (forge) se lance au clic gauche et explose au bout de 2 s ; la **dynamite** (établi : charbon, sable, bâton) met 3 s mais détruit aussi les blocs alentour, sauf les murs renforcés, les coffres et les tourelles. Attention, l'explosion blesse aussi les joueurs et les survivants trop proches. L'**arbalète** (forge) tire des carreaux qui traversent jusqu'à 4 zombies alignés.
- **Électricité** : le **générateur** (forge : 6 lingots de fer, 4 charbons) brûle le charbon qu'on range dedans (1 charbon = 2 minutes) et alimente sans câble les appareils à 12 blocs autour. Le **projecteur** (établi) éclaire la nuit et brûle les zombies à moins de 7 m. La **porte électrique** (forge, très solide) s'ouvre seule devant toi et tes survivants et reste fermée aux zombies ; sans courant, elle s'ouvre à la main. La **bobine électrique** (forge) foudroie le zombie visible le plus proche à 10 m, sans munitions.
- **Événements** (à partir du jour 2, tirés au sort à chaque aube) : un **brouillard** épais cache le soleil, les zombies ne brûlent plus et quelques-uns rôdent en plein jour ; un **largage** de ravitaillement tombe à 30-70 m d'un joueur (le chat donne la direction, la caisse orange se voit sur la carte : cartouches, rations, bandages, parfois un antidote, des grenades, un fusil, une armure ou un générateur) ; un **marchand** passe près d'un joueur jusqu'au soir et propose trois échanges (par exemple 4 peaux contre 12 cartouches), clic droit pour échanger.
- **Solidité des constructions** : un bloc posé doit reposer, de proche en proche, sur le sol ou la roche. Un pilier monte aussi haut qu'on veut, mais chaque bloc en porte-à-faux perd de la solidité : 1 bloc pour la terre ou le sable, 3 pour la pierre, 4 pour le bois et les planches, 7 pour le mur renforcé (un pont de planches franchit 8 blocs entre deux piliers). Un bloc qui ne tiendrait pas est refusé. Si un pilier est cassé, explosé ou abattu par les zombies, tout ce qu'il portait s'effondre : les blocs retombent tout droit, écrasent zombies et joueurs dessous, et les coffres emportent leur contenu dans leur chute.
- **JcJ optionnel et bases protégées** (serveur lancé avec `PVP=oui`, ou partie locale avec `--pvp`) : les joueurs peuvent se frapper et se tirer dessus. Chacun peut planter une **bannière** (établi : 2 planches, 2 peaux, 2 bâtons) : dans un rayon de 12 m, les autres joueurs ne peuvent ni casser, ni poser, ni ouvrir de coffre ou de porte, ni faire sauter quoi que ce soit, et le propriétaire n'y prend aucun coup des autres joueurs. Une bannière par joueur, et deux bases ne se chevauchent pas. Les zombies ignorent tout ça : une bannière abattue libère la base. L'écran rappelle que le JcJ est actif et dans quelle base on se trouve.
- **Armes fixes** à piloter (clic droit dessus pour prendre les commandes, clic gauche pour tirer, clic droit pour descendre) : la **mitrailleuse** (forge, avec un fusil) tire très vite avec tes cartouches ; le **canon** (forge) envoie des boulets qui explosent à l'impact ; la **catapulte** (établi) lance tes blocs de pierre en cloche, par-dessus tes propres murs.
- **Armures** : casque et armure en bois, en pierre (établi) ou en fer (forge). Le meilleur casque et la meilleure armure de ton sac sont portés d'office et absorbent jusqu'à 45 % des dégâts.
- Certaines recettes demandent un atelier posé à moins de 4 blocs : l'**établi** (6 planches) pour les outils en pierre, l'arc, les flèches, les portes et les coffres ; la **forge** (10 pierres et 2 charbons, à l'établi) pour fondre le fer, les outils en fer, les murs renforcés, les pics et le fusil.
- Les blocs attaqués par les zombies se fissurent et rougissent avant de céder ; une barre de vie s'affiche au-dessus des zombies blessés.
- La faim baisse avec le temps. Sans manger, la vie ne remonte plus, puis diminue. Trois pommes font une ration ; feuilles et bâton font un bandage.
- Les filons de fer et de charbon (rochers sombres en surface, poches sous terre) se récoltent 20 fois par bloc. Lingot = minerai de fer + charbon.
- Arc et flèches, puis fusil et cartouches, pour tenir les zombies à distance.
- Portes (deux blocs empilés s'ouvrent ensemble) et coffres partagés pour la base. Un coffre cassé par un joueur lui rend son contenu ; cassé par un zombie, le contenu est perdu.
- Le monde a ses **biomes** : prairies dégagées, forêts denses, déserts de sable et de cactus, terres enneigées. Les creux sont remplis de **lacs** où l'on nage : on flotte la tête hors de l'eau, on plonge avec Ctrl, et le **souffle** (bulles au-dessus de la barre) s'épuise sous l'eau avant la noyade. Pour sortir, il suffit d'avancer vers la rive.
- La **soif** baisse plus vite que la faim (environ 10 minutes). Clic droit sur l'eau, main libre, pour boire. La **gourde** (établi : 1 lingot de fer et 1 bâton) se remplit au lac et se boit n'importe où. Sans boire, la vie ne remonte plus, puis diminue.
- L'**eau** est transparente et **s'écoule** : creuser au bord d'un lac l'inonde, et l'eau courante s'étale jusqu'à 7 blocs à plat ou tombe dans les trous. Le **seau** (établi : 3 lingots de fer) se remplit à l'eau et se vide au clic droit : il crée une source, pratique pour des douves. Une source versée au seau se reprend ; l'eau courante se retire quand sa source disparaît.
- La **température** compte dans la neige et le désert. La nuit enneigée glace en moins d'une minute à découvert : un **feu de camp** à quelques pas, un **toit** au-dessus de la tête ou un **manteau de fourrure** (établi : 4 peaux, prises sur le gibier) protègent. Le jour, le désert brûle : de l'ombre ou un **chapeau de paille** (4 blés). Sous 35 °C ou au-dessus de 39,5 °C, la vie baisse ; quand on a trop chaud, la soif se creuse deux fois plus vite.
- Une morsure qui traverse l'armure peut **infecter** (environ une fois sur huit). L'infection monte en 12 minutes : la vie ne remonte plus, puis elle baisse au-delà de 60 %. L'**antidote** guérit (établi : 1 charbon, 2 pommes et 1 bandage ; on en trouve aussi dans les entrepôts).
- Des **ruines** (maisons écroulées) parsèment la carte : leur coffre contient de la nourriture, des munitions, parfois une arme. Les grands **entrepôts** en ruine ont trois coffres. Chaque coffre ne se pille qu'une fois.
- Des **survivants** (losange jaune au-dessus de la tête) errent le jour, souvent près des ruines. Donne-leur une ration ou 3 pommes pour les recruter (autant que tu veux). Chacun a sa force (dégâts), son endurance (vie) et son habileté (vitesse de travail). Clic droit sur un de tes survivants (losange vert) pour changer son ordre : **suivre**, **garder** l'endroit, **récolter** (bois, pierre, pommes, charbon, rangés dans le coffre le plus proche de son poste, sinon dans ton inventaire ; le jour seulement) **réparer** les blocs abîmés autour de lui, ou prendre un métier : **fermier** (récolte le blé mûr dans un rayon de 6 blocs et le ressème, sème sur la terre nue avec les graines du coffre), **cuisinier** (au feu de camp près de son poste, cuit la viande crue et fait le pain avec le blé du coffre) ou **chasseur** (traque le gibier dans un rayon de 40 blocs le jour, rapporte viande et peaux au coffre). Ils se battent contre les zombies proches, mais les zombies s'en prennent aussi à eux. Confie-leur une arme (épée, lance, hache, masse, arc, fusil) et des pièces d'armure : armés d'une arme à distance, ils tirent avec les munitions de ton sac quand ils te suivent, ou avec celles du coffre près de leur poste.
- **Nourriture** : l'herbe donne parfois des **graines** ; sème-les (clic droit sur la terre ou l'herbe) et le blé mûrit en quelques minutes. Récolte-le mûr pour du blé et de nouvelles graines. Le **gibier** (lapins, cerfs, sangliers) se promène dans les prairies et fuit à ton approche ; abats-le pour sa viande (attention, le sanglier blessé charge). Au **feu de camp** (4 bâtons, 3 pierres, 1 charbon), cuis la viande et fais du pain avec 3 blés. Un pain et une viande cuite donnent 2 rations.
- Tes **survivants** ont faim eux aussi : ils mangent ce qu'il y a dans ton sac quand ils te suivent, sinon dans le coffre près de leur poste. Affamés, ils arrêtent de travailler et dépérissent.
- Leur **moral** monte quand ils mangent à leur faim, ont un feu de camp près de leur poste, une arme et une armure ; il baisse quand ils sont blessés, les nuits de grande horde et quand un compagnon tombe près d'eux. Bon moral : ils travaillent plus vite et frappent plus fort ; moral en berne : ils traînent. Au plus bas pendant une minute, ils te quittent (en te rendant ce que tu leur avais confié). Vise un survivant pour voir son moral.
- La nuit, les zombies sortent et cassent les murs pour t'atteindre. Toutes les 7 nuits arrive une grande horde, menée par un **colosse** énorme qui défonce les murs. Dès la troisième nuit apparaissent les **cracheurs**, qui s'arrêtent à distance pour cracher de l'acide (la garde le bloque). Et à partir de la troisième nuit aussi, les zombies serrés contre un mur **grimpent les uns sur les autres** pour passer par-dessus : au fil des nuits, il en faut moins et ils montent plus haut (jusqu'à 6 blocs). Les murs renforcés tiennent longtemps, les piques blessent les zombies qui les frappent ou marchent dessus. Les **barbelés** (établi : 1 lingot de fer et 2 bâtons pour 4) se traversent, mais ralentissent fortement les zombies et les blessent, en s'usant peu à peu. La **tourelle** (forge : 3 lingots de fer, 4 planches et un arc) s'ouvre comme un coffre : range-y des cartouches (55 dégâts, 24 m) ou des flèches (30 dégâts, 18 m) et elle tire seule sur le zombie visible le plus proche.

En haut à droite, une **carte** montre le terrain autour de toi, les zombies (rouge), les survivants (jaune), le gibier et les autres joueurs (bleu) ; M la masque. À gauche, les **objectifs** guident les premiers pas et se cochent tout seuls ; H les masque.

Le menu règle la sensibilité de la souris, le volume, le plein écran et la distance d'affichage (à baisser si le jeu rame) (enregistrés dans `reglages.txt`, à côté de la sauvegarde locale).

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
| `PVP` | `non` | Combat entre joueurs (`oui`/`non`) ; les bannières protègent alors les bases |

### Sauvegarde

La partie (blocs modifiés, heure, joueurs avec leur inventaire, coffres, survivants recrutés) est enregistrée dans `DATA_DIR/world.sav` toutes les minutes et à l'arrêt (`docker stop`, Ctrl+C). Au redémarrage, la graine et la taille de la sauvegarde l'emportent sur les variables d'environnement. Les mondes créés avant les biomes gardent leur terrain d'origine (sans lacs, donc sans soif). Pour repartir de zéro, supprimer le volume : `docker compose down -v`.

Copie de secours : `docker compose cp server:/data/world.sav ./world.sav`.

En partie locale, la sauvegarde est dans `%APPDATA%\ZombieSurvivors\local` (Windows), `~/Library/Application Support/ZombieSurvivors/local` (macOS) ou `~/.local/share/zombie-survivors/local` (Linux).

Un joueur qui revient avec le même pseudo retrouve sa position, sa santé et son inventaire.

## Tests

```sh
cargo test --workspace
```
