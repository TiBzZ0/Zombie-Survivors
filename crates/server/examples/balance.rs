//! Simulation d'équilibrage : un joueur scripté tient plusieurs nuits d'affilée, sans réseau
//! et en temps accéléré, et l'on mesure ce que chaque nuit lui coûte.
//!
//! `cargo run -p server --release --example balance -- [nuits]`
//!
//! Trois scénarios, du plus démuni au mieux équipé :
//! - « à découvert » : épée en fer et armure de bois, pas de base ;
//! - « cabane » : murs de pierre de 2 blocs, une marche pour tirer par-dessus, fusil
//!   et 40 cartouches par nuit, puis l'épée ;
//! - « fort » : murs renforcés de 3 blocs, barbelés, deux tourelles chargées, fusil et
//!   60 cartouches par nuit, armure de fer.
//!
//! Le joueur ne tire que sur ce qu'il voit (pas à travers les murs).
//!
//! Les journées sont sautées (rien ne s'y passe pour les hordes). À l'aube, la base est
//! réparée et l'on note les blocs perdus. Le joueur réapparaît aussitôt s'il meurt.

use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use server::daynight::Clock;
use server::net::{ConnectedPlayer, Lobby, WorldVersion};
use server::survivors::clear_line;
use server::world::{Chests, GameWorld, SpawnPoint};
use server::zombies::{Zombie, ZombieBody, ZombieSet};
use server::{ServerConfig, build_app};
use shared::block::Block;
use shared::daytime::{DAYLIGHT_FRACTION, GameTime};
use shared::item::{Inventory, Item, Tier};

/// Identifiant du joueur scripté.
const BOT: u64 = 1;

#[derive(Clone, Copy, PartialEq)]
enum Scenario {
    Open,
    Shack,
    Fort,
}

impl Scenario {
    fn name(self) -> &'static str {
        match self {
            Scenario::Open => "à découvert (épée en fer, armure de bois)",
            Scenario::Shack => "cabane (murs de pierre, fusil et 40 cartouches, puis épée)",
            Scenario::Fort => "fort (murs renforcés, barbelés, 2 tourelles, fusil et 60 cartouches, armure de fer)",
        }
    }

    fn weapon(self) -> Item {
        match self {
            Scenario::Open => Item::Sword(Tier::Iron),
            _ => Item::Rifle,
        }
    }

    /// Cartouches pour une nuit.
    fn ammo(self) -> u32 {
        match self {
            Scenario::Open => 0,
            Scenario::Shack => 40,
            Scenario::Fort => 60,
        }
    }

    /// Bloc des murs, demi-largeur, hauteur des murs et de la marche où se tient le joueur.
    fn walls(self) -> Option<(Block, i32, i32, i32)> {
        match self {
            Scenario::Open => None,
            Scenario::Shack => Some((Block::Stone, 3, 2, 1)),
            Scenario::Fort => Some((Block::ReinforcedWall, 4, 3, 2)),
        }
    }

    fn inventory(self) -> Inventory {
        let mut inventory = Inventory::default();
        inventory.add(self.weapon(), 1);
        inventory.add(Item::Sword(Tier::Iron), 1);
        match self {
            Scenario::Open => {
                inventory.add(Item::BodyArmor(Tier::Wood), 1);
            }
            Scenario::Shack => {}
            Scenario::Fort => {
                inventory.add(Item::Helmet(Tier::Iron), 1);
                inventory.add(Item::BodyArmor(Tier::Iron), 1);
            }
        }
        inventory
    }
}

/// Blocs de la base, posés autour du point d'apparition (et remis en état à chaque aube).
#[derive(Resource, Default)]
struct Base(Vec<(IVec3, Block)>);

/// Mesures de la nuit en cours.
#[derive(Resource, Default, Clone)]
struct Night {
    kills: u32,
    shots: u32,
    damage_taken: f32,
    deaths: u32,
    peak_zombies: usize,
    spawned: u32,
    last_id: u32,
    cooldown: f32,
    ammo_left: u32,
    /// Instant de la première mort dans la nuit, en secondes.
    first_death: Option<f32>,
    elapsed: f32,
    /// Vie du joueur au tick précédent, pour compter les dégâts reçus.
    last_health: f32,
}

/// Blocs de la base, et hauteur où se tient le joueur.
fn build_base(scenario: Scenario, center: IVec3, world: &GameWorld) -> (Vec<(IVec3, Block)>, i32) {
    let mut blocks = Vec::new();
    let ground = world.generator().surface_height(center.x, center.z);
    let Some((wall, half, height, step)) = scenario.walls() else {
        return (blocks, ground);
    };
    // Murs à niveau : leur sommet est `height` blocs au-dessus du point le plus haut du terrain
    // autour (un joueur ne laisse pas un pan d'un seul bloc sur une pente).
    let ring = |r: i32| {
        (-r..=r)
            .flat_map(move |dx| (-r..=r).map(move |dz| (dx, dz)))
            .filter(move |(dx, dz)| dx.abs() == r || dz.abs() == r)
    };
    let top = ring(half + 1)
        .chain(ring(half))
        .map(|(dx, dz)| world.generator().surface_height(center.x + dx, center.z + dz))
        .max()
        .unwrap_or(ground)
        + height;
    // La marche du joueur, assez haute pour tirer par-dessus les murs.
    let stand = (top - height + step).max(ground);
    for y in ground..stand {
        blocks.push((IVec3::new(center.x, y, center.z), Block::Stone));
    }
    for (dx, dz) in ring(half) {
        {
            let ground = world.generator().surface_height(center.x + dx, center.z + dz);
            for dy in 0..(top - ground) {
                blocks.push((IVec3::new(center.x + dx, ground + dy, center.z + dz), wall));
            }
        }
    }
    if scenario == Scenario::Fort {
        // Une ceinture de barbelés deux blocs devant les murs.
        let ring = half + 2;
        for dx in -ring..=ring {
            for dz in -ring..=ring {
                if dx.abs() == ring || dz.abs() == ring {
                    let ground = world.generator().surface_height(center.x + dx, center.z + dz);
                    blocks.push((IVec3::new(center.x + dx, ground, center.z + dz), Block::BarbedWire));
                }
            }
        }
        // Les tourelles sur deux coins du mur, d'où elles voient venir la horde.
        for (dx, dz) in [(-half, -half), (half, half)] {
            blocks.push((IVec3::new(center.x + dx, top, center.z + dz), Block::Turret));
        }
    }
    (blocks, stand)
}

/// Pose (ou répare) la base, et recharge les tourelles. Renvoie le nombre de blocs remplacés.
fn repair_base(world: &mut World) -> usize {
    let base = world.resource::<Base>().0.clone();
    let mut repaired = 0;
    world.resource_scope(|world, mut game: Mut<GameWorld>| {
        for (pos, block) in &base {
            if game.block(*pos) != *block {
                game.set_block(*pos, *block);
                repaired += 1;
            }
            if *block == Block::Turret {
                let mut ammo = Inventory::default();
                ammo.add(Item::Ammo, 60);
                world.resource_mut::<Chests>().0.insert(*pos, ammo);
            }
        }
    });
    world.resource_mut::<WorldVersion>().0 += 1;
    repaired
}

/// Le joueur scripté : il reste au centre de sa base, mange et boit, tire sur le zombie le plus
/// proche (ou frappe à l'épée), et réapparaît aussitôt s'il meurt.
fn bot_fights(
    time: Res<Time>,
    scenario: Res<ScenarioRes>,
    spawn: Res<SpawnPoint>,
    mut lobby: ResMut<Lobby>,
    mut night: ResMut<Night>,
    mut zombies: Query<(&mut Zombie, &ZombieBody)>,
    world: Res<GameWorld>,
) {
    let Some(player) = lobby.players.get_mut(&BOT) else {
        return;
    };
    night.damage_taken += (night.last_health - player.health).max(0.0);
    night.elapsed += time.delta_secs();
    if player.health <= 0.0 {
        night.deaths += 1;
        if night.first_death.is_none() {
            night.first_death = Some(night.elapsed);
        }
        player.health = 100.0;
        player.state.position = spawn.0;
    }
    // La survie au quotidien n'est pas ce qu'on mesure ici.
    player.hunger = 100.0;
    player.thirst = 100.0;
    night.last_health = player.health;

    night.cooldown -= time.delta_secs();
    let peak = zombies.iter().count();
    night.peak_zombies = night.peak_zombies.max(peak);
    for (zombie, _) in &zombies {
        if zombie.id > night.last_id {
            night.spawned += zombie.id - night.last_id;
            night.last_id = zombie.id;
        }
    }
    let me = player.state.position;
    if night.cooldown > 0.0 {
        return;
    }
    let eye = me + Vec3::Y * 1.6;
    let mut weapon = scenario.0.weapon();
    if weapon.ranged().is_some() && night.ammo_left == 0 {
        weapon = Item::Sword(Tier::Iron);
    }
    // On attend qu'ils approchent (40 m au plus) pour ne pas gaspiller de cartouches.
    let (damage, range, cooldown) = match weapon.ranged() {
        Some(r) => (r.damage, r.range.min(40.0), r.cooldown),
        None => {
            let m = Item::melee(Some(weapon));
            (m.damage, m.reach + 0.6, m.cooldown)
        }
    };
    let world = world.into_inner();
    let target = zombies
        .iter_mut()
        .filter(|(z, _)| z.health > 0.0)
        .map(|(z, b)| (z, b.position.distance(me), b.position))
        .filter(|(_, d, _)| *d <= range)
        .filter(|(_, d, p)| *d < 2.5 || clear_line(world, eye, *p + Vec3::Y * 1.2))
        .min_by(|a, b| a.1.total_cmp(&b.1));
    if let Some((mut zombie, _, _)) = target {
        if weapon.ranged().is_some() {
            night.ammo_left -= 1;
        }
        zombie.health -= damage;
        zombie.last_attacker = Some(BOT);
        if zombie.health <= 0.0 {
            night.kills += 1;
        }
        night.shots += 1;
        night.cooldown = cooldown;
    }
}

#[derive(Resource)]
struct ScenarioRes(Scenario);

fn run(scenario: Scenario, nights: u32, port: u16) {
    let data_dir = std::env::temp_dir().join(format!("zs-balance-{}-{port}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data_dir);
    let config = ServerConfig {
        port,
        data_dir: data_dir.clone(),
        ..ServerConfig::default()
    };
    let mut app = build_app(config, false);
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 30.0)))
        .insert_resource(ScenarioRes(scenario))
        .init_resource::<Base>()
        .init_resource::<Night>()
        .add_systems(FixedUpdate, bot_fights.after(ZombieSet::Ai).before(ZombieSet::Deaths));
    app.finish();
    app.cleanup();
    app.update();

    // Le joueur arrive, la base se construit autour de lui.
    let spawn = app.world().resource::<SpawnPoint>().0;
    let center = spawn.floor().as_ivec3();
    let (base, stand) = build_base(scenario, center, app.world().resource::<GameWorld>());
    let spawn = Vec3::new(center.x as f32 + 0.5, stand as f32, center.z as f32 + 0.5);
    app.world_mut().resource_mut::<SpawnPoint>().0 = spawn;
    app.world_mut().resource_mut::<Base>().0 = base;
    repair_base(app.world_mut());
    let mut player = ConnectedPlayer::new(spawn);
    player.name = "Bot".into();
    player.inventory = scenario.inventory();
    player.held = Some(scenario.weapon());
    app.world_mut().resource_mut::<Lobby>().players.insert(BOT, player);

    println!("\n## {}\n", scenario.name());
    println!(
        "| Nuit | Zombies apparus | Pic simultané | Tués par le joueur | Tirs/coups | Cartouches restantes | Dégâts reçus | Première mort | Morts | Blocs perdus |"
    );
    println!("|---|---|---|---|---|---|---|---|---|---|");
    let started = Instant::now();
    for day in 1..=nights {
        // Saute la journée : la nuit commence tout de suite.
        app.world_mut().resource_mut::<Clock>().0 = GameTime {
            day,
            phase: DAYLIGHT_FRACTION + 0.0005,
        };
        let last_id = app.world().resource::<Night>().last_id;
        *app.world_mut().resource_mut::<Night>() = Night {
            last_id,
            last_health: 100.0,
            ammo_left: scenario.ammo(),
            ..default()
        };
        if let Some(p) = app.world_mut().resource_mut::<Lobby>().players.get_mut(&BOT) {
            p.health = 100.0;
            p.state.position = spawn;
        }
        while app.world().resource::<Clock>().0.day == day {
            app.update();
        }
        // L'aube : on compte les dégâts puis on répare. Le soleil brûle les zombies restants.
        let lost = repair_base(app.world_mut());
        let world = app.world_mut();
        let left: Vec<Entity> = world.query_filtered::<Entity, With<Zombie>>().iter(world).collect();
        for entity in left {
            world.despawn(entity);
        }
        let n = app.world().resource::<Night>().clone();
        let horde = if day.is_multiple_of(shared::daytime::HORDE_EVERY) {
            " (horde)"
        } else {
            ""
        };
        println!(
            "| {day}{horde} | {} | {} | {} | {} | {} | {:.0} | {} | {} | {lost} |",
            n.spawned,
            n.peak_zombies,
            n.kills,
            n.shots,
            n.ammo_left,
            n.damage_taken,
            n.first_death.map_or("—".to_string(), |t| format!("{:.0} s", t)),
            n.deaths
        );
    }
    eprintln!(
        "({} nuits simulées en {:.0} s)",
        nights,
        started.elapsed().as_secs_f32()
    );
    let _ = std::fs::remove_dir_all(&data_dir);
}

fn main() {
    let nights: u32 = std::env::args().nth(1).and_then(|n| n.parse().ok()).unwrap_or(14);
    let only = std::env::args().nth(2);
    for (i, scenario) in [Scenario::Open, Scenario::Shack, Scenario::Fort]
        .into_iter()
        .enumerate()
    {
        if only.as_deref().is_some_and(|o| o != i.to_string()) {
            continue;
        }
        run(scenario, nights, 21_000 + i as u16);
    }
}
