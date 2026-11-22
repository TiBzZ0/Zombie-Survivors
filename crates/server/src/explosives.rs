//! Explosifs lancés : grenades et dynamite. Ils roulent, rebondissent, puis explosent
//! en blessant tout ce qui est autour (la dynamite détruit aussi les blocs).

use bevy::prelude::*;
use bevy_renet::RenetServer;
use bevy_renet::renet::ClientId;
use shared::block::{Block, Emplacement};
use shared::item::{Item, Throwable};
use shared::physics::{PROJECTILE_GRAVITY, step_projectile};
use shared::protocol::ServerMessage;

use crate::animals::Animal;
use crate::net::{Lobby, WorldVersion, broadcast};
use crate::survivors::Survivor;
use crate::world::{Deposits, GameWorld};
use crate::zombies::{BlockDamage, Zombie, ZombieBody, ZombieSet, hurt_player};

pub struct ExplosivesPlugin;

impl Plugin for ExplosivesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Projectiles>().add_systems(
            FixedUpdate,
            update_projectiles.after(ZombieSet::Ai).before(ZombieSet::Deaths),
        );
    }
}

/// Délai entre deux lancers.
pub const THROW_COOLDOWN: f32 = 0.8;
/// Vitesse d'un objet lancé, en m/s.
const THROW_SPEED: f32 = 14.0;
/// Les joueurs et survivants pris dans une explosion ne subissent qu'une part des dégâts.
const FRIENDLY_FACTOR: f32 = 0.4;

/// Un explosif en vol ou posé, mèche allumée.
#[derive(Clone, Debug)]
pub struct Projectile {
    pub owner: Option<ClientId>,
    pub item: Item,
    pub position: Vec3,
    pub velocity: Vec3,
    pub fuse: f32,
    pub blast: Throwable,
    /// Explose dès qu'il touche un bloc ou un zombie (boulet, pierre de catapulte).
    pub impact: bool,
    /// Part de la gravité subie (projectiles à impact seulement).
    pub gravity: f32,
}

impl Projectile {
    pub fn thrown(owner: ClientId, item: Item, eye: Vec3, dir: Vec3) -> Self {
        let blast = item.throwable().unwrap_or(Throwable {
            fuse: 2.0,
            radius: 3.0,
            damage: 50.0,
            blast_blocks: 0.0,
        });
        Self {
            owner: Some(owner),
            item,
            position: eye + dir * 0.5,
            // Un peu vers le haut, pour lancer en cloche.
            velocity: (dir + Vec3::Y * 0.25).normalize_or_zero() * THROW_SPEED,
            fuse: blast.fuse,
            blast,
            impact: false,
            gravity: 1.0,
        }
    }

    /// Projectile tiré par une arme fixe, qui explose à l'impact.
    pub fn shell(owner: ClientId, gun: Emplacement, origin: Vec3, dir: Vec3) -> Self {
        Self {
            owner: Some(owner),
            item: gun.ammo,
            position: origin,
            velocity: dir * gun.speed,
            // Au bout de ce temps, il explose en l'air.
            fuse: 8.0,
            blast: Throwable {
                fuse: 0.0,
                radius: gun.radius,
                damage: gun.damage,
                blast_blocks: 0.0,
            },
            impact: true,
            gravity: gun.gravity,
        }
    }

    pub fn message(&self) -> ServerMessage {
        ServerMessage::Thrown {
            origin: self.position,
            velocity: self.velocity,
            fuse: self.fuse,
            item: self.item,
        }
    }
}

/// Explosifs dont la mèche brûle.
#[derive(Resource, Default)]
pub struct Projectiles(pub Vec<Projectile>);

#[allow(clippy::too_many_arguments)]
fn update_projectiles(
    time: Res<Time>,
    mut projectiles: ResMut<Projectiles>,
    mut world: ResMut<GameWorld>,
    mut version: ResMut<WorldVersion>,
    mut damage: ResMut<BlockDamage>,
    mut deposits: ResMut<Deposits>,
    mut lobby: ResMut<Lobby>,
    mut server: Option<ResMut<RenetServer>>,
    mut zombies: Query<(&mut Zombie, &ZombieBody)>,
    mut survivors: Query<&mut Survivor>,
    mut animals: Query<&mut Animal>,
) {
    let dt = time.delta_secs();
    let mut exploded = Vec::new();
    let targets: Vec<Vec3> = zombies
        .iter()
        .map(|(z, b)| b.position + Vec3::Y * z.kind.stats().size.height * 0.5)
        .collect();
    projectiles.0.retain_mut(|p| {
        if p.impact {
            p.velocity.y -= PROJECTILE_GRAVITY * p.gravity * dt;
            let next = p.position + p.velocity * dt;
            // Un zombie touché de plein fouet : l'explosion a lieu sur lui.
            let hit_zombie = targets
                .iter()
                .find(|t| distance_to_segment(**t, p.position, next) < 1.0);
            if let Some(target) = hit_zombie {
                p.position = *target;
                p.fuse = 0.0;
            } else if world.is_solid(next.floor().as_ivec3()) {
                p.fuse = 0.0;
            } else {
                p.position = next;
            }
        } else {
            (p.position, p.velocity) = step_projectile(p.position, p.velocity, dt, &|b| world.is_solid(b));
        }
        p.fuse -= dt;
        if p.fuse <= 0.0 {
            exploded.push(p.clone());
        }
        p.fuse > 0.0 && p.position.y > -10.0
    });
    for p in exploded {
        let blast = p.blast;
        let falloff = |d: f32| blast.damage * (1.0 - 0.5 * d / blast.radius);
        for (mut z, body) in &mut zombies {
            let center = body.position + Vec3::Y * z.kind.stats().size.height * 0.5;
            let d = center.distance(p.position);
            if d < blast.radius {
                z.health -= falloff(d);
                z.hurt_timer = 0.3;
                let away = (center - p.position).with_y(0.0).normalize_or_zero();
                z.knockback = away * 10.0 + Vec3::Y * 5.0;
                if p.owner.is_some() {
                    z.last_attacker = p.owner;
                }
            }
        }
        for (id, player) in lobby.players.iter_mut().filter(|(_, pl)| pl.is_alive()) {
            let d = (player.state.position + Vec3::Y).distance(p.position);
            if d < blast.radius {
                let amount = falloff(d) * FRIENDLY_FACTOR * (1.0 - player.inventory.armor_reduction());
                hurt_player(*id, player, amount, "Pris dans une explosion", server.as_deref_mut());
            }
        }
        for mut a in &mut animals {
            let (min, max) = a.bounds();
            let center = (min + max) * 0.5;
            let d = center.distance(p.position);
            if d < blast.radius {
                let away = (center - p.position).with_y(0.0).normalize_or_zero();
                a.hurt(falloff(d), p.owner, away * 8.0 + Vec3::Y * 4.0);
            }
        }
        for mut s in &mut survivors {
            let d = (s.position + Vec3::Y).distance(p.position);
            if d < blast.radius {
                s.hurt(falloff(d) * FRIENDLY_FACTOR);
            }
        }
        let mut destroyed = Vec::new();
        if blast.blast_blocks > 0.0 {
            let r = blast.blast_blocks.ceil() as i32;
            let center = p.position.floor().as_ivec3();
            for x in -r..=r {
                for y in -r..=r {
                    for z in -r..=r {
                        let pos = center + IVec3::new(x, y, z);
                        let block = world.block(pos);
                        let close = (pos.as_vec3() + Vec3::splat(0.5)).distance(p.position) <= blast.blast_blocks;
                        if close && pos.y > 0 && !resists_blast(block) && world.set_block(pos, Block::Air) {
                            damage.0.remove(&pos);
                            deposits.0.remove(&pos);
                            destroyed.push(pos);
                        }
                    }
                }
            }
        }
        if !destroyed.is_empty() {
            version.0 += 1;
        }
        if let Some(server) = server.as_deref_mut() {
            for pos in destroyed {
                broadcast(server, &ServerMessage::BlockChanged { pos, block: Block::Air });
            }
            broadcast(
                server,
                &ServerMessage::Explosion {
                    pos: p.position,
                    radius: blast.radius,
                },
            );
        }
    }
}

/// Distance d'un point au segment [a, b].
fn distance_to_segment(point: Vec3, a: Vec3, b: Vec3) -> f32 {
    let ab = b - a;
    let k = if ab.length_squared() > 0.0 {
        ((point - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0)
    } else {
        0.0
    };
    point.distance(a + ab * k)
}

/// Blocs que la dynamite épargne : le vide, les murs renforcés, les coffres et tourelles (et leur contenu).
fn resists_blast(block: Block) -> bool {
    block == Block::Air || block == Block::ReinforcedWall || block.is_storage()
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;
    use shared::config::WorldConfig;
    use shared::world::WorldState;
    use shared::zombie::ZombieKind;

    use super::*;

    #[test]
    fn dynamite_blasts_zombies_and_blocks_but_spares_reinforced_walls() {
        let mut world = GameWorld(WorldState::new(WorldConfig::default()));
        let y = world.generator().surface_height(10, 10) + 10;
        let center = IVec3::new(10, y, 10);
        assert!(world.set_block(center + IVec3::X, Block::Planks));
        assert!(world.set_block(center - IVec3::X, Block::ReinforcedWall));
        assert!(world.set_block(center + IVec3::Z * 2, Block::Stone));

        let mut app = App::new();
        app.insert_resource(world)
            .init_resource::<WorldVersion>()
            .init_resource::<BlockDamage>()
            .init_resource::<Deposits>()
            .init_resource::<Lobby>()
            .init_resource::<Time>()
            .insert_resource(Projectiles(vec![Projectile {
                owner: None,
                item: Item::Dynamite,
                position: center.as_vec3() + Vec3::splat(0.5),
                velocity: Vec3::ZERO,
                fuse: 0.0,
                blast: Item::Dynamite.throwable().unwrap(),
                impact: false,
                gravity: 1.0,
            }]));
        let zombie = |id: u32, offset: f32| {
            (
                Zombie {
                    id,
                    kind: ZombieKind::Walker,
                    health: 100.0,
                    attack_cooldown: 0.0,
                    attacking: 0.0,
                    hurt_timer: 0.0,
                    knockback: Vec3::ZERO,
                    last_attacker: None,
                    climb_from: None,
                    stuck: 0.0,
                },
                ZombieBody {
                    position: center.as_vec3() + Vec3::new(0.5, -0.5, 0.5 + offset),
                    velocity: Vec3::ZERO,
                    yaw: 0.0,
                    on_ground: true,
                    hit_wall: false,
                },
            )
        };
        app.world_mut().spawn(zombie(1, 0.0));
        app.world_mut().spawn(zombie(2, 30.0));
        app.world_mut().run_system_once(update_projectiles).unwrap();

        let world = app.world().resource::<GameWorld>();
        assert_eq!(world.block(center + IVec3::X), Block::Air);
        assert_eq!(world.block(center + IVec3::Z * 2), Block::Air);
        assert_eq!(world.block(center - IVec3::X), Block::ReinforcedWall);
        assert!(app.world().resource::<Projectiles>().0.is_empty());
        let mut health: Vec<(u32, f32)> = app
            .world_mut()
            .query::<&Zombie>()
            .iter(app.world())
            .map(|z| (z.id, z.health))
            .collect();
        health.sort_by_key(|h| h.0);
        assert!(health[0].1 <= 0.0, "le zombie au centre est tué : {health:?}");
        assert_eq!(health[1].1, 100.0);
    }

    #[test]
    fn cannon_shell_explodes_on_the_zombie_it_hits() {
        let world = GameWorld(WorldState::new(WorldConfig::default()));
        let y = world.generator().surface_height(40, 40) as f32 + 12.0;
        let mut app = App::new();
        app.insert_resource(world)
            .init_resource::<WorldVersion>()
            .init_resource::<BlockDamage>()
            .init_resource::<Deposits>()
            .init_resource::<Lobby>()
            .init_resource::<Time>()
            .insert_resource(Projectiles(vec![Projectile::shell(
                1,
                Block::Cannon.emplacement().unwrap(),
                Vec3::new(30.5, y + 1.0, 40.5),
                Vec3::X,
            )]));
        app.world_mut().spawn((
            Zombie {
                id: 7,
                kind: ZombieKind::Walker,
                health: 100.0,
                attack_cooldown: 0.0,
                attacking: 0.0,
                hurt_timer: 0.0,
                knockback: Vec3::ZERO,
                last_attacker: None,
                climb_from: None,
                stuck: 0.0,
            },
            ZombieBody {
                position: Vec3::new(42.5, y, 40.5),
                velocity: Vec3::ZERO,
                yaw: 0.0,
                on_ground: true,
                hit_wall: false,
            },
        ));
        for _ in 0..30 {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::from_millis(33));
            app.world_mut().run_system_once(update_projectiles).unwrap();
        }
        assert!(
            app.world().resource::<Projectiles>().0.is_empty(),
            "le boulet a explosé"
        );
        let z = app.world_mut().query::<&Zombie>().single(app.world()).unwrap();
        assert!(z.health < 0.0, "touché de plein fouet : {}", z.health);
        assert_eq!(z.last_attacker, Some(1));
    }
}
