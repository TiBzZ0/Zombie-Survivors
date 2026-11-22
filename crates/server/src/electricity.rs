//! Électricité. Un générateur brûle le charbon qu'on range dedans (comme dans un coffre) et
//! alimente, sans câble, les appareils électriques à moins de `POWER_RADIUS` :
//! - le **projecteur** éclaire et, la nuit, brûle les zombies proches ;
//! - la **porte électrique** s'ouvre seule devant les joueurs et les survivants, et reste
//!   fermée aux zombies ;
//! - la **bobine électrique** foudroie le zombie visible le plus proche.
//!
//! Sans courant, la porte électrique s'ouvre à la main comme une autre, le reste s'éteint.

use std::collections::HashMap;

use bevy::prelude::*;
use bevy_renet::RenetServer;
use shared::block::{Block, POWER_RADIUS};
use shared::item::Item;
use shared::physics::{PLAYER_SIZE, overlaps_solid};
use shared::protocol::ServerMessage;

use crate::daynight::Clock;
use crate::net::{Lobby, WorldVersion, broadcast, send_chest_to_viewers};
use crate::survivors::{Survivor, clear_line};
use crate::world::{Chests, GameWorld};
use crate::zombies::{Zombie, ZombieBody, ZombieSet};

/// Durée de marche pour un charbon, en secondes.
pub const COAL_SECS: f32 = 120.0;
/// Portée et dégâts par seconde de la lumière d'un projecteur sur les zombies (la nuit).
pub const FLOOD_RADIUS: f32 = 7.0;
pub const FLOOD_DPS: f32 = 5.0;
/// Portée, dégâts et cadence d'une bobine électrique.
pub const TESLA_RANGE: f32 = 10.0;
pub const TESLA_DAMAGE: f32 = 30.0;
pub const TESLA_COOLDOWN: f32 = 1.0;
/// Distance à laquelle une porte électrique s'ouvre devant quelqu'un.
const DOOR_SENSE: f32 = 2.4;

pub struct ElectricityPlugin;

impl Plugin for ElectricityPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Power>().add_systems(
            FixedUpdate,
            (run_generators, floodlights, electric_doors, tesla_coils)
                .chain()
                .after(ZombieSet::Ai)
                .before(ZombieSet::Deaths),
        );
    }
}

/// État du réseau électrique.
#[derive(Resource, Default, Debug)]
pub struct Power {
    /// Secondes de marche restantes de chaque générateur.
    fuel: HashMap<IVec3, f32>,
    /// Générateurs en marche.
    pub running: Vec<IVec3>,
    /// Appareils électriques posés (relevés quand le monde change).
    devices: Vec<(IVec3, Block)>,
    seen_version: Option<u64>,
    broadcast_timer: f32,
    tesla_cooldowns: HashMap<IVec3, f32>,
    door_timer: f32,
}

impl Power {
    /// Un générateur en marche est assez proche de cette case.
    pub fn powers(&self, pos: IVec3) -> bool {
        is_powered(&self.running, pos)
    }

    /// Appareils alimentés de cette sorte.
    fn devices(&self, kind: fn(Block) -> bool) -> Vec<IVec3> {
        self.devices
            .iter()
            .filter(|(_, b)| kind(*b))
            .map(|(p, _)| *p)
            .filter(|p| self.powers(*p))
            .collect()
    }
}

/// Une case est alimentée si un des générateurs en marche est à portée.
pub fn is_powered(running: &[IVec3], pos: IVec3) -> bool {
    running
        .iter()
        .any(|g| g.as_vec3().distance(pos.as_vec3()) <= POWER_RADIUS)
}

/// Brûle le charbon des générateurs et tient la liste de ceux qui tournent.
fn run_generators(
    time: Res<Time>,
    world: Res<GameWorld>,
    version: Res<WorldVersion>,
    lobby: Res<Lobby>,
    mut chests: ResMut<Chests>,
    mut power: ResMut<Power>,
    mut server: Option<ResMut<RenetServer>>,
) {
    let dt = time.delta_secs();
    if power.seen_version != Some(version.0) {
        power.seen_version = Some(version.0);
        power.devices = world
            .edits()
            .iter()
            .filter(|(_, b)| b.is_electric())
            .map(|(p, b)| (*p, *b))
            .collect();
    }
    let generators: Vec<IVec3> = chests
        .0
        .keys()
        .copied()
        .filter(|p| world.block(*p) == Block::Generator)
        .collect();
    power.fuel.retain(|p, _| generators.contains(p));
    let mut running = Vec::new();
    for pos in generators {
        let left = power.fuel.entry(pos).or_insert(0.0);
        *left -= dt;
        if *left <= 0.0
            && let Some(inv) = chests.0.get_mut(&pos)
            && inv.remove(Item::Coal, 1)
        {
            *left += COAL_SECS;
            if let Some(server) = server.as_deref_mut() {
                send_chest_to_viewers(server, &lobby, &chests, pos);
            }
        }
        let left = power.fuel.get(&pos).copied().unwrap_or(0.0);
        if left > 0.0 {
            running.push(pos);
        } else {
            power.fuel.insert(pos, 0.0);
        }
    }
    running.sort_by_key(|p| (p.x, p.y, p.z));
    let changed = running != power.running;
    power.running = running;
    power.broadcast_timer -= dt;
    if (changed || power.broadcast_timer <= 0.0)
        && let Some(server) = server.as_deref_mut()
    {
        power.broadcast_timer = 3.0;
        broadcast(server, &ServerMessage::Power(power.running.clone()));
    }
}

/// La nuit, la lumière des projecteurs brûle les zombies proches.
fn floodlights(time: Res<Time>, clock: Res<Clock>, power: Res<Power>, mut zombies: Query<(&mut Zombie, &ZombieBody)>) {
    if !clock.is_night() {
        return;
    }
    let lights: Vec<Vec3> = power
        .devices(|b| b == Block::Floodlight)
        .into_iter()
        .map(|p| p.as_vec3() + Vec3::splat(0.5))
        .collect();
    if lights.is_empty() {
        return;
    }
    let dt = time.delta_secs();
    for (mut zombie, body) in &mut zombies {
        let center = body.position + Vec3::Y;
        if lights.iter().any(|l| l.distance(center) < FLOOD_RADIUS) {
            zombie.health -= FLOOD_DPS * dt;
            zombie.hurt_timer = zombie.hurt_timer.max(0.1);
        }
    }
}

/// Les portes électriques alimentées s'ouvrent devant les joueurs et les survivants.
fn electric_doors(
    time: Res<Time>,
    mut power: ResMut<Power>,
    mut world: ResMut<GameWorld>,
    mut version: ResMut<WorldVersion>,
    lobby: Res<Lobby>,
    survivors: Query<&Survivor>,
    mut server: Option<ResMut<RenetServer>>,
) {
    power.door_timer -= time.delta_secs();
    if power.door_timer > 0.0 {
        return;
    }
    power.door_timer = 0.2;
    let friends: Vec<Vec3> = lobby
        .players
        .values()
        .filter(|p| p.is_alive())
        .map(|p| p.state.position)
        .chain(survivors.iter().filter(|s| s.leader.is_some()).map(|s| s.position))
        .collect();
    let doors = power.devices(|b| matches!(b, Block::ElectricDoor | Block::ElectricDoorOpen));
    for pos in doors {
        let block = world.block(pos);
        if !matches!(block, Block::ElectricDoor | Block::ElectricDoorOpen) {
            continue;
        }
        let center = pos.as_vec3() + Vec3::new(0.5, 0.0, 0.5);
        let near =
            |p: &Vec3| Vec2::new(p.x - center.x, p.z - center.z).length() < DOOR_SENSE && (p.y - center.y).abs() < 2.5;
        let wanted = if friends.iter().any(near) {
            Block::ElectricDoorOpen
        } else {
            // Ne se referme pas sur quelqu'un.
            let blocked = friends.iter().any(|p| overlaps_solid(*p, PLAYER_SIZE, &|b| b == pos));
            if blocked { block } else { Block::ElectricDoor }
        };
        if wanted != block && world.set_block(pos, wanted) {
            version.0 += 1;
            if let Some(server) = server.as_deref_mut() {
                broadcast(server, &ServerMessage::BlockChanged { pos, block: wanted });
            }
        }
    }
}

/// Les bobines alimentées foudroient le zombie visible le plus proche.
fn tesla_coils(
    time: Res<Time>,
    world: Res<GameWorld>,
    mut power: ResMut<Power>,
    mut zombies: Query<(&mut Zombie, &ZombieBody)>,
    mut server: Option<ResMut<RenetServer>>,
) {
    let dt = time.delta_secs();
    let coils = power.devices(|b| b == Block::TeslaCoil);
    power.tesla_cooldowns.retain(|p, _| coils.contains(p));
    for pos in coils {
        let cooldown = power.tesla_cooldowns.entry(pos).or_insert(0.0);
        *cooldown -= dt;
        if *cooldown > 0.0 {
            continue;
        }
        let top = pos.as_vec3() + Vec3::new(0.5, 1.1, 0.5);
        let target = zombies
            .iter()
            .filter(|(z, _)| z.health > 0.0)
            .map(|(z, b)| (z.id, b.position + Vec3::Y))
            .filter(|(_, aim)| aim.distance(top) <= TESLA_RANGE && clear_line(&world, top, *aim))
            .min_by(|a, b| a.1.distance(top).total_cmp(&b.1.distance(top)));
        let Some((id, aim)) = target else { continue };
        *cooldown = TESLA_COOLDOWN;
        if let Some((mut z, _)) = zombies.iter_mut().find(|(z, _)| z.id == id) {
            z.health -= TESLA_DAMAGE;
            z.hurt_timer = 0.3;
        }
        if let Some(server) = server.as_deref_mut() {
            broadcast(server, &ServerMessage::Zap { origin: top, end: aim });
        }
    }
}

#[cfg(test)]
mod tests {
    use shared::config::WorldConfig;
    use shared::daytime::GameTime;
    use shared::item::Inventory;
    use shared::world::WorldState;
    use shared::zombie::ZombieKind;

    use super::*;
    use crate::net::ConnectedPlayer;

    /// Une dalle dans le ciel avec un générateur (chargé ou non) au centre.
    fn base(coal: u32, night: bool) -> (App, IVec3) {
        let mut world = GameWorld(WorldState::new(WorldConfig::default()));
        let y = world.generator().surface_height(100, 100) + 25;
        for x in 80..=120 {
            for z in 80..=120 {
                world.set_block(IVec3::new(x, y - 1, z), Block::Stone);
            }
        }
        let generator = IVec3::new(100, y, 100);
        world.set_block(generator, Block::Generator);
        let mut chests = Chests::default();
        let mut fuel = Inventory::default();
        if coal > 0 {
            fuel.add(Item::Coal, coal);
        }
        chests.0.insert(generator, fuel);
        let mut app = App::new();
        app.insert_resource(world)
            .insert_resource(chests)
            .insert_resource(Clock(GameTime {
                day: 1,
                phase: if night { 0.8 } else { 0.2 },
            }))
            .init_resource::<Time>()
            .init_resource::<WorldVersion>()
            .init_resource::<Lobby>()
            .init_resource::<Power>()
            .add_systems(
                Update,
                (run_generators, floodlights, electric_doors, tesla_coils).chain(),
            );
        (app, generator)
    }

    fn place(app: &mut App, pos: IVec3, block: Block) {
        app.world_mut().resource_mut::<GameWorld>().set_block(pos, block);
        app.world_mut().resource_mut::<WorldVersion>().0 += 1;
    }

    fn zombie(app: &mut App, at: Vec3) -> Entity {
        app.world_mut()
            .spawn((
                Zombie {
                    id: 1,
                    kind: ZombieKind::Walker,
                    health: 1000.0,
                    attack_cooldown: 0.0,
                    attacking: 0.0,
                    hurt_timer: 0.0,
                    knockback: Vec3::ZERO,
                    last_attacker: None,
                    climb_from: None,
                    stuck: 0.0,
                },
                ZombieBody {
                    position: at,
                    velocity: Vec3::ZERO,
                    yaw: 0.0,
                    on_ground: true,
                    hit_wall: false,
                },
            ))
            .id()
    }

    fn run(app: &mut App, secs: f32) {
        for _ in 0..(secs * 10.0) as usize {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::from_secs_f32(0.1));
            app.update();
        }
    }

    fn health(app: &App, entity: Entity) -> f32 {
        app.world().get::<Zombie>(entity).unwrap().health
    }

    #[test]
    fn a_fuelled_generator_powers_a_tesla_coil() {
        let (mut app, generator) = base(1, false);
        place(&mut app, generator + IVec3::new(4, 0, 0), Block::TeslaCoil);
        let z = zombie(&mut app, generator.as_vec3() + Vec3::new(9.5, 0.0, 0.5));
        run(&mut app, 3.0);
        assert!(health(&app, z) <= 1000.0 - 2.0 * TESLA_DAMAGE);
        // Le charbon a été brûlé.
        assert!(!app.world().resource::<Chests>().0[&generator].has(Item::Coal, 1));
    }

    #[test]
    fn nothing_works_without_coal() {
        let (mut app, generator) = base(0, true);
        place(&mut app, generator + IVec3::new(4, 0, 0), Block::TeslaCoil);
        place(&mut app, generator + IVec3::new(0, 0, 4), Block::Floodlight);
        let z = zombie(&mut app, generator.as_vec3() + Vec3::new(6.5, 0.0, 4.5));
        run(&mut app, 3.0);
        assert_eq!(health(&app, z), 1000.0);
    }

    #[test]
    fn floodlights_burn_zombies_at_night_only_within_reach_of_power() {
        let (mut app, generator) = base(1, true);
        let near = generator + IVec3::new(0, 0, 6);
        place(&mut app, near, Block::Floodlight);
        // Trop loin du générateur : éteint.
        let far = generator + IVec3::new(0, 0, -16);
        place(&mut app, far, Block::Floodlight);
        let lit = zombie(&mut app, near.as_vec3() + Vec3::new(3.5, 0.0, 0.5));
        let dark = zombie(&mut app, far.as_vec3() + Vec3::new(3.5, 0.0, 0.5));
        run(&mut app, 4.0);
        assert!(health(&app, lit) < 1000.0 - 3.0 * FLOOD_DPS);
        assert_eq!(health(&app, dark), 1000.0);
    }

    #[test]
    fn an_electric_door_opens_for_a_player_and_closes_behind() {
        let (mut app, generator) = base(1, false);
        let door = generator + IVec3::new(5, 0, 0);
        place(&mut app, door, Block::ElectricDoor);
        let mut player = ConnectedPlayer::new(door.as_vec3() + Vec3::new(-1.0, 0.0, 0.5));
        player.name = "Alice".into();
        app.world_mut().resource_mut::<Lobby>().players.insert(1, player);
        run(&mut app, 1.0);
        assert_eq!(app.world().resource::<GameWorld>().block(door), Block::ElectricDoorOpen);
        app.world_mut()
            .resource_mut::<Lobby>()
            .players
            .get_mut(&1)
            .unwrap()
            .state
            .position = door.as_vec3() + Vec3::new(-8.0, 0.0, 0.5);
        run(&mut app, 1.0);
        assert_eq!(app.world().resource::<GameWorld>().block(door), Block::ElectricDoor);
    }
}
