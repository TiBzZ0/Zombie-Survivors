//! Affichage du terrain : chargement progressif des chunks autour du joueur et
//! remaillage des chunks dont un bloc a changé.

use std::collections::{HashMap, HashSet};

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, futures::check_ready};
use shared::chunk::{CHUNK_SIZE, ChunkPos};
use shared::world::WorldState;

use crate::AppState;
use crate::meshing::{MeshData, build_chunk_mesh};
use crate::net::{ClientWorld, NetEvent};

/// Distance d'affichage, en chunks.
const VIEW_RADIUS: i32 = 7;
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
    /// Chunks affichés (ou vides), avec leur entité s'ils ont un maillage.
    loaded: HashMap<ChunkPos, Option<Entity>>,
    /// Chunks en cours de calcul.
    pending: HashSet<ChunkPos>,
    /// Chunks à recalculer dès que possible (un bloc a changé).
    dirty: HashSet<ChunkPos>,
}

#[derive(Resource)]
struct TerrainMaterial(Handle<StandardMaterial>);

#[derive(Component)]
struct ChunkTask {
    pos: ChunkPos,
    task: Task<MeshData>,
}

#[derive(Component)]
struct ChunkMesh;

fn setup_material(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let atlas = images.add(crate::textures::build_atlas());
    let handle = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(atlas),
        perceptual_roughness: 1.0,
        ..default()
    });
    commands.insert_resource(TerrainMaterial(handle));
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
                Some(b) => b.is_opaque(),
                None => generator.block_at(p.x, p.y, p.z).is_opaque(),
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
) {
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
    for dx in -VIEW_RADIUS..=VIEW_RADIUS {
        for dz in -VIEW_RADIUS..=VIEW_RADIUS {
            if dx * dx + dz * dz > VIEW_RADIUS * VIEW_RADIUS {
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
    let far = VIEW_RADIUS + 2;
    let to_unload: Vec<ChunkPos> = terrain
        .loaded
        .keys()
        .filter(|c| (c.x - center.x).abs() > far || (c.z - center.z).abs() > far)
        .copied()
        .collect();
    for pos in to_unload {
        if let Some(Some(entity)) = terrain.loaded.remove(&pos) {
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
    material: Res<TerrainMaterial>,
) {
    for (task_entity, mut task) in &mut tasks {
        let Some(data) = check_ready(&mut task.task) else {
            continue;
        };
        let pos = task.pos;
        commands.entity(task_entity).despawn();
        terrain.pending.remove(&pos);
        if let Some(Some(old)) = terrain.loaded.remove(&pos) {
            commands.entity(old).despawn();
        }
        if data.is_empty() {
            terrain.loaded.insert(pos, None);
            continue;
        }
        let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, data.positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, data.normals)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, data.uvs)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, data.colors)
            .with_inserted_indices(Indices::U32(data.indices));
        let [ox, oy, oz] = pos.origin();
        let entity = commands
            .spawn((
                ChunkMesh,
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(material.0.clone()),
                Transform::from_xyz(ox as f32, oy as f32, oz as f32),
            ))
            .id();
        terrain.loaded.insert(pos, Some(entity));
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
