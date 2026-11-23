//! Affichage du terrain : chargement progressif des chunks autour du joueur et
//! remaillage des chunks dont un bloc a changé.

use std::collections::{HashMap, HashSet};

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, futures::check_ready};
use shared::block::Block;
use shared::chunk::{CHUNK_SIZE, ChunkPos};
use shared::world::WorldState;

use crate::AppState;
use crate::meshing::{ChunkMeshes, build_chunk_mesh};
use crate::net::{ClientWorld, NetEvent};
use crate::settings::Settings;
use crate::wind::{Wind, WindMaterial};

/// Distance d'affichage, en chunks.
/// Nombre maximal de chunks en cours de génération en même temps.
const MAX_TASKS: usize = 12;

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Terrain>()
            .add_systems(Startup, setup_material)
            .add_systems(
                Update,
                (mark_dirty, request_chunks, finish_chunks)
                    .chain()
                    .run_if(in_state(AppState::Playing))
                    .run_if(resource_exists::<ClientWorld>),
            )
            .add_systems(OnExit(AppState::Playing), clear_terrain);
    }
}

#[derive(Resource, Default)]
struct Terrain {
    /// Chunks affichés, avec leurs entités (une par type de bloc ; aucune si le chunk est vide).
    loaded: HashMap<ChunkPos, Vec<Entity>>,
    /// Chunks en cours de calcul.
    pending: HashSet<ChunkPos>,
    /// Chunks à recalculer dès que possible (un bloc a changé).
    dirty: HashSet<ChunkPos>,
}

/// Un matériau par type de bloc, chacun avec sa texture répétée.
#[derive(Resource)]
struct TerrainMaterials(HashMap<Block, TerrainMaterial>);

/// Matériau d'un type de bloc : standard, ou animé par le vent.
enum TerrainMaterial {
    Plain(Handle<StandardMaterial>),
    Wind(Handle<WindMaterial>),
}

#[derive(Component)]
struct ChunkTask {
    pos: ChunkPos,
    task: Task<ChunkMeshes>,
}

#[derive(Component)]
struct ChunkMesh;

fn setup_material(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut wind_materials: ResMut<Assets<WindMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let handles = crate::textures::all_blocks()
        .map(|block| {
            let texture = images.add(crate::textures::build_block_texture(block));
            let material = if block.is_water() {
                // Eau translucide, visible des deux côtés (depuis la berge ou en nageant), ridée.
                let base = StandardMaterial {
                    base_color: Color::srgba(0.8, 0.9, 1.0, 0.8),
                    base_color_texture: Some(texture),
                    alpha_mode: AlphaMode::Blend,
                    perceptual_roughness: 0.6,
                    reflectance: 0.15,
                    cull_mode: None,
                    double_sided: true,
                    ..default()
                };
                TerrainMaterial::Wind(wind_materials.add(WindMaterial {
                    base,
                    extension: Wind::water(),
                }))
            } else if block == Block::Leaves || block.is_plant() {
                // Feuillage troué et plantes découpées, vus des deux côtés, qui bougent au vent.
                let base = StandardMaterial {
                    base_color: Color::WHITE,
                    base_color_texture: Some(texture),
                    alpha_mode: AlphaMode::Mask(0.5),
                    perceptual_roughness: 1.0,
                    cull_mode: None,
                    double_sided: true,
                    ..default()
                };
                let extension = if block == Block::Leaves {
                    Wind::foliage()
                } else {
                    Wind::plants()
                };
                TerrainMaterial::Wind(wind_materials.add(WindMaterial { base, extension }))
            } else {
                TerrainMaterial::Plain(materials.add(StandardMaterial {
                    base_color: Color::WHITE,
                    base_color_texture: Some(texture),
                    perceptual_roughness: 1.0,
                    ..default()
                }))
            };
            (block, material)
        })
        .collect();
    commands.insert_resource(TerrainMaterials(handles));
}

fn mark_dirty(mut events: MessageReader<NetEvent>, mut terrain: ResMut<Terrain>) {
    for event in events.read() {
        if let NetEvent::BlockChanged(pos) = event {
            for chunk in WorldState::chunks_touched_by(*pos) {
                if terrain.loaded.contains_key(&chunk) || terrain.pending.contains(&chunk) {
                    terrain.dirty.insert(chunk);
                }
            }
        }
    }
}

fn spawn_task(commands: &mut Commands, world: &WorldState, pos: ChunkPos) {
    let generator = world.generator().clone();
    let edits = world.edits_around_chunk(pos);
    let task = AsyncComputeTaskPool::get().spawn(async move {
        let mut chunk = generator.generate_chunk(pos);
        let origin = IVec3::from(pos.origin());
        for (p, block) in &edits {
            let local = *p - origin;
            if local.cmpge(IVec3::ZERO).all() && local.cmplt(IVec3::splat(CHUNK_SIZE as i32)).all() {
                chunk.set(local.x as usize, local.y as usize, local.z as usize, *block);
            }
        }
        build_chunk_mesh(&chunk, |x, y, z| {
            let p = origin + IVec3::new(x, y, z);
            match edits.get(&p) {
                Some(b) => *b,
                None => generator.block_at(p.x, p.y, p.z),
            }
        })
    });
    commands.spawn(ChunkTask { pos, task });
}

/// Lance le calcul des chunks proches de la caméra (les plus proches d'abord) et des chunks modifiés.
fn request_chunks(
    mut commands: Commands,
    mut terrain: ResMut<Terrain>,
    world: Res<ClientWorld>,
    camera: Single<&Transform, With<Camera3d>>,
    settings: Res<Settings>,
) {
    let view_radius = settings.view_distance;
    let mut budget = MAX_TASKS.saturating_sub(terrain.pending.len());

    // Les chunks modifiés passent en premier : c'est ce que le joueur regarde.
    let dirty: Vec<ChunkPos> = terrain.dirty.iter().copied().collect();
    for pos in dirty {
        if budget == 0 {
            return;
        }
        if terrain.pending.contains(&pos) {
            continue; // attendra la fin du calcul en cours
        }
        terrain.dirty.remove(&pos);
        terrain.pending.insert(pos);
        spawn_task(&mut commands, &world, pos);
        budget -= 1;
    }

    let p = camera.translation;
    let center = ChunkPos::from_block(p.x as i32, 0, p.z as i32);
    let config = world.config().clone();
    let mut wanted: Vec<ChunkPos> = Vec::new();
    for dx in -view_radius..=view_radius {
        for dz in -view_radius..=view_radius {
            if dx * dx + dz * dz > view_radius * view_radius {
                continue;
            }
            for y in 0..config.height_chunks {
                let pos = ChunkPos::new(center.x + dx, y, center.z + dz);
                if config.contains_chunk(pos) && !terrain.loaded.contains_key(&pos) && !terrain.pending.contains(&pos) {
                    wanted.push(pos);
                }
            }
        }
    }
    wanted.sort_by_key(|c| (c.x - center.x).pow(2) + (c.z - center.z).pow(2));
    for pos in wanted.into_iter().take(budget) {
        terrain.pending.insert(pos);
        spawn_task(&mut commands, &world, pos);
    }

    // Décharge les chunks trop loin.
    let far = view_radius + 2;
    let to_unload: Vec<ChunkPos> = terrain
        .loaded
        .keys()
        .filter(|c| (c.x - center.x).abs() > far || (c.z - center.z).abs() > far)
        .copied()
        .collect();
    for pos in to_unload {
        for entity in terrain.loaded.remove(&pos).unwrap_or_default() {
            commands.entity(entity).despawn();
        }
    }
}

/// Récupère les maillages terminés et remplace l'ancien maillage du chunk.
fn finish_chunks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut ChunkTask)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut terrain: ResMut<Terrain>,
    materials: Res<TerrainMaterials>,
) {
    for (task_entity, mut task) in &mut tasks {
        let Some(data) = check_ready(&mut task.task) else {
            continue;
        };
        let pos = task.pos;
        commands.entity(task_entity).despawn();
        terrain.pending.remove(&pos);
        for old in terrain.loaded.remove(&pos).unwrap_or_default() {
            commands.entity(old).despawn();
        }
        let [ox, oy, oz] = pos.origin();
        let mut entities = Vec::new();
        for (block, data) in data.parts {
            let Some(material) = materials.0.get(&block) else {
                continue;
            };
            let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, data.positions)
                .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, data.normals)
                .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, data.uvs)
                .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, data.colors)
                .with_inserted_indices(Indices::U32(data.indices));
            let mut entity = commands.spawn((
                ChunkMesh,
                Mesh3d(meshes.add(mesh)),
                Transform::from_xyz(ox as f32, oy as f32, oz as f32),
            ));
            match material {
                TerrainMaterial::Plain(handle) => entity.insert(MeshMaterial3d(handle.clone())),
                TerrainMaterial::Wind(handle) => entity.insert(MeshMaterial3d(handle.clone())),
            };
            entities.push(entity.id());
        }
        terrain.loaded.insert(pos, entities);
    }
}

fn clear_terrain(
    mut commands: Commands,
    mut terrain: ResMut<Terrain>,
    meshes: Query<Entity, Or<(With<ChunkMesh>, With<ChunkTask>)>>,
) {
    for entity in &meshes {
        commands.entity(entity).despawn();
    }
    *terrain = Terrain::default();
}
