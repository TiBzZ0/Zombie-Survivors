//! Explosifs lancés : grenades et dynamite. Ils roulent, rebondissent, puis explosent
//! en blessant tout ce qui est autour (la dynamite détruit aussi les blocs).

use bevy::prelude::*;
use bevy_renet::RenetServer;
use bevy_renet::renet::ClientId;
use shared::block::Block;
use shared::item::Item;
use shared::physics::step_projectile;
use shared::protocol::ServerMessage;

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
}

impl Projectile {
    pub fn thrown(owner: ClientId, item: Item, eye: Vec3, dir: Vec3) -> Self {
        let fuse = item.throwable().map_or(2.0, |t| t.fuse);
        Self {
            owner: Some(owner),
            item,
            position: eye + dir * 0.5,
            // Un peu vers le haut, pour lancer en cloche.
            velocity: (dir + Vec3::Y * 0.25).normalize_or_zero() * THROW_SPEED,
            fuse,
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
) {
    let dt = time.delta_secs();
    let mut exploded = Vec::new();
    projectiles.0.retain_mut(|p| {
        (p.position, p.velocity) = step_projectile(p.position, p.velocity, dt, &|b| world.is_solid(b));
        p.fuse -= dt;
        if p.fuse <= 0.0 {
            exploded.push(p.clone());
        }
        p.fuse > 0.0 && p.position.y > -10.0
    });
    for p in exploded {
        let Some(blast) = p.item.throwable() else { continue };
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
}
