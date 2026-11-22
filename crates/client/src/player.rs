//! Le joueur local : vue à la première personne, déplacements avec collisions,
//! casser et poser des blocs.

use std::f32::consts::FRAC_PI_2;

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions};
use bevy_renet::RenetClient;
use shared::block::Block;
use shared::physics::{PLAYER_EYE_HEIGHT, PLAYER_SIZE, overlaps_solid, step_body};
use shared::protocol::{ClientMessage, PlayerState, REACH};
use shared::raycast::raycast;

use crate::AppState;
use crate::net::{ClientWorld, NetEvent, Session, send_reliable, send_state};

const WALK_SPEED: f32 = 4.5;
const SPRINT_SPEED: f32 = 7.0;
const JUMP_SPEED: f32 = 8.5;
const MOUSE_SENSITIVITY: f32 = 0.0025;
/// Délai entre deux actions sur les blocs quand le bouton reste enfoncé.
const ACTION_COOLDOWN: f32 = 0.22;

/// Blocs posables, dans l'ordre des touches 1 à 7.
pub const HOTBAR: [Block; 7] = [
    Block::Dirt,
    Block::Stone,
    Block::Wood,
    Block::Sand,
    Block::Grass,
    Block::Leaves,
    Block::IronOre,
];

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LocalPlayer>()
            .add_systems(OnEnter(AppState::Playing), spawn_player)
            .add_systems(
                Update,
                (
                    grab_cursor,
                    look,
                    select_block,
                    apply_teleports,
                    move_player,
                    interact,
                    update_camera,
                    send_position,
                )
                    .chain()
                    .run_if(in_state(AppState::Playing))
                    .run_if(resource_exists::<ClientWorld>),
            );
    }
}

/// État du joueur local.
#[derive(Resource, Debug, Default)]
pub struct LocalPlayer {
    pub position: Vec3,
    pub velocity: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
    pub selected: usize,
    /// Bloc visé, s'il y en a un à portée.
    pub target: Option<shared::raycast::RayHit>,
    cooldown: f32,
    last_sent: f64,
}

impl LocalPlayer {
    pub fn eye(&self) -> Vec3 {
        self.position + Vec3::Y * PLAYER_EYE_HEIGHT
    }

    pub fn look_direction(&self) -> Vec3 {
        Quat::from_euler(EulerRot::YXZ, self.yaw, self.pitch, 0.0) * Vec3::NEG_Z
    }

    pub fn selected_block(&self) -> Block {
        HOTBAR[self.selected]
    }
}

fn spawn_player(mut player: ResMut<LocalPlayer>, session: Res<Session>) {
    *player = LocalPlayer {
        position: session.spawn,
        yaw: -std::f32::consts::FRAC_PI_4,
        ..default()
    };
}

fn cursor_locked(cursor: &CursorOptions) -> bool {
    cursor.grab_mode != CursorGrabMode::None
}

fn grab_cursor(
    mut cursor: Single<&mut CursorOptions>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
) {
    if mouse.just_pressed(MouseButton::Left) && !cursor_locked(&cursor) {
        cursor.visible = false;
        cursor.grab_mode = CursorGrabMode::Locked;
    }
    if keys.just_pressed(KeyCode::Escape) {
        cursor.visible = true;
        cursor.grab_mode = CursorGrabMode::None;
    }
}

fn look(
    motion: Res<AccumulatedMouseMotion>,
    cursor: Single<&CursorOptions>,
    mut player: ResMut<LocalPlayer>,
) {
    if !cursor_locked(&cursor) {
        return;
    }
    player.yaw -= motion.delta.x * MOUSE_SENSITIVITY;
    player.pitch = (player.pitch - motion.delta.y * MOUSE_SENSITIVITY)
        .clamp(-FRAC_PI_2 + 0.01, FRAC_PI_2 - 0.01);
}

fn select_block(keys: Res<ButtonInput<KeyCode>>, mut player: ResMut<LocalPlayer>) {
    const KEYS: [KeyCode; 7] = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
    ];
    for (i, key) in KEYS.iter().enumerate() {
        if keys.just_pressed(*key) {
            player.selected = i;
        }
    }
}

fn apply_teleports(mut events: MessageReader<NetEvent>, mut player: ResMut<LocalPlayer>) {
    for event in events.read() {
        if let NetEvent::Teleport(position) = event {
            player.position = *position;
            player.velocity = Vec3::ZERO;
        }
    }
}

fn move_player(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    world: Res<ClientWorld>,
    mut player: ResMut<LocalPlayer>,
) {
    // Les touches sont physiques : KeyW est la touche Z sur un clavier AZERTY.
    let mut wish = Vec3::ZERO;
    if keys.pressed(KeyCode::KeyW) {
        wish.z -= 1.0;
    }
    if keys.pressed(KeyCode::KeyS) {
        wish.z += 1.0;
    }
    if keys.pressed(KeyCode::KeyA) {
        wish.x -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) {
        wish.x += 1.0;
    }
    let speed = if keys.pressed(KeyCode::ShiftLeft) {
        SPRINT_SPEED
    } else {
        WALK_SPEED
    };
    let wish = Quat::from_rotation_y(player.yaw) * wish.normalize_or_zero() * speed;

    let dt = time.delta_secs().min(0.05);
    let control = if player.on_ground { 1.0 } else { 0.25 };
    let horizontal = Vec3::new(player.velocity.x, 0.0, player.velocity.z);
    let target = Vec3::new(wish.x, 0.0, wish.z);
    let new_horizontal = horizontal.lerp(target, (control * 15.0 * dt).min(1.0));
    let mut velocity = Vec3::new(new_horizontal.x, player.velocity.y, new_horizontal.z);
    if player.on_ground && keys.pressed(KeyCode::Space) {
        velocity.y = JUMP_SPEED;
    }

    // Tant que le terrain sous le joueur n'est pas connu, il ne tombe pas dans le vide.
    let solid = |p: IVec3| world.is_solid(p) || !world.in_bounds(p) && p.y < 0;
    let result = step_body(player.position, velocity, PLAYER_SIZE, dt, &solid);
    player.position = result.position;
    player.velocity = result.velocity;
    player.on_ground = result.on_ground;

    // Tombé sous la carte : retour en surface.
    if player.position.y < -20.0 {
        let p = player.position.as_ivec3();
        let ground = world.generator().surface_height(p.x, p.z);
        player.position.y = ground as f32 + 2.0;
        player.velocity = Vec3::ZERO;
    }
}

fn interact(
    mouse: Res<ButtonInput<MouseButton>>,
    cursor: Single<&CursorOptions>,
    time: Res<Time>,
    mut world: ResMut<ClientWorld>,
    mut player: ResMut<LocalPlayer>,
    mut client: ResMut<RenetClient>,
    mut changes: MessageWriter<NetEvent>,
) {
    player.target = raycast(player.eye(), player.look_direction(), REACH, |p| {
        world.is_solid(p)
    });
    player.cooldown -= time.delta_secs();
    if !cursor_locked(&cursor) || player.cooldown > 0.0 {
        return;
    }
    let Some(hit) = player.target else { return };

    if mouse.pressed(MouseButton::Left) {
        if hit.block.y > 0 {
            // Appliqué tout de suite pour la réactivité ; le serveur corrige s'il refuse.
            world.set_block(hit.block, Block::Air);
            changes.write(NetEvent::BlockChanged(hit.block));
            send_reliable(&mut client, &ClientMessage::BreakBlock { pos: hit.block });
            player.cooldown = ACTION_COOLDOWN;
        }
    } else if mouse.pressed(MouseButton::Right) {
        let pos = hit.block + hit.normal;
        let blocks_me = overlaps_solid(player.position, PLAYER_SIZE, &|b| b == pos);
        if !blocks_me && world.in_bounds(pos) && !world.is_solid(pos) {
            let block = player.selected_block();
            world.set_block(pos, block);
            changes.write(NetEvent::BlockChanged(pos));
            send_reliable(&mut client, &ClientMessage::PlaceBlock { pos, block });
            player.cooldown = ACTION_COOLDOWN;
        }
    }
}

fn update_camera(player: Res<LocalPlayer>, mut camera: Single<&mut Transform, With<Camera3d>>) {
    camera.translation = player.eye();
    camera.rotation = Quat::from_euler(EulerRot::YXZ, player.yaw, player.pitch, 0.0);
}

fn send_position(
    mut player: ResMut<LocalPlayer>,
    mut client: ResMut<RenetClient>,
    time: Res<Time>,
) {
    let state = PlayerState {
        position: player.position,
        yaw: player.yaw,
        pitch: player.pitch,
    };
    let mut last = player.last_sent;
    send_state(&mut client, state, &time, &mut last);
    player.last_sent = last;
}
