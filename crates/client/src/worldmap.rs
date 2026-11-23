//! Carte du monde (touche M) : tout le monde vu de dessus, mais seulement les régions déjà
//! explorées ; le reste reste dans le brouillard. Un clic pose un repère, visible aussi
//! sur la mini-carte avec sa distance ; un clic droit l'efface.

use std::collections::HashSet;

use bevy::asset::RenderAssetUsages;
use bevy::image::{Image, ImageSampler};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::ui::UiGlobalTransform;
use bevy::window::PrimaryWindow;

use crate::AppState;
use crate::minimap::{Columns, SCALE, request_columns};
use crate::net::ClientWorld;
use crate::player::LocalPlayer;
use crate::remote::RemotePlayers;
use crate::survivors::SurvivorView;

/// Côté d'une case de brouillard, en blocs.
const CELL: i32 = 8;
/// Rayon dévoilé autour du joueur, en blocs.
const REVEAL: f32 = 44.0;
/// Côté maximal de la carte à l'écran.
const DISPLAY: f32 = 560.0;
/// Intervalle entre deux mises à jour de l'image ouverte.
const REFRESH: f32 = 0.3;
/// Colonnes demandées à chaque tâche, pour ne pas bloquer un cœur trop longtemps.
const BATCH: usize = 4096;

pub struct WorldMapPlugin;

impl Plugin for WorldMapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Explored>()
            .init_resource::<Waypoint>()
            .add_systems(OnEnter(AppState::Playing), spawn_map)
            .add_systems(OnExit(AppState::Playing), despawn_map)
            .add_systems(
                Update,
                (explore, show_map, place_waypoint, draw_map)
                    .chain()
                    .run_if(in_state(AppState::Playing))
                    .run_if(resource_exists::<ClientWorld>),
            );
    }
}

/// Cases de brouillard déjà visitées.
#[derive(Resource, Default)]
struct Explored(HashSet<(i32, i32)>);

/// Repère posé sur la carte (x, z).
#[derive(Resource, Default)]
pub struct Waypoint(pub Option<Vec2>);

#[derive(Component)]
struct MapRoot;

#[derive(Component)]
struct MapImageNode;

#[derive(Resource)]
struct MapImage {
    handle: Handle<Image>,
    /// Côté de l'image en pixels (un pixel = SCALE blocs).
    side: u32,
}

fn spawn_map(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    world: Res<ClientWorld>,
    mut explored: ResMut<Explored>,
    mut waypoint: ResMut<Waypoint>,
) {
    *explored = Explored::default();
    *waypoint = Waypoint::default();
    let side = (world.generator().config().size_blocks() / SCALE).max(1) as u32;
    let mut image = Image::new(
        Extent3d {
            width: side,
            height: side,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        vec![0; (side * side * 4) as usize],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::nearest();
    let handle = images.add(image);
    commands.insert_resource(MapImage {
        handle: handle.clone(),
        side,
    });
    let label = |text: &str, size: f32, color: Color| {
        (
            Text::new(text),
            TextFont {
                font_size: FontSize::Px(size),
                ..default()
            },
            TextColor(color),
            TextShadow::default(),
        )
    };
    commands
        .spawn((
            MapRoot,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: px(8),
                padding: UiRect::bottom(px(70)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.65)),
            GlobalZIndex(20),
            Visibility::Hidden,
        ))
        .with_children(|root| {
            root.spawn(label("CARTE DU MONDE", 24.0, Color::srgb(1.0, 0.9, 0.5)));
            root.spawn((
                MapImageNode,
                Node {
                    width: px(DISPLAY),
                    height: px(DISPLAY),
                    max_width: vh(78),
                    max_height: vh(78),
                    border: px(3).all(),
                    ..default()
                },
                BorderColor::all(Color::srgb(0.45, 0.35, 0.2)),
                ImageNode::new(handle),
            ));
            root.spawn(label(
                "Clic gauche : poser un repère · Clic droit : l'effacer · M ou Échap : fermer",
                15.0,
                Color::srgba(1.0, 1.0, 1.0, 0.85),
            ));
            root.spawn(label(
                "Blanc : toi · Bleu : autres joueurs · Jaune : survivants · Rose : repère",
                13.0,
                Color::srgba(1.0, 1.0, 1.0, 0.6),
            ));
        });
}

fn despawn_map(mut commands: Commands, roots: Query<Entity, With<MapRoot>>) {
    for entity in &roots {
        commands.entity(entity).despawn();
    }
}

/// Dévoile les cases autour du joueur.
fn explore(time: Res<Time>, player: Res<LocalPlayer>, mut explored: ResMut<Explored>, mut since: Local<f32>) {
    *since += time.delta_secs();
    if *since < 0.5 {
        return;
    }
    *since = 0.0;
    let p = Vec2::new(player.position.x, player.position.z);
    let r = (REVEAL / CELL as f32).ceil() as i32;
    let (cx, cz) = ((p.x / CELL as f32).floor() as i32, (p.y / CELL as f32).floor() as i32);
    for dz in -r..=r {
        for dx in -r..=r {
            let cell = (cx + dx, cz + dz);
            let center = Vec2::new(cell.0 as f32 + 0.5, cell.1 as f32 + 0.5) * CELL as f32;
            if center.distance(p) <= REVEAL && !explored.0.contains(&cell) {
                explored.0.insert(cell);
            }
        }
    }
}

fn show_map(player: Res<LocalPlayer>, mut root: Single<&mut Visibility, With<MapRoot>>) {
    let wanted = if player.map_open && player.dead.is_none() {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if **root != wanted {
        **root = wanted;
    }
}

fn place_waypoint(
    mouse: Res<ButtonInput<MouseButton>>,
    player: Res<LocalPlayer>,
    world: Res<ClientWorld>,
    node: Single<(&ComputedNode, &UiGlobalTransform), With<MapImageNode>>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut waypoint: ResMut<Waypoint>,
) {
    if !player.map_open {
        return;
    }
    if mouse.just_pressed(MouseButton::Right) {
        waypoint.0 = None;
    }
    let (computed, transform) = *node;
    if mouse.just_pressed(MouseButton::Left)
        && let Some(cursor) = window.physical_cursor_position()
    {
        // Position du curseur dans l'image, de 0 à 1 (coin haut gauche à coin bas droit).
        let at = (cursor - transform.translation) / computed.size() + Vec2::splat(0.5);
        if (0.0..=1.0).contains(&at.x) && (0.0..=1.0).contains(&at.y) {
            let size = world.generator().config().size_blocks() as f32;
            waypoint.0 = Some(at * size);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_map(
    mut commands: Commands,
    time: Res<Time>,
    world: Res<ClientWorld>,
    player: Res<LocalPlayer>,
    explored: Res<Explored>,
    waypoint: Res<Waypoint>,
    mut columns: ResMut<Columns>,
    image: Res<MapImage>,
    mut images: ResMut<Assets<Image>>,
    survivors: Query<&Transform, With<SurvivorView>>,
    remote: Res<RemotePlayers>,
    transforms: Query<&Transform>,
    mut since: Local<f32>,
) {
    *since += time.delta_secs();
    if !player.map_open || *since < REFRESH {
        return;
    }
    *since = 0.0;
    let side = image.side as i32;
    // Colonnes explorées encore inconnues : calculées en tâche de fond, par paquets.
    let mut missing = Vec::new();
    for &(cx, cz) in &explored.0 {
        for z in (cz * CELL..(cz + 1) * CELL).step_by(SCALE as usize) {
            for x in (cx * CELL..(cx + 1) * CELL).step_by(SCALE as usize) {
                let key = (x, z);
                if (0..side * SCALE).contains(&x)
                    && (0..side * SCALE).contains(&z)
                    && !columns.colors.contains_key(&key)
                    && !columns.pending.contains(&key)
                {
                    missing.push(key);
                }
            }
        }
    }
    for chunk in missing.chunks(BATCH) {
        request_columns(&mut commands, &world, &mut columns, chunk.to_vec());
    }

    let Some(mut img) = images.get_mut(&image.handle) else {
        return;
    };
    let Some(data) = img.data.as_mut() else {
        return;
    };
    let put = |data: &mut [u8], x: i32, y: i32, color: [u8; 3]| {
        if (0..side).contains(&x) && (0..side).contains(&y) {
            let i = ((y * side + x) * 4) as usize;
            data[i..i + 4].copy_from_slice(&[color[0], color[1], color[2], 255]);
        }
    };
    for py in 0..side {
        for px in 0..side {
            let (x, z) = (px * SCALE, py * SCALE);
            let seen = explored.0.contains(&(x.div_euclid(CELL), z.div_euclid(CELL)));
            let color = if seen {
                columns.colors.get(&(x, z)).copied().unwrap_or([40, 40, 45])
            } else {
                // Brouillard : un quadrillage discret de vieux papier.
                let grid = x % 64 == 0 || z % 64 == 0;
                if grid { [52, 46, 38] } else { [34, 30, 26] }
            };
            put(data, px, py, color);
        }
    }
    let to_pixel = |p: Vec2| ((p.x / SCALE as f32).floor() as i32, (p.y / SCALE as f32).floor() as i32);
    let mark = |data: &mut [u8], p: Vec2, color: [u8; 3], radius: i32| {
        let (x, y) = to_pixel(p);
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx.abs() + dy.abs() <= radius {
                    put(data, x + dx, y + dy, color);
                }
            }
        }
    };
    let flat = |v: Vec3| Vec2::new(v.x, v.z);
    for t in &survivors {
        mark(data, flat(t.translation), [250, 220, 60], 1);
    }
    for entity in remote.entities.values() {
        if let Ok(t) = transforms.get(*entity) {
            mark(data, flat(t.translation), [80, 160, 255], 2);
        }
    }
    if let Some(w) = waypoint.0 {
        let (x, y) = to_pixel(w);
        for d in -3..=3 {
            put(data, x + d, y + d, [255, 80, 200]);
            put(data, x + d, y - d, [255, 80, 200]);
        }
    }
    let look = Quat::from_rotation_y(player.yaw) * Vec3::NEG_Z;
    for step in 1..=6 {
        let p = player.position + look * (step as f32 * SCALE as f32);
        mark(data, flat(p), [255, 255, 255], 0);
    }
    mark(data, flat(player.position), [255, 255, 255], 2);
}
