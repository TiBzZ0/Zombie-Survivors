//! Chargement progressif des chunks autour du joueur.
//!
//! Pour l'instant le client génère lui-même le terrain depuis la graine (même code
//! que le serveur). Quand le réseau arrivera, il recevra en plus les chunks modifiés.

use std::collections::HashSet;
use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, futures::check_ready};
use shared::chunk::ChunkPos;
use shared::config::WorldConfig;
use shared::worldgen::WorldGenerator;

use crate::meshing::{MeshData, build_chunk_mesh};

/// Distance d'affichage, en chunks.
const VIEW_RADIUS: i32 = 8;
/// Nombre maximal de chunks en cours de génération en même temps.
const MAX_TASKS: usize = 16;

pub struct TerrainPlugin {
    pub world: WorldConfig,
}

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Terrain {
            generator: Arc::new(WorldGenerator::new(self.world.clone())),
            requested: HashSet::new(),
        })
        .add_systems(Startup, setup_material)
        .add_systems(Update, (request_chunks, finish_chunks));
    }
}

#[derive(Resource)]
struct Terrain {
    generator: Arc<WorldGenerator>,
    requested: HashSet<ChunkPos>,
}

#[derive(Resource)]
struct TerrainMaterial(Handle<StandardMaterial>);

/// Chunk en cours de génération sur un thread de calcul.
#[derive(Component)]
struct ChunkTask(Task<MeshData>);

fn setup_material(mut commands: Commands, mut materials: ResMut<Assets<StandardMaterial>>) {
    let handle = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 1.0,
        ..default()
    });
    commands.insert_resource(TerrainMaterial(handle));
}

/// Lance la génération des chunks proches de la caméra, les plus proches d'abord.
fn request_chunks(
    mut commands: Commands,
    mut terrain: ResMut<Terrain>,
    camera: Single<&Transform, With<Camera3d>>,
    tasks: Query<(), With<ChunkTask>>,
) {
    let mut budget = MAX_TASKS.saturating_sub(tasks.iter().count());
    if budget == 0 {
        return;
    }
    let p = camera.translation;
    let center = ChunkPos::from_block(p.x as i32, 0, p.z as i32);
    let config = terrain.generator.config().clone();

    let mut wanted: Vec<ChunkPos> = Vec::new();
    for dx in -VIEW_RADIUS..=VIEW_RADIUS {
        for dz in -VIEW_RADIUS..=VIEW_RADIUS {
            if dx * dx + dz * dz > VIEW_RADIUS * VIEW_RADIUS {
                continue;
            }
            for y in 0..config.height_chunks {
                let pos = ChunkPos::new(center.x + dx, y, center.z + dz);
                if config.contains_chunk(pos) && !terrain.requested.contains(&pos) {
                    wanted.push(pos);
                }
            }
        }
    }
    wanted.sort_by_key(|c| (c.x - center.x).pow(2) + (c.z - center.z).pow(2));

    let pool = AsyncComputeTaskPool::get();
    for pos in wanted {
        if budget == 0 {
            break;
        }
        budget -= 1;
        terrain.requested.insert(pos);
        let generator = terrain.generator.clone();
        let task = pool.spawn(async move {
            let chunk = generator.generate_chunk(pos);
            let [ox, oy, oz] = pos.origin();
            build_chunk_mesh(&chunk, |x, y, z| {
                generator.block_at(ox + x, oy + y, oz + z).is_solid()
            })
        });
        let [ox, oy, oz] = pos.origin();
        commands.spawn((
            ChunkTask(task),
            Transform::from_xyz(ox as f32, oy as f32, oz as f32),
            Name::new(format!("Chunk {} {} {}", pos.x, pos.y, pos.z)),
        ));
    }
}

/// Récupère les maillages terminés et les affiche.
fn finish_chunks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut ChunkTask)>,
    mut meshes: ResMut<Assets<Mesh>>,
    material: Res<TerrainMaterial>,
) {
    for (entity, mut task) in &mut tasks {
        let Some(data) = check_ready(&mut task.0) else {
            continue;
        };
        let mut entity = commands.entity(entity);
        entity.remove::<ChunkTask>();
        if data.is_empty() {
            entity.despawn();
            continue;
        }
        let vertex_count = data.positions.len();
        let mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, data.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, data.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32, 0.0]; vertex_count])
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, data.colors)
        .with_inserted_indices(Indices::U32(data.indices));
        entity.insert((Mesh3d(meshes.add(mesh)), MeshMaterial3d(material.0.clone())));
    }
}

/// Position de départ : au centre de la carte, un peu au-dessus du sol.
pub fn spawn_point(world: &WorldConfig) -> Vec3 {
    let generator = WorldGenerator::new(world.clone());
    let c = world.size_blocks() / 2;
    let ground = generator.surface_height(c, c);
    Vec3::new(c as f32 + 0.5, ground as f32 + 12.0, c as f32 + 0.5)
}
