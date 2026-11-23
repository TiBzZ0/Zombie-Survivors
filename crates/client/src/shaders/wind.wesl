// Vent : le feuillage oscille, les plantes ondulent par le haut et la surface de l'eau
// se ride. Le déplacement ne dépend que de la position dans le monde, si bien que les
// sommets partagés par deux faces bougent ensemble (pas de fente entre les blocs).

import bevy_pbr::render::{
    mesh_functions,
    forward_io::{Vertex, VertexOutput, decompress_vertex},
    view_transformations::position_world_to_clip,
    mesh_view_bindings::globals,
};

// x : balancement de tous les sommets (feuilles), y : balancement du haut seulement
// (plantes), z : vaguelettes du dessus de l'eau.
@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> wind: vec4<f32>;

fn gust(p: vec3<f32>, t: f32) -> vec3<f32> {
    let a = sin(t * 1.7 + p.x * 0.35 + p.z * 0.21) + 0.4 * sin(t * 3.1 + p.x * 0.9 + p.y * 0.5);
    let b = cos(t * 1.3 + p.z * 0.33 + p.x * 0.17) + 0.4 * sin(t * 2.7 + p.z * 0.8 + p.y * 0.5);
    return vec3<f32>(a, 0.0, b);
}

@vertex
fn vertex(vertex_in: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let v = decompress_vertex(vertex_in, vertex_in.instance_index);
    let world_from_local = mesh_functions::get_world_from_local(vertex_in.instance_index);
    var world = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(v.position, 1.0));
    let t = globals.time;

    var weight = wind.x;
@if(VERTEX_UVS_A)
    weight += wind.y * (1.0 - clamp(v.uv.y, 0.0, 1.0));
    let offset = gust(world.xyz, t) * weight;
    world.x += offset.x;
    world.z += offset.z;
    // Le dessus de l'eau est abaissé de 1/8 de bloc : seuls ces sommets ondulent.
    let surface = select(0.0, 1.0, abs(fract(world.y) - 0.875) < 0.01);
    world.y += wind.z * surface * (sin(t * 1.4 + world.x * 0.7 + world.z * 0.5) - 1.0);

    out.world_position = world;
    out.position = position_world_to_clip(world.xyz);
@if(VERTEX_NORMALS)
    out.world_normal = mesh_functions::mesh_normal_local_to_world(v.normal, vertex_in.instance_index);
@if(VERTEX_UVS_A)
    out.uv = v.uv;
@if(VERTEX_COLORS)
    out.color = v.color;
@if(VERTEX_OUTPUT_INSTANCE_INDEX)
    out.instance_index = vertex_in.instance_index;
    return out;
}
