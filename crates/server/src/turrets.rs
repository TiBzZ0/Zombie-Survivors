//! Tourelles automatiques : un bloc qui stocke des munitions comme un coffre
//! et tire seul sur le zombie visible le plus proche.

use std::collections::HashMap;

use bevy::prelude::*;
use bevy_renet::RenetServer;
use shared::block::Block;
use shared::item::Item;
use shared::protocol::ServerMessage;

use crate::net::{Lobby, broadcast, send_chest_to_viewers};
use crate::survivors::clear_line;
use crate::world::{Chests, GameWorld};
use crate::zombies::{Zombie, ZombieBody, ZombieSet};

pub struct TurretPlugin;

impl Plugin for TurretPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TurretCooldowns>()
            .add_systems(FixedUpdate, turret_fire.after(ZombieSet::Ai).before(ZombieSet::Deaths));
    }
}

/// Munition tirée par une tourelle : dégâts, portée, cadence.
pub struct TurretShot {
    pub ammo: Item,
    pub damage: f32,
    pub range: f32,
    pub cooldown: f32,
}

/// Les balles sont préférées aux flèches quand la tourelle a les deux.
pub const TURRET_SHOTS: [TurretShot; 2] = [
    TurretShot {
        ammo: Item::Ammo,
        damage: 55.0,
        range: 24.0,
        cooldown: 0.8,
    },
    TurretShot {
        ammo: Item::Arrow,
        damage: 30.0,
        range: 18.0,
        cooldown: 1.2,
    },
];

/// Temps restant avant le prochain tir de chaque tourelle.
#[derive(Resource, Default)]
struct TurretCooldowns(HashMap<IVec3, f32>);

fn turret_fire(
    time: Res<Time>,
    world: Res<GameWorld>,
    mut chests: ResMut<Chests>,
    lobby: Res<Lobby>,
    mut cooldowns: ResMut<TurretCooldowns>,
    mut zombies: Query<(&mut Zombie, &ZombieBody)>,
    mut server: Option<ResMut<RenetServer>>,
) {
    let dt = time.delta_secs();
    let turrets: Vec<IVec3> = chests
        .0
        .keys()
        .copied()
        .filter(|pos| world.block(*pos) == Block::Turret)
        .collect();
    cooldowns.0.retain(|pos, _| turrets.contains(pos));
    for pos in turrets {
        let cooldown = cooldowns.0.entry(pos).or_insert(0.0);
        *cooldown -= dt;
        if *cooldown > 0.0 {
            continue;
        }
        let Some(shot) = chests
            .0
            .get(&pos)
            .and_then(|inv| TURRET_SHOTS.iter().find(|s| inv.has(s.ammo, 1)))
        else {
            continue;
        };
        // Le canon dépasse au-dessus du bloc.
        let eye = pos.as_vec3() + Vec3::new(0.5, 1.2, 0.5);
        let target = zombies
            .iter()
            .filter(|(z, _)| z.health > 0.0)
            .map(|(z, b)| (z.id, b.position + Vec3::Y * 1.0))
            .filter(|(_, aim)| aim.distance(eye) <= shot.range)
            .filter(|(_, aim)| clear_line(&world, eye, *aim))
            .min_by(|a, b| a.1.distance(eye).total_cmp(&b.1.distance(eye)));
        let Some((zombie_id, aim)) = target else {
            continue;
        };
        *cooldown = shot.cooldown;
        if let Some(inv) = chests.0.get_mut(&pos) {
            inv.remove(shot.ammo, 1);
        }
        if let Some((mut z, _)) = zombies.iter_mut().find(|(z, _)| z.id == zombie_id) {
            z.health -= shot.damage * z.kind.armor_factor(false);
            z.hurt_timer = 0.3;
        }
        if let Some(server) = server.as_deref_mut() {
            send_chest_to_viewers(server, &lobby, &chests, pos);
            broadcast(
                server,
                &ServerMessage::Shot {
                    from: 0,
                    origin: eye,
                    end: aim,
                    rifle: shot.ammo == Item::Ammo,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;
    use shared::config::WorldConfig;
    use shared::item::Inventory;
    use shared::world::WorldState;
    use shared::zombie::ZombieKind;

    use super::*;

    #[test]
    fn turret_shoots_the_nearest_zombie_and_uses_ammo() {
        let mut world = GameWorld(WorldState::new(WorldConfig::default()));
        let y = world.generator().surface_height(0, 0) + 12;
        let pos = IVec3::new(0, y, 0);
        assert!(world.set_block(pos, Block::Turret));
        let mut ammo = Inventory::default();
        ammo.add(Item::Arrow, 3);
        ammo.add(Item::Ammo, 1);
        let mut chests = Chests::default();
        chests.0.insert(pos, ammo);

        let mut app = App::new();
        app.insert_resource(world)
            .insert_resource(chests)
            .init_resource::<Lobby>()
            .init_resource::<TurretCooldowns>()
            .init_resource::<Time>();
        let zombie = |id: u32, x: f32| {
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
                    frenzy: 0.0,
                    fuse: None,
                    trapped: 0.0,
                },
                ZombieBody {
                    position: Vec3::new(x, y as f32, 0.5),
                    velocity: Vec3::ZERO,
                    yaw: 0.0,
                    on_ground: true,
                    hit_wall: false,
                },
            )
        };
        app.world_mut().spawn(zombie(1, 10.5));
        app.world_mut().spawn(zombie(2, 5.5));
        app.world_mut().spawn(zombie(3, 60.5));

        app.world_mut().run_system_once(turret_fire).unwrap();
        let mut health: Vec<(u32, f32)> = app
            .world_mut()
            .query::<&Zombie>()
            .iter(app.world())
            .map(|z| (z.id, z.health))
            .collect();
        health.sort_by_key(|h| h.0);
        // La cartouche part d'abord, sur le zombie le plus proche.
        assert_eq!(health, vec![(1, 100.0), (2, 45.0), (3, 100.0)]);
        let chests = app.world().resource::<Chests>();
        assert_eq!(chests.0[&pos].count(Item::Ammo), 0);
        assert_eq!(chests.0[&pos].count(Item::Arrow), 3);

        // Rechargement en cours : pas de second tir tout de suite.
        app.world_mut().run_system_once(turret_fire).unwrap();
        assert_eq!(app.world().resource::<Chests>().0[&pos].count(Item::Arrow), 3);
    }
}
