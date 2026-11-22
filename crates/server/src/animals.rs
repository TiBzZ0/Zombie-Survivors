//! Gibier : des animaux se promènent dans les prairies, fuient les joueurs et
//! donnent de la viande au chasseur qui les abat. Le sanglier blessé charge.

use bevy::prelude::*;
use bevy_renet::RenetServer;
use bevy_renet::renet::{ClientId, DefaultChannel};
use shared::animal::{AnimalKind, AnimalState};
use shared::block::Block;
use shared::item::Item;
use shared::physics::step_body;
use shared::protocol::{ServerMessage, encode};

use crate::net::{Lobby, send_inventory};
use crate::world::GameWorld;
use crate::zombies::{ZombieSet, find_spawn_spot, hurt_player};

pub struct AnimalPlugin;

impl Plugin for AnimalPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NextAnimalId>().add_systems(
            FixedUpdate,
            (spawn_animals, animal_ai, animal_deaths, broadcast_animals)
                .chain()
                .after(ZombieSet::Ai)
                .before(ZombieSet::Deaths),
        );
    }
}

/// Animaux qu'on cherche à garder autour de chaque joueur.
const PER_PLAYER: usize = 5;
/// Distance à laquelle un animal remarque un joueur et s'enfuit.
const SCARE_DIST: f32 = 8.0;
/// Au-delà de cette distance de tout joueur, l'animal disparaît.
const DESPAWN_DIST: f32 = 120.0;
const VIEW_DIST: f32 = 90.0;
const JUMP_SPEED: f32 = 7.0;
/// Allonge et cadence de la charge du sanglier.
const BITE_REACH: f32 = 1.3;
const BITE_COOLDOWN: f32 = 1.2;

#[derive(Resource, Default)]
struct NextAnimalId(u32);

#[derive(Component, Debug)]
pub struct Animal {
    pub id: u32,
    pub kind: AnimalKind,
    pub health: f32,
    pub position: Vec3,
    velocity: Vec3,
    pub yaw: f32,
    on_ground: bool,
    hit_wall: bool,
    wander: Option<Vec3>,
    wander_timer: f32,
    /// Temps de fuite restant.
    flee: f32,
    /// Temps de colère restant (sanglier).
    angry: f32,
    pub hurt_timer: f32,
    attack_cooldown: f32,
    /// Dernier joueur à l'avoir blessé : il reçoit la viande.
    pub last_attacker: Option<ClientId>,
}

impl Animal {
    pub fn new(id: u32, kind: AnimalKind, position: Vec3) -> Self {
        Self {
            id,
            kind,
            health: kind.stats().health,
            position,
            velocity: Vec3::ZERO,
            yaw: fastrand::f32() * std::f32::consts::TAU,
            on_ground: false,
            hit_wall: false,
            wander: None,
            wander_timer: 0.0,
            flee: 0.0,
            angry: 0.0,
            hurt_timer: 0.0,
            attack_cooldown: 0.0,
            last_attacker: None,
        }
    }

    /// Blessure : l'animal fuit, ou charge s'il sait se défendre.
    pub fn hurt(&mut self, amount: f32, by: Option<ClientId>, push: Vec3) {
        self.health -= amount;
        self.hurt_timer = 0.3;
        self.velocity += push;
        if by.is_some() {
            self.last_attacker = by;
        }
        if self.kind.stats().damage > 0.0 {
            self.angry = 10.0;
        } else {
            self.flee = 6.0;
        }
    }

    /// Boîte de collision, pour les tirs.
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let size = self.kind.stats().size;
        let half = Vec3::new(size.half_width + 0.1, 0.0, size.half_width + 0.1);
        (self.position - half, self.position + half + Vec3::Y * size.height)
    }
}

fn random_kind() -> AnimalKind {
    match fastrand::u32(0..100) {
        0..50 => AnimalKind::Rabbit,
        50..80 => AnimalKind::Deer,
        _ => AnimalKind::Boar,
    }
}

fn spawn_animals(
    mut commands: Commands,
    world: Res<GameWorld>,
    lobby: Res<Lobby>,
    mut ids: ResMut<NextAnimalId>,
    animals: Query<(Entity, &Animal)>,
    mut tick: Local<u32>,
) {
    *tick += 1;
    if !tick.is_multiple_of(60) {
        return;
    }
    let players: Vec<Vec3> = lobby
        .players
        .values()
        .filter(|p| p.is_ready())
        .map(|p| p.state.position)
        .collect();
    for (entity, animal) in &animals {
        if players.iter().all(|p| p.distance(animal.position) > DESPAWN_DIST) {
            commands.entity(entity).despawn();
        }
    }
    for player in &players {
        let near = animals
            .iter()
            .filter(|(_, a)| a.position.distance(*player) < 64.0)
            .count();
        if near >= PER_PLAYER {
            continue;
        }
        // Dans une prairie, hors de vue immédiate.
        if let Some(spot) = find_spawn_spot(&world, *player, 20.0, 45.0)
            && world.block(spot.floor().as_ivec3() - IVec3::Y) == Block::Grass
        {
            ids.0 = ids.0.wrapping_add(1);
            commands.spawn(Animal::new(ids.0, random_kind(), spot));
        }
    }
}

fn animal_ai(
    time: Res<Time>,
    world: Res<GameWorld>,
    mut lobby: ResMut<Lobby>,
    mut server: Option<ResMut<RenetServer>>,
    mut animals: Query<&mut Animal>,
) {
    let dt = time.delta_secs();
    let players: Vec<(ClientId, Vec3)> = lobby
        .players
        .iter()
        .filter(|(_, p)| p.is_alive())
        .map(|(id, p)| (*id, p.state.position))
        .collect();
    for mut a in &mut animals {
        let stats = a.kind.stats();
        a.hurt_timer -= dt;
        a.flee -= dt;
        a.angry -= dt;
        a.attack_cooldown -= dt;
        let nearest = players
            .iter()
            .map(|(id, p)| (*id, *p, p.distance(a.position)))
            .min_by(|x, y| x.2.total_cmp(&y.2));
        if let Some((_, _, d)) = nearest
            && d < SCARE_DIST
            && stats.damage == 0.0
        {
            a.flee = a.flee.max(3.0);
        }
        let mut wish = Vec3::ZERO;
        let mut speed = stats.speed;
        match nearest {
            Some((id, target, d)) if a.angry > 0.0 => {
                // Le sanglier charge le joueur le plus proche.
                speed = stats.run_speed;
                wish = (target - a.position).with_y(0.0);
                if d < BITE_REACH + stats.size.half_width && a.attack_cooldown <= 0.0 {
                    a.attack_cooldown = BITE_COOLDOWN;
                    if let Some(player) = lobby.players.get_mut(&id) {
                        let (amount, _) = player.incoming_damage(stats.damage, a.position);
                        hurt_player(id, player, amount, "Encorné par un sanglier", server.as_deref_mut());
                    }
                }
            }
            Some((_, from, _)) if a.flee > 0.0 => {
                speed = stats.run_speed;
                wish = (a.position - from).with_y(0.0);
            }
            _ => {
                a.wander_timer -= dt;
                if a.wander_timer <= 0.0 {
                    a.wander_timer = 3.0 + fastrand::f32() * 6.0;
                    // Parfois, il broute sur place.
                    a.wander = (fastrand::f32() < 0.6).then(|| {
                        let angle = fastrand::f32() * std::f32::consts::TAU;
                        a.position + Vec3::new(angle.cos(), 0.0, angle.sin()) * (2.0 + fastrand::f32() * 6.0)
                    });
                }
                if let Some(goal) = a.wander {
                    let to = (goal - a.position).with_y(0.0);
                    if to.length() > 0.5 {
                        wish = to;
                    }
                }
            }
        }
        let wish = wish.normalize_or_zero() * speed;
        if wish != Vec3::ZERO {
            a.yaw = (-wish.x).atan2(-wish.z);
        }
        let mut velocity = Vec3::new(wish.x, a.velocity.y, wish.z);
        if a.on_ground && a.hit_wall && wish != Vec3::ZERO {
            velocity.y = JUMP_SPEED;
        }
        let result = step_body(a.position, velocity, stats.size, dt, &|p| world.is_solid(p) || p.y < 0);
        a.position = result.position;
        a.velocity = Vec3::new(0.0, result.velocity.y, 0.0);
        a.on_ground = result.on_ground;
        a.hit_wall = result.hit_wall;
    }
}

/// Un animal abattu donne sa viande au dernier joueur qui l'a blessé.
fn animal_deaths(
    mut commands: Commands,
    mut lobby: ResMut<Lobby>,
    mut server: Option<ResMut<RenetServer>>,
    animals: Query<(Entity, &Animal)>,
) {
    for (entity, a) in &animals {
        if a.health > 0.0 && a.position.y > -10.0 {
            continue;
        }
        commands.entity(entity).despawn();
        let (min, max) = a.kind.stats().meat;
        if let Some(id) = a.last_attacker
            && let Some(player) = lobby.players.get_mut(&id)
        {
            player.inventory.add(Item::RawMeat, fastrand::u32(min..=max));
            if let Some(server) = server.as_deref_mut() {
                send_inventory(server, id, player);
            }
        }
    }
}

fn broadcast_animals(
    mut server: Option<ResMut<RenetServer>>,
    lobby: Res<Lobby>,
    animals: Query<&Animal>,
    mut tick: Local<u32>,
) {
    *tick += 1;
    if !tick.is_multiple_of(3) {
        return;
    }
    let Some(server) = server.as_deref_mut() else { return };
    for (id, player) in lobby.players.iter().filter(|(_, p)| p.is_ready()) {
        let states: Vec<AnimalState> = animals
            .iter()
            .filter(|a| a.health > 0.0 && a.position.distance(player.state.position) < VIEW_DIST)
            .map(|a| {
                let mut flags = 0;
                if a.flee > 0.0 || a.angry > 0.0 {
                    flags |= AnimalState::RUNNING;
                }
                if a.hurt_timer > 0.0 {
                    flags |= AnimalState::HURT;
                }
                AnimalState::new(a.id, a.position, a.yaw, a.kind, flags)
            })
            .collect();
        server.send_message(*id, DefaultChannel::Unreliable, encode(&ServerMessage::Animals(states)));
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;

    use super::*;
    use crate::net::ConnectedPlayer;

    #[test]
    fn hunter_gets_the_meat() {
        let mut app = App::new();
        let mut lobby = Lobby::default();
        lobby.players.insert(5, ConnectedPlayer::new(Vec3::ZERO));
        app.insert_resource(lobby);
        let mut deer = Animal::new(1, AnimalKind::Deer, Vec3::new(3.0, 0.0, 0.0));
        deer.hurt(15.0, Some(5), Vec3::ZERO);
        assert!(deer.flee > 0.0, "le cerf blessé s'enfuit");
        deer.hurt(30.0, None, Vec3::ZERO);
        app.world_mut().spawn(deer);
        app.world_mut().run_system_once(animal_deaths).unwrap();
        let meat = app.world().resource::<Lobby>().players[&5]
            .inventory
            .count(Item::RawMeat);
        assert!((2..=3).contains(&meat), "{meat} viandes");
        let mut boar = Animal::new(2, AnimalKind::Boar, Vec3::ZERO);
        boar.hurt(5.0, Some(5), Vec3::ZERO);
        assert!(boar.angry > 0.0, "le sanglier blessé charge");
    }
}
