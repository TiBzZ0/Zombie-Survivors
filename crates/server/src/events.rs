//! Événements du jour, tirés au sort à chaque aube (à partir du jour 2) :
//! - **brouillard** : le soleil ne brûle plus les zombies et quelques-uns rôdent en plein jour ;
//! - **largage** : une caisse de ravitaillement tombe à distance d'un joueur, annoncée dans le chat ;
//! - **marchand** : un marchand passe près d'un joueur et repart avant la nuit.

use bevy::prelude::*;
use bevy_renet::RenetServer;
use shared::block::Block;
use shared::item::{Inventory, Item, Tier};
use shared::protocol::ServerMessage;

use crate::daynight::Clock;
use crate::net::{Lobby, WorldVersion, broadcast};
use crate::world::{Chests, GameWorld};
use crate::zombies::find_spawn_spot;

/// Chances, chaque jour, de brouillard, d'un largage et d'un marchand.
pub const FOG_CHANCE: f32 = 0.25;
pub const AIRDROP_CHANCE: f32 = 0.6;
pub const TRADER_CHANCE: f32 = 0.4;

pub struct EventsPlugin;

impl Plugin for EventsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Weather>()
            .init_resource::<Happenings>()
            .add_systems(FixedUpdate, (plan_day, airdrop, broadcast_weather).chain());
    }
}

/// Le temps qu'il fait.
#[derive(Resource, Default, Debug)]
pub struct Weather {
    pub fog: bool,
}

/// Ce qui est prévu aujourd'hui.
#[derive(Resource, Default, Debug)]
pub struct Happenings {
    day: u32,
    /// Moment du largage (avancée de la journée), s'il y en a un.
    pub airdrop_at: Option<f32>,
    /// Moment du passage du marchand.
    pub trader_at: Option<f32>,
    /// Le marchand doit apparaître maintenant (lu par les survivants).
    pub trader_due: bool,
    weather_timer: f32,
}

fn chat(server: Option<&mut RenetServer>, text: String) {
    if let Some(server) = server {
        broadcast(
            server,
            &ServerMessage::Chat {
                from: "Serveur".into(),
                text,
            },
        );
    }
}

/// À l'aube, tire au sort les événements du jour ; à la nuit, le brouillard se lève.
fn plan_day(
    clock: Res<Clock>,
    mut weather: ResMut<Weather>,
    mut plans: ResMut<Happenings>,
    mut server: Option<ResMut<RenetServer>>,
) {
    if clock.is_night() {
        weather.fog = false;
        plans.airdrop_at = None;
        plans.trader_at = None;
        return;
    }
    if plans.day == clock.day {
        if plans.trader_at.is_some_and(|at| clock.phase >= at) {
            plans.trader_at = None;
            plans.trader_due = true;
        }
        return;
    }
    plans.day = clock.day;
    if clock.day < 2 {
        return;
    }
    weather.fog = fastrand::f32() < FOG_CHANCE;
    plans.airdrop_at = (fastrand::f32() < AIRDROP_CHANCE).then(|| 0.08 + fastrand::f32() * 0.35);
    plans.trader_at = (fastrand::f32() < TRADER_CHANCE).then(|| 0.05 + fastrand::f32() * 0.25);
    if weather.fog {
        chat(
            server.as_deref_mut(),
            "Un épais brouillard se lève : le soleil ne brûlera pas les zombies aujourd'hui.".into(),
        );
    }
}

/// Contenu d'une caisse larguée.
pub fn airdrop_loot() -> Inventory {
    let mut loot = Inventory::default();
    loot.add(Item::Ammo, 10 + fastrand::u32(0..11));
    loot.add(Item::Ration, 2 + fastrand::u32(0..3));
    loot.add(Item::Bandage, 2);
    let mut maybe = |chance: f32, item: Item, n: u32| {
        if fastrand::f32() < chance {
            loot.add(item, n);
        }
    };
    maybe(0.5, Item::Antidote, 1);
    maybe(0.5, Item::Grenade, 2);
    maybe(0.4, Item::IronIngot, 4);
    maybe(0.3, Item::WaterFlask, 2);
    maybe(0.15, Item::Rifle, 1);
    maybe(0.15, Item::BodyArmor(Tier::Iron), 1);
    maybe(0.2, Item::Block(Block::Generator), 1);
    loot
}

/// Direction cardinale de `to` vu depuis `from` (le nord est vers -Z, en haut de la carte).
pub fn compass(from: Vec3, to: Vec3) -> &'static str {
    let (dx, dz) = (to.x - from.x, to.z - from.z);
    let angle = dx.atan2(-dz).to_degrees().rem_euclid(360.0);
    const NAMES: [&str; 8] = [
        "au nord",
        "au nord-est",
        "à l'est",
        "au sud-est",
        "au sud",
        "au sud-ouest",
        "à l'ouest",
        "au nord-ouest",
    ];
    NAMES[(((angle + 22.5) / 45.0) as usize) % 8]
}

/// Le moment venu, une caisse tombe entre 30 et 70 m d'un joueur.
#[allow(clippy::too_many_arguments)]
fn airdrop(
    clock: Res<Clock>,
    mut plans: ResMut<Happenings>,
    lobby: Res<Lobby>,
    mut world: ResMut<GameWorld>,
    mut chests: ResMut<Chests>,
    mut version: ResMut<WorldVersion>,
    mut server: Option<ResMut<RenetServer>>,
) {
    let Some(at) = plans.airdrop_at else { return };
    if clock.phase < at {
        return;
    }
    plans.airdrop_at = None;
    let players: Vec<(&str, Vec3)> = lobby
        .players
        .values()
        .filter(|p| p.is_alive())
        .map(|p| (p.name.as_str(), p.state.position))
        .collect();
    if players.is_empty() {
        return;
    }
    let (name, around) = players[fastrand::usize(0..players.len())];
    let Some(spot) = find_spawn_spot(&world, around, 30.0, 70.0) else {
        return;
    };
    let pos = spot.floor().as_ivec3();
    if !world.block(pos).is_replaceable() || !world.set_block(pos, Block::SupplyCrate) {
        return;
    }
    version.0 += 1;
    chests.0.insert(pos, airdrop_loot());
    let distance = around.distance(spot).round();
    let direction = compass(around, spot);
    info!("Largage en {pos}");
    if let Some(server) = server.as_deref_mut() {
        broadcast(
            server,
            &ServerMessage::BlockChanged {
                pos,
                block: Block::SupplyCrate,
            },
        );
    }
    chat(
        server.as_deref_mut(),
        format!(
            "Un largage de ravitaillement est tombé {direction}, à {distance} m de {name} (caisse orange sur la carte)."
        ),
    );
}

fn broadcast_weather(
    time: Res<Time>,
    weather: Res<Weather>,
    mut plans: ResMut<Happenings>,
    mut last: Local<Option<bool>>,
    mut server: Option<ResMut<RenetServer>>,
) {
    plans.weather_timer -= time.delta_secs();
    if *last == Some(weather.fog) && plans.weather_timer > 0.0 {
        return;
    }
    plans.weather_timer = 5.0;
    *last = Some(weather.fog);
    if let Some(server) = server.as_deref_mut() {
        broadcast(server, &ServerMessage::Weather { fog: weather.fog });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compass_points_the_right_way() {
        let o = Vec3::ZERO;
        assert_eq!(compass(o, Vec3::new(0.0, 0.0, -10.0)), "au nord");
        assert_eq!(compass(o, Vec3::new(10.0, 0.0, 0.0)), "à l'est");
        assert_eq!(compass(o, Vec3::new(0.0, 0.0, 10.0)), "au sud");
        assert_eq!(compass(o, Vec3::new(-7.0, 0.0, -7.0)), "au nord-ouest");
    }

    #[test]
    fn airdrops_always_carry_ammo_and_food() {
        for _ in 0..20 {
            let loot = airdrop_loot();
            assert!(loot.has(Item::Ammo, 10) && loot.has(Item::Ration, 2));
        }
    }
}
