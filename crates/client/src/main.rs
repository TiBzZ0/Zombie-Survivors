//! Client du jeu.
//!
//! Pour l'instant : mode local uniquement. Le client démarre le serveur dans un
//! thread et affiche le terrain avec une caméra libre. Le réseau, le joueur et
//! ses collisions arrivent au jalon 0.

mod meshing;
mod terrain;

use bevy::camera_controller::free_camera::{FreeCamera, FreeCameraPlugin};
use bevy::prelude::*;
use server::ServerConfig;

use crate::terrain::{TerrainPlugin, spawn_point};

fn main() {
    let server_config = ServerConfig::default();
    let world = server_config.world.clone();

    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Zombie Survivors".into(),
            ..default()
        }),
        ..default()
    }));

    // Mode local : le même serveur que dans Docker, lancé dans un thread.
    server::spawn_local(server_config);
    info!("Mode local : serveur démarré dans un thread");

    app.insert_resource(ClearColor(Color::srgb(0.53, 0.72, 0.90)))
        .add_plugins(FreeCameraPlugin)
        .add_plugins(TerrainPlugin {
            world: world.clone(),
        })
        .add_systems(Startup, move |commands: Commands| {
            setup_scene(commands, spawn_point(&world))
        })
        .run();
}

fn setup_scene(mut commands: Commands, spawn: Vec3) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_translation(spawn).looking_to(Vec3::new(1.0, -0.3, 1.0), Vec3::Y),
        FreeCamera {
            walk_speed: 10.0,
            run_speed: 40.0,
            ..default()
        },
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 8_000.0,
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 0.0).looking_to(Vec3::new(-0.4, -1.0, -0.3), Vec3::Y),
    ));
    commands.insert_resource(GlobalAmbientLight {
        brightness: 600.0,
        ..default()
    });
}
