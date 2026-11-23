//! Les autres joueurs : un personnage articulé qui suit les positions reçues du serveur,
//! marche, tourne la tête, tient son objet et frappe.

use std::collections::HashMap;

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

use bevy::prelude::*;
use shared::item::Item;
use shared::protocol::PlayerId;

use crate::AppState;
use crate::net::{NetEvent, Session};
use crate::viewmodel::HeldAssets;

pub struct RemotePlayersPlugin;

impl Plugin for RemotePlayersPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RemotePlayers>()
            .add_systems(Startup, setup_assets)
            .add_systems(
                Update,
                (handle_events, interpolate, animate, update_held)
                    .chain()
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(OnExit(AppState::Playing), clear);
    }
}

/// Joueurs distants affichés, par identifiant.
#[derive(Resource, Default)]
pub struct RemotePlayers {
    pub entities: HashMap<PlayerId, Entity>,
    pub names: HashMap<PlayerId, String>,
}

#[derive(Component)]
pub struct RemotePlayer {
    pub id: PlayerId,
    target: Vec3,
    target_yaw: f32,
    pitch: f32,
    swinging: bool,
    held: Option<Item>,
}

/// Membres articulés d'un joueur distant.
#[derive(Component)]
struct Rig {
    head: Entity,
    arms: [Entity; 2],
    legs: [Entity; 2],
    /// Main droite, où s'accroche l'objet tenu.
    hand: Entity,
    /// Objet affiché dans la main et son entité.
    shown: Option<(Item, Entity)>,
    /// Phase de la marche, amplitude du balancement et avancée du coup.
    stride: f32,
    gait: f32,
    swing: f32,
    last: Vec3,
}

#[derive(Resource)]
struct RemoteAssets {
    torso: Handle<Mesh>,
    head: Handle<Mesh>,
    limb: Handle<Mesh>,
    shirt: Handle<StandardMaterial>,
    skin: Handle<StandardMaterial>,
    pants: Handle<StandardMaterial>,
}

/// Hauteur des hanches, des épaules et du cou.
const HIP: f32 = 0.75;
const SHOULDER: f32 = 1.35;
const NECK: f32 = 1.4;

fn setup_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(RemoteAssets {
        torso: meshes.add(Cuboid::new(0.5, 0.65, 0.28)),
        head: meshes.add(Cuboid::new(0.42, 0.42, 0.42)),
        limb: meshes.add(Cuboid::new(0.2, 0.68, 0.2)),
        shirt: materials.add(Color::srgb(0.2, 0.35, 0.75)),
        skin: materials.add(Color::srgb(0.95, 0.8, 0.6)),
        pants: materials.add(Color::srgb(0.22, 0.2, 0.18)),
    });
}

fn spawn_remote(commands: &mut Commands, assets: &RemoteAssets, id: PlayerId, position: Vec3, yaw: f32) -> Entity {
    let root = commands
        .spawn((
            RemotePlayer {
                id,
                target: position,
                target_yaw: yaw,
                pitch: 0.0,
                swinging: false,
                held: None,
            },
            Transform::from_translation(position).with_rotation(Quat::from_rotation_y(yaw)),
            Visibility::default(),
        ))
        .id();
    commands.spawn((
        Mesh3d(assets.torso.clone()),
        MeshMaterial3d(assets.shirt.clone()),
        Transform::from_xyz(0.0, HIP + 0.325, 0.0),
        ChildOf(root),
    ));
    // Chaque membre pivote à son attache ; le maillage pend en dessous.
    let mut limb = |x: f32, y: f32, material: &Handle<StandardMaterial>| {
        let pivot = commands
            .spawn((Transform::from_xyz(x, y, 0.0), Visibility::Inherited, ChildOf(root)))
            .id();
        commands.spawn((
            Mesh3d(assets.limb.clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_xyz(0.0, -0.32, 0.0),
            ChildOf(pivot),
        ));
        pivot
    };
    let arms = [
        limb(-0.35, SHOULDER, &assets.shirt),
        limb(0.35, SHOULDER, &assets.shirt),
    ];
    let legs = [limb(-0.12, HIP, &assets.pants), limb(0.12, HIP, &assets.pants)];
    let head = commands
        .spawn((
            Transform::from_xyz(0.0, NECK, 0.0),
            Visibility::Inherited,
            ChildOf(root),
        ))
        .id();
    commands.spawn((
        Mesh3d(assets.head.clone()),
        MeshMaterial3d(assets.skin.clone()),
        Transform::from_xyz(0.0, 0.21, 0.0),
        ChildOf(head),
    ));
    let hand = commands
        .spawn((
            Transform::from_xyz(0.0, -0.62, 0.0),
            Visibility::Inherited,
            ChildOf(arms[1]),
        ))
        .id();
    commands.entity(root).insert(Rig {
        head,
        arms,
        legs,
        hand,
        shown: None,
        stride: 0.0,
        gait: 0.0,
        swing: 0.0,
        last: position,
    });
    root
}

fn handle_events(
    mut commands: Commands,
    mut events: MessageReader<NetEvent>,
    mut remotes: ResMut<RemotePlayers>,
    mut players: Query<&mut RemotePlayer>,
    session: Res<Session>,
    assets: Res<RemoteAssets>,
) {
    for event in events.read() {
        match event {
            NetEvent::PlayerJoined { id, name } => {
                remotes.names.insert(*id, name.clone());
            }
            NetEvent::PlayerLeft(id) => {
                remotes.names.remove(id);
                if let Some(entity) = remotes.entities.remove(id) {
                    commands.entity(entity).despawn();
                }
            }
            NetEvent::Players(list) => {
                for (id, state, held) in list {
                    if *id == session.my_id {
                        continue;
                    }
                    let entity = match remotes.entities.get(id) {
                        Some(entity) if players.contains(*entity) => *entity,
                        _ => {
                            let entity = spawn_remote(&mut commands, &assets, *id, state.position, state.yaw);
                            remotes.entities.insert(*id, entity);
                            commands.entity(entity).insert(RemotePlayer {
                                id: *id,
                                target: state.position,
                                target_yaw: state.yaw,
                                pitch: state.pitch,
                                swinging: state.swinging,
                                held: *held,
                            });
                            continue;
                        }
                    };
                    if let Ok(mut remote) = players.get_mut(entity) {
                        remote.target = state.position;
                        remote.target_yaw = state.yaw;
                        remote.pitch = state.pitch;
                        remote.swinging = state.swinging;
                        remote.held = *held;
                    }
                }
            }
            _ => {}
        }
    }
}

fn interpolate(time: Res<Time>, mut players: Query<(&RemotePlayer, &mut Transform)>) {
    let t = (time.delta_secs() * 12.0).min(1.0);
    for (remote, mut transform) in &mut players {
        transform.translation = transform.translation.lerp(remote.target, t);
        let target = Quat::from_rotation_y(remote.target_yaw);
        transform.rotation = transform.rotation.slerp(target, t);
    }
}

/// Marche (jambes et bras opposés), tête qui suit le regard, bras droit qui frappe.
fn animate(
    time: Res<Time>,
    mut players: Query<(&RemotePlayer, &Transform, &mut Rig)>,
    mut limbs: Query<&mut Transform, Without<RemotePlayer>>,
) {
    let dt = time.delta_secs().max(1e-4);
    for (remote, transform, mut rig) in &mut players {
        let moved = transform.translation - rig.last;
        rig.last = transform.translation;
        let speed = Vec2::new(moved.x, moved.z).length() / dt;
        rig.stride += speed * dt * 2.2;
        let target = (speed / 4.0).min(1.0);
        rig.gait += (target - rig.gait) * (dt * 10.0).min(1.0);
        let swing_angle = if remote.swinging || rig.swing > 0.0 {
            rig.swing = (rig.swing + dt / 0.3) % 1.0;
            if !remote.swinging && rig.swing < dt / 0.3 {
                rig.swing = 0.0;
            }
            Some(1.0 + 0.8 * (rig.swing * std::f32::consts::TAU).sin())
        } else {
            None
        };
        let walk = rig.stride.sin() * 0.7 * rig.gait;
        let rest = if remote.held.is_some() { 0.3 } else { 0.0 };
        let angles = [
            (rig.arms[0], -walk * 0.7),
            (rig.arms[1], swing_angle.unwrap_or(rest + walk * 0.7)),
            (rig.legs[0], walk),
            (rig.legs[1], -walk),
        ];
        for (entity, angle) in angles {
            if let Ok(mut limb) = limbs.get_mut(entity) {
                limb.rotation = Quat::from_rotation_x(angle);
            }
        }
        if let Ok(mut head) = limbs.get_mut(rig.head) {
            head.rotation = Quat::from_rotation_x(remote.pitch.clamp(-1.2, 1.2));
        }
    }
}

/// Affiche dans la main droite l'objet que tient chaque joueur.
fn update_held(mut commands: Commands, mut players: Query<(&RemotePlayer, &mut Rig)>, mut assets: HeldAssets) {
    for (remote, mut rig) in &mut players {
        if rig.shown.map(|(item, _)| item) == remote.held {
            continue;
        }
        if let Some((_, entity)) = rig.shown.take() {
            commands.entity(entity).despawn();
        }
        let Some(item) = remote.held else { continue };
        let (mesh, material) = assets.model(item);
        let transform = match item {
            Item::Block(_) => Transform::from_xyz(0.0, -0.05, -0.12).with_scale(Vec3::splat(0.25)),
            // L'icône (manche en bas à gauche) pointe vers l'avant, vue de côté.
            _ => Transform::from_xyz(0.0, -0.05, -0.14)
                .with_rotation(Quat::from_rotation_y(FRAC_PI_2) * Quat::from_rotation_z(-FRAC_PI_4))
                .with_scale(Vec3::splat(0.5)),
        };
        let entity = commands
            .spawn((Mesh3d(mesh), MeshMaterial3d(material), transform, ChildOf(rig.hand)))
            .id();
        rig.shown = Some((item, entity));
    }
}

fn clear(mut commands: Commands, mut remotes: ResMut<RemotePlayers>) {
    for entity in remotes.entities.values() {
        commands.entity(*entity).despawn();
    }
    *remotes = RemotePlayers::default();
}
