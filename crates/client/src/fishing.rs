//! Pêche : bouchon qui flotte et plonge, ligne tendue depuis la canne, annonce de la prise.

use bevy::prelude::*;
use shared::fishing::BITE_WINDOW;

use crate::AppState;
use crate::net::NetEvent;
use crate::player::LocalPlayer;

pub struct FishingPlugin;

impl Plugin for FishingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Playing), spawn_tackle).add_systems(
            Update,
            (fishing_events, draw_tackle)
                .chain()
                .run_if(in_state(AppState::Playing)),
        );
    }
}

#[derive(Component)]
struct Bobber;

#[derive(Component)]
struct FishingLine;

fn spawn_tackle(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mat = |materials: &mut Assets<StandardMaterial>, color: Color| {
        materials.add(StandardMaterial {
            base_color: color,
            perceptual_roughness: 0.6,
            ..default()
        })
    };
    let red = mat(&mut materials, Color::srgb(0.9, 0.12, 0.1));
    let white = mat(&mut materials, Color::srgb(0.95, 0.95, 0.92));
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    // Bouchon : moitié rouge en haut, moitié blanche dans l'eau, et une petite antenne.
    commands
        .spawn((
            DespawnOnExit(AppState::Playing),
            Bobber,
            Transform::default(),
            Visibility::Hidden,
        ))
        .with_children(|b| {
            b.spawn((
                Mesh3d(cube.clone()),
                MeshMaterial3d(white.clone()),
                Transform::from_xyz(0.0, -0.04, 0.0).with_scale(Vec3::new(0.16, 0.1, 0.16)),
            ));
            b.spawn((
                Mesh3d(cube.clone()),
                MeshMaterial3d(red.clone()),
                Transform::from_xyz(0.0, 0.06, 0.0).with_scale(Vec3::new(0.16, 0.1, 0.16)),
            ));
            b.spawn((
                Mesh3d(cube.clone()),
                MeshMaterial3d(red),
                Transform::from_xyz(0.0, 0.16, 0.0).with_scale(Vec3::new(0.03, 0.12, 0.03)),
            ));
        });
    commands.spawn((
        DespawnOnExit(AppState::Playing),
        FishingLine,
        Mesh3d(cube),
        MeshMaterial3d(white),
        Transform::default(),
        Visibility::Hidden,
    ));
}

fn fishing_events(mut commands: Commands, mut events: MessageReader<NetEvent>, mut player: ResMut<LocalPlayer>) {
    for event in events.read() {
        match event {
            NetEvent::Bite => {
                if let Some((_, bite)) = player.fishing.as_mut() {
                    *bite = BITE_WINDOW;
                }
            }
            NetEvent::Caught(item) => {
                player.fishing = None;
                if let Some((item, n)) = item {
                    let count = if *n > 1 { format!(" ×{n}") } else { String::new() };
                    crate::skills::popup(
                        &mut commands,
                        format!("Prise : {}{count}", item.name()),
                        22.0,
                        Color::srgb(0.55, 0.85, 1.0),
                        46.0,
                        2.0,
                    );
                }
            }
            _ => {}
        }
    }
}

fn draw_tackle(
    time: Res<Time>,
    mut player: ResMut<LocalPlayer>,
    camera: Single<&GlobalTransform, With<Camera3d>>,
    mut bobber: Single<(&mut Transform, &mut Visibility), (With<Bobber>, Without<FishingLine>)>,
    mut line: Single<(&mut Transform, &mut Visibility), (With<FishingLine>, Without<Bobber>)>,
) {
    let Some((at, bite)) = player.fishing else {
        *bobber.1 = Visibility::Hidden;
        *line.1 = Visibility::Hidden;
        return;
    };
    let t = time.elapsed_secs();
    // Un léger roulis ; quand ça mord, il plonge par saccades.
    let dip = if bite > 0.0 {
        -0.22 - 0.08 * (t * 30.0).sin().abs()
    } else {
        0.03 * (t * 2.5).sin()
    };
    if let Some((_, b)) = player.fishing.as_mut() {
        *b = (*b - time.delta_secs()).max(0.0);
    }
    let float = at.as_vec3() + Vec3::new(0.5, 0.92 + dip, 0.5);
    *bobber.0 = Transform::from_translation(float);
    *bobber.1 = Visibility::Visible;
    // Ligne : du bout de la canne (en haut à droite de la vue) jusqu'au bouchon.
    let cam = camera.compute_transform();
    let tip = cam.translation + cam.forward() * 1.2 + cam.right() * 0.45 + cam.up() * 0.35;
    let end = float + Vec3::Y * 0.2;
    let length = tip.distance(end);
    *line.0 = Transform::from_translation((tip + end) * 0.5)
        .looking_at(end, Vec3::Y)
        .with_scale(Vec3::new(0.012, 0.012, length));
    *line.1 = Visibility::Visible;
}
