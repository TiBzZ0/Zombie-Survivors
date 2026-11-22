//! Les autres joueurs : un personnage simple qui suit les positions reçues du serveur.

use std::collections::HashMap;

use bevy::prelude::*;
use shared::protocol::PlayerId;

use crate::AppState;
use crate::net::{NetEvent, Session};

pub struct RemotePlayersPlugin;

impl Plugin for RemotePlayersPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RemotePlayers>()
            .add_systems(Startup, setup_assets)
            .add_systems(
                Update,
                (handle_events, interpolate).chain().run_if(in_state(AppState::Playing)),
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
}

#[derive(Resource)]
struct RemoteAssets {
    body: Handle<Mesh>,
    head: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    visor: Handle<StandardMaterial>,
}

fn setup_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(RemoteAssets {
        body: meshes.add(Cuboid::new(0.6, 1.3, 0.35)),
        head: meshes.add(Cuboid::new(0.45, 0.45, 0.45)),
        material: materials.add(Color::srgb(0.2, 0.35, 0.75)),
        visor: materials.add(Color::srgb(0.95, 0.8, 0.6)),
    });
}

fn spawn_remote(commands: &mut Commands, assets: &RemoteAssets, id: PlayerId, position: Vec3, yaw: f32) -> Entity {
    commands
        .spawn((
            RemotePlayer {
                id,
                target: position,
                target_yaw: yaw,
            },
            Transform::from_translation(position).with_rotation(Quat::from_rotation_y(yaw)),
            Visibility::default(),
            children![
                (
                    Mesh3d(assets.body.clone()),
                    MeshMaterial3d(assets.material.clone()),
                    Transform::from_xyz(0.0, 0.65, 0.0),
                ),
                (
                    Mesh3d(assets.head.clone()),
                    MeshMaterial3d(assets.visor.clone()),
                    Transform::from_xyz(0.0, 1.55, 0.0),
                ),
            ],
        ))
        .id()
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
                for (id, state) in list {
                    if *id == session.my_id {
                        continue;
                    }
                    match remotes.entities.get(id).and_then(|e| players.get_mut(*e).ok()) {
                        Some(mut remote) => {
                            remote.target = state.position;
                            remote.target_yaw = state.yaw;
                        }
                        None => {
                            let entity = spawn_remote(&mut commands, &assets, *id, state.position, state.yaw);
                            remotes.entities.insert(*id, entity);
                        }
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

fn clear(mut commands: Commands, mut remotes: ResMut<RemotePlayers>) {
    for entity in remotes.entities.values() {
        commands.entity(*entity).despawn();
    }
    *remotes = RemotePlayers::default();
}
