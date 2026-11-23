//! Carte en haut à droite : le terrain vu de dessus autour du joueur (nord en haut),
//! avec les zombies, les survivants, le gibier et les autres joueurs. Touche M pour la masquer.
//!
//! Les couleurs du terrain sont calculées en tâche de fond, colonne par colonne, et gardées
//! en cache ; seule une colonne modifiée (bloc posé ou cassé) est recalculée.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::image::{Image, ImageSampler};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::tasks::{AsyncComputeTaskPool, Task, futures::check_ready};
use shared::block::Block;
use shared::worldgen::WorldGenerator;

use crate::AppState;
use crate::animals::AnimalView;
use crate::net::{ClientWorld, NetEvent};
use crate::player::LocalPlayer;
use crate::remote::RemotePlayers;
use crate::settings::Settings;
use crate::survivors::SurvivorView;
use crate::zombies::ZombieView;

/// Côté de la carte, en pixels de l'image.
const SIZE: u32 = 96;
/// Blocs par pixel.
const SCALE: i32 = 2;
/// Taille affichée à l'écran.
const DISPLAY: f32 = 192.0;
/// Intervalle entre deux mises à jour de l'image, en secondes.
const REFRESH: f32 = 0.25;

pub struct MinimapPlugin;

impl Plugin for MinimapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Columns>()
            .add_systems(OnEnter(AppState::Playing), spawn_minimap)
            .add_systems(OnExit(AppState::Playing), despawn_minimap)
            .add_systems(
                Update,
                (
                    toggle_minimap,
                    forget_changed,
                    finish_columns,
                    draw_minimap,
                    update_coords,
                )
                    .chain()
                    .run_if(in_state(AppState::Playing))
                    .run_if(resource_exists::<ClientWorld>),
            );
    }
}

/// Couleurs des colonnes déjà calculées, par position (x, z) d'un coin de pixel.
#[derive(Resource, Default)]
struct Columns {
    colors: HashMap<(i32, i32), [u8; 3]>,
    /// Colonnes en cours de calcul.
    pending: HashSet<(i32, i32)>,
}

#[derive(Component)]
struct ColumnTask(Task<Vec<((i32, i32), [u8; 3])>>);

#[derive(Component)]
struct MinimapRoot;

#[derive(Resource)]
struct MinimapImage(Handle<Image>);

fn spawn_minimap(mut commands: Commands, mut images: ResMut<Assets<Image>>, mut columns: ResMut<Columns>) {
    // Nouvelle partie : peut-être un autre monde.
    *columns = Columns::default();
    let mut image = Image::new(
        Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        vec![0; (SIZE * SIZE * 4) as usize],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::nearest();
    let handle = images.add(image);
    commands.insert_resource(MinimapImage(handle.clone()));
    commands.spawn((
        MinimapRoot,
        Node {
            position_type: PositionType::Absolute,
            top: px(84),
            right: px(10),
            width: px(DISPLAY),
            height: px(DISPLAY),
            border: px(2).all(),
            ..default()
        },
        BorderColor::all(Color::srgba(0.0, 0.0, 0.0, 0.7)),
        ImageNode::new(handle),
        children![
            (
                Node {
                    position_type: PositionType::Absolute,
                    top: px(1),
                    width: percent(100),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                children![(
                    Text::new("N"),
                    TextFont {
                        font_size: FontSize::Px(14.0),
                        ..default()
                    },
                    TextColor(Color::srgb(1.0, 0.9, 0.5)),
                    TextShadow::default(),
                )],
            ),
            (
                Node {
                    position_type: PositionType::Absolute,
                    top: px(DISPLAY + 2.0),
                    width: percent(100),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                children![(
                    Text::new(""),
                    TextFont {
                        font_size: FontSize::Px(13.0),
                        ..default()
                    },
                    TextColor(Color::srgba(1.0, 1.0, 1.0, 0.85)),
                    TextShadow::default(),
                    MinimapCoords,
                )],
            ),
        ],
    ));
}

/// Position du joueur sous la carte.
#[derive(Component)]
struct MinimapCoords;

fn update_coords(player: Res<LocalPlayer>, mut text: Single<&mut Text, With<MinimapCoords>>) {
    let p = player.position.floor().as_ivec3();
    let label = format!("X {} · Z {} · alt. {}", p.x, p.z, p.y);
    if text.0 != label {
        text.0 = label;
    }
}

fn despawn_minimap(
    mut commands: Commands,
    roots: Query<Entity, With<MinimapRoot>>,
    tasks: Query<Entity, With<ColumnTask>>,
) {
    for entity in roots.iter().chain(tasks.iter()) {
        commands.entity(entity).despawn();
    }
}

fn toggle_minimap(
    keys: Res<ButtonInput<KeyCode>>,
    player: Res<LocalPlayer>,
    mut settings: ResMut<Settings>,
    mut root: Single<&mut Visibility, With<MinimapRoot>>,
) {
    if keys.just_pressed(KeyCode::KeyM) && !player.typing {
        settings.minimap = !settings.minimap;
        settings.save();
    }
    let wanted = if settings.minimap {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if **root != wanted {
        **root = wanted;
    }
}

/// Un bloc a changé : sa colonne sera recalculée.
fn forget_changed(mut events: MessageReader<NetEvent>, mut columns: ResMut<Columns>) {
    for event in events.read() {
        if let NetEvent::BlockChanged(pos) = event {
            let key = (pos.x.div_euclid(SCALE) * SCALE, pos.z.div_euclid(SCALE) * SCALE);
            columns.colors.remove(&key);
        }
    }
}

fn finish_columns(mut commands: Commands, mut columns: ResMut<Columns>, mut tasks: Query<(Entity, &mut ColumnTask)>) {
    for (entity, mut task) in &mut tasks {
        if let Some(done) = check_ready(&mut task.0) {
            for (key, color) in done {
                columns.pending.remove(&key);
                columns.colors.insert(key, color);
            }
            commands.entity(entity).despawn();
        }
    }
}

/// Couleur d'une colonne vue de dessus : le premier bloc visible, plus sombre s'il est bas.
fn column_color(
    generator: &WorldGenerator,
    edits: &HashMap<IVec3, Block>,
    top_edit: Option<i32>,
    x: i32,
    z: i32,
) -> [u8; 3] {
    let size = generator.config().size_blocks();
    if x < 0 || z < 0 || x >= size || z >= size {
        return [20, 20, 25];
    }
    let surface = generator.surface_height(x, z);
    let start = (surface + 8).max(top_edit.unwrap_or(0));
    for y in (0..=start).rev() {
        let block = edits
            .get(&IVec3::new(x, y, z))
            .copied()
            .unwrap_or_else(|| generator.block_at(x, y, z));
        if block == Block::Air {
            continue;
        }
        let [r, g, b] = block.color();
        // Relief : plus clair en hauteur, plus sombre en contrebas.
        let base = generator.config().height_blocks() as f32 * 0.45;
        let shade = (1.0 + (y as f32 - base) * 0.012).clamp(0.6, 1.3);
        let channel = |c: f32| ((c * shade).clamp(0.0, 1.0) * 255.0) as u8;
        return [channel(r), channel(g), channel(b)];
    }
    [20, 20, 25]
}

fn put(data: &mut [u8], x: i32, y: i32, color: [u8; 3]) {
    if (0..SIZE as i32).contains(&x) && (0..SIZE as i32).contains(&y) {
        let i = ((y as u32 * SIZE + x as u32) * 4) as usize;
        data[i..i + 4].copy_from_slice(&[color[0], color[1], color[2], 255]);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_minimap(
    mut commands: Commands,
    time: Res<Time>,
    settings: Res<Settings>,
    world: Res<ClientWorld>,
    player: Res<LocalPlayer>,
    mut columns: ResMut<Columns>,
    image: Res<MinimapImage>,
    mut images: ResMut<Assets<Image>>,
    zombies: Query<&Transform, With<ZombieView>>,
    survivors: Query<&Transform, With<SurvivorView>>,
    animals: Query<&Transform, With<AnimalView>>,
    remote: Res<RemotePlayers>,
    transforms: Query<&Transform>,
    mut since: Local<f32>,
) {
    *since += time.delta_secs();
    if !settings.minimap || *since < REFRESH {
        return;
    }
    *since = 0.0;
    let half = SIZE as i32 / 2;
    let center = player.position.as_ivec3();
    let (cx, cz) = (center.x.div_euclid(SCALE) * SCALE, center.z.div_euclid(SCALE) * SCALE);
    let key_at = |px: i32, py: i32| (cx + (px - half) * SCALE, cz + (py - half) * SCALE);

    // Colonnes manquantes : calculées en tâche de fond.
    let missing: Vec<(i32, i32)> = (0..SIZE as i32)
        .flat_map(|py| (0..SIZE as i32).map(move |px| (px, py)))
        .map(|(px, py)| key_at(px, py))
        .filter(|k| !columns.colors.contains_key(k) && !columns.pending.contains(k))
        .collect();
    if !missing.is_empty() {
        columns.pending.extend(missing.iter().copied());
        let generator: Arc<WorldGenerator> = world.generator().clone();
        let edits: HashMap<IVec3, Block> = world.edits().clone();
        let task = AsyncComputeTaskPool::get().spawn(async move {
            let mut tops: HashMap<(i32, i32), i32> = HashMap::new();
            for (p, b) in &edits {
                if *b != Block::Air {
                    let top = tops.entry((p.x, p.z)).or_insert(p.y);
                    *top = (*top).max(p.y);
                }
            }
            missing
                .into_iter()
                .map(|(x, z)| {
                    let color = column_color(&generator, &edits, tops.get(&(x, z)).copied(), x, z);
                    ((x, z), color)
                })
                .collect()
        });
        commands.spawn(ColumnTask(task));
    }

    let Some(mut image) = images.get_mut(&image.0) else {
        return;
    };
    let Some(data) = image.data.as_mut() else {
        return;
    };
    for py in 0..SIZE as i32 {
        for px in 0..SIZE as i32 {
            let color = columns.colors.get(&key_at(px, py)).copied().unwrap_or([10, 10, 12]);
            put(data, px, py, color);
        }
    }
    // Position monde -> pixel de la carte.
    let to_pixel = |p: Vec3| {
        (
            ((p.x - cx as f32) / SCALE as f32).floor() as i32 + half,
            ((p.z - cz as f32) / SCALE as f32).floor() as i32 + half,
        )
    };
    let dot = |data: &mut [u8], p: Vec3, color: [u8; 3], big: bool| {
        let (x, y) = to_pixel(p);
        put(data, x, y, color);
        if big {
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                put(data, x + dx, y + dy, color);
            }
        }
    };
    for t in &animals {
        dot(data, t.translation, [150, 100, 60], false);
    }
    for t in &survivors {
        dot(data, t.translation, [250, 220, 60], true);
    }
    for t in &zombies {
        dot(data, t.translation, [230, 40, 40], true);
    }
    for entity in remote.entities.values() {
        if let Ok(t) = transforms.get(*entity) {
            dot(data, t.translation, [80, 160, 255], true);
        }
    }
    // Le joueur, et la direction de son regard.
    let look = Quat::from_rotation_y(player.yaw) * Vec3::NEG_Z;
    for step in 1..=5 {
        dot(
            data,
            player.position + look * (step as f32 * SCALE as f32),
            [255, 255, 255],
            false,
        );
    }
    dot(data, player.position, [255, 255, 255], true);
}
