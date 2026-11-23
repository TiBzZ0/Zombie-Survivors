//! Bras du joueur en vue subjective, avec l'objet tenu : un bloc devient un petit cube texturé,
//! les autres objets leur icône découpée. Le bras se balance en marchant, frappe quand on mine
//! ou qu'on attaque, recule quand on tire, se lève pour parer et redescend pour changer d'objet.

use std::collections::HashMap;
use std::f32::consts::PI;

use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions};
use shared::block::Block;
use shared::item::Item;

use crate::AppState;
use crate::icons::ItemIcons;
use crate::player::LocalPlayer;

pub struct ViewModelPlugin;

impl Plugin for ViewModelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HeldMaterials>()
            .add_systems(OnEnter(AppState::Playing), spawn_view_model)
            .add_systems(OnExit(AppState::Playing), despawn_view_model)
            .add_systems(
                Update,
                (animate_view_model, update_held_item).run_if(in_state(AppState::Playing)),
            );
    }
}

/// Position de la main dans le repère de la caméra (assez près pour ne pas entrer dans les murs).
const HAND: Vec3 = Vec3::new(0.12, -0.09, -0.27);
/// Épaule, hors de l'écran : le bras pivote autour d'elle.
const SHOULDER: Vec3 = Vec3::new(0.17, -0.2, -0.13);
/// Taille du bras et de l'objet.
const SCALE: f32 = 0.6;
/// Durée d'un coup (minage, frappe).
const SWING_TIME: f32 = 0.3;
/// Durée de la descente puis remontée du bras quand on change d'objet.
const EQUIP_TIME: f32 = 0.22;

/// Pivot du bras (épaule ramenée à la main), enfant de la caméra.
#[derive(Component)]
struct ViewModel;

/// Point d'attache de l'objet tenu.
#[derive(Component)]
struct HeldAnchor;

/// Objet actuellement affiché dans la main.
#[derive(Component)]
struct HeldModel(Item);

/// Matériaux et maillages réutilisés d'un objet à l'autre.
#[derive(Resource, Default)]
struct HeldMaterials {
    blocks: HashMap<Block, Handle<StandardMaterial>>,
    icons: HashMap<Item, Handle<StandardMaterial>>,
    cube: Option<Handle<Mesh>>,
    quad: Option<Handle<Mesh>>,
}

/// État de l'animation.
#[derive(Default)]
struct Motion {
    /// Avancée du coup en cours (0 à 1), `None` au repos.
    swing: Option<f32>,
    /// Distance parcourue, pour le balancement de la marche.
    stride: f32,
    /// Amplitude du balancement (s'estompe à l'arrêt).
    bob: f32,
    /// Recul de l'arme à feu.
    recoil: f32,
    /// Levée de la garde (0 à 1).
    guard: f32,
    /// Changement d'objet en cours (1 → 0).
    equip: f32,
    held: Option<Item>,
}

fn spawn_view_model(
    mut commands: Commands,
    camera: Single<Entity, With<Camera3d>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let skin = materials.add(StandardMaterial {
        base_color: Color::srgb(0.86, 0.66, 0.52),
        perceptual_roughness: 0.9,
        ..default()
    });
    let sleeve = materials.add(StandardMaterial {
        base_color: Color::srgb(0.25, 0.32, 0.22),
        perceptual_roughness: 1.0,
        ..default()
    });
    let root = commands
        .spawn((
            ViewModel,
            Transform::from_translation(SHOULDER).with_scale(Vec3::splat(SCALE)),
            Visibility::Hidden,
            ChildOf(*camera),
        ))
        .id();
    // Le bras va de l'épaule à la main.
    let hand = (HAND - SHOULDER) / SCALE;
    let arm = commands
        .spawn((
            Transform::from_translation(hand).looking_to(hand, Vec3::Y),
            Visibility::Inherited,
            ChildOf(root),
        ))
        .id();
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(0.05, 0.05, 0.11))),
        MeshMaterial3d(skin),
        Transform::from_xyz(0.0, 0.0, 0.045),
        NotShadowCaster,
        ChildOf(arm),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(0.056, 0.056, hand.length()))),
        MeshMaterial3d(sleeve),
        Transform::from_xyz(0.0, 0.0, 0.1 + hand.length() / 2.0),
        NotShadowCaster,
        ChildOf(arm),
    ));
    commands.spawn((
        HeldAnchor,
        Transform::from_translation(hand + Vec3::new(-0.012, 0.03, -0.02)),
        Visibility::Inherited,
        ChildOf(root),
    ));
}

fn despawn_view_model(mut commands: Commands, roots: Query<Entity, With<ViewModel>>) {
    for root in &roots {
        commands.entity(root).despawn();
    }
}

/// Ressources de rendu d'un objet tenu.
#[derive(bevy::ecs::system::SystemParam)]
pub struct HeldAssets<'w> {
    cache: ResMut<'w, HeldMaterials>,
    icons: ResMut<'w, ItemIcons>,
    images: ResMut<'w, Assets<Image>>,
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
}

impl HeldAssets<'_> {
    /// Maillage et matériau d'un objet tenu : un cube unité texturé pour un bloc, sinon un
    /// carré unité portant l'icône (dessinée en diagonale, manche en bas à gauche).
    pub fn model(&mut self, item: Item) -> (Handle<Mesh>, Handle<StandardMaterial>) {
        let cache = &mut *self.cache;
        match item {
            Item::Block(block) => {
                let meshes = &mut self.meshes;
                let mesh = cache
                    .cube
                    .get_or_insert_with(|| meshes.add(Cuboid::from_length(1.0)))
                    .clone();
                let (images, materials) = (&mut self.images, &mut self.materials);
                let material = cache
                    .blocks
                    .entry(block)
                    .or_insert_with(|| {
                        let [r, g, b] = block.color();
                        materials.add(StandardMaterial {
                            base_color: Color::srgb(r, g, b),
                            base_color_texture: Some(images.add(crate::textures::build_block_texture(block))),
                            perceptual_roughness: 1.0,
                            ..default()
                        })
                    })
                    .clone();
                (mesh, material)
            }
            _ => {
                let meshes = &mut self.meshes;
                let mesh = cache
                    .quad
                    .get_or_insert_with(|| meshes.add(Rectangle::new(1.0, 1.0)))
                    .clone();
                let (icons, images, materials) = (&mut self.icons, &mut self.images, &mut self.materials);
                let material = cache
                    .icons
                    .entry(item)
                    .or_insert_with(|| {
                        materials.add(StandardMaterial {
                            base_color_texture: Some(icons.get(images, item)),
                            alpha_mode: AlphaMode::Mask(0.5),
                            double_sided: true,
                            cull_mode: None,
                            perceptual_roughness: 1.0,
                            ..default()
                        })
                    })
                    .clone();
                (mesh, material)
            }
        }
    }
}

/// Remplace le modèle de l'objet tenu quand il change.
fn update_held_item(
    mut commands: Commands,
    player: Res<LocalPlayer>,
    anchor: Single<Entity, With<HeldAnchor>>,
    current: Query<(Entity, &HeldModel)>,
    mut assets: HeldAssets,
) {
    let held = player.held();
    if current.iter().map(|(_, m)| Some(m.0)).next().unwrap_or(None) == held {
        return;
    }
    for (entity, _) in &current {
        commands.entity(entity).despawn();
    }
    let Some(item) = held else { return };
    let (mesh, material) = assets.model(item);
    let transform = match item {
        Item::Block(_) => Transform::from_xyz(-0.01, 0.0, 0.0)
            .with_rotation(Quat::from_euler(EulerRot::YXZ, 0.6, 0.15, 0.0))
            .with_scale(Vec3::splat(0.06)),
        // L'icône est tenue par le bas, tournée vers le centre de l'écran.
        _ => Transform::from_xyz(0.0, 0.035, 0.0)
            .with_rotation(Quat::from_euler(EulerRot::YXZ, 0.75, 0.0, 0.1))
            .with_scale(Vec3::splat(0.1)),
    };
    commands.spawn((
        HeldModel(item),
        Mesh3d(mesh),
        MeshMaterial3d(material),
        transform,
        NotShadowCaster,
        ChildOf(*anchor),
    ));
}

fn animate_view_model(
    player: Res<LocalPlayer>,
    time: Res<Time>,
    mouse: Res<ButtonInput<MouseButton>>,
    cursor: Single<&CursorOptions>,
    mut root: Single<(&mut Transform, &mut Visibility), With<ViewModel>>,
    mut motion: Local<Motion>,
) {
    let dt = time.delta_secs();
    let (transform, visibility) = &mut *root;
    let shown = player.dead.is_none() && player.mounted.is_none() && !player.panel_open();
    **visibility = if shown {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    let active = shown && cursor.grab_mode != CursorGrabMode::None && !player.typing;
    let held = player.held();

    // Changement d'objet : le bras redescend puis remonte.
    if held != motion.held {
        motion.held = held;
        motion.equip = 1.0;
    }
    motion.equip = (motion.equip - dt / EQUIP_TIME).max(0.0);

    let ranged = held.and_then(Item::ranged).is_some();
    let attacking = active && mouse.pressed(MouseButton::Left);
    if attacking && ranged {
        // Petit recul répété tant qu'on tire.
        motion.recoil = (motion.recoil + dt * 8.0).min(1.0);
    } else {
        motion.recoil = (motion.recoil - dt * 6.0).max(0.0);
    }
    // Coup : en continu tant que le clic gauche est maintenu, un seul pour poser un bloc.
    let starts = (attacking && !ranged) || (active && mouse.just_pressed(MouseButton::Right) && !player.guarding);
    match motion.swing {
        Some(t) => {
            let t = t + dt / SWING_TIME;
            motion.swing = if t < 1.0 {
                Some(t)
            } else if attacking && !ranged {
                Some(t.fract())
            } else {
                None
            };
        }
        None if starts => motion.swing = Some(0.0),
        None => {}
    }
    let target_guard = if player.guarding { 1.0 } else { 0.0 };
    motion.guard += (target_guard - motion.guard) * (dt * 12.0).min(1.0);

    // Balancement de la marche.
    let speed = Vec2::new(player.velocity.x, player.velocity.z).length();
    let walking = player.on_ground && speed > 0.5;
    motion.stride += speed * dt;
    let target_bob = if walking { (speed / 5.0).min(1.3) } else { 0.0 };
    motion.bob += (target_bob - motion.bob) * (dt * 8.0).min(1.0);
    let phase = motion.stride * 1.6;
    let bob = Vec3::new((phase).sin() * 0.008, -(phase).cos().abs() * 0.007, 0.0) * motion.bob;

    // Coup : le bras se lève un peu, s'abat vers le centre puis revient.
    let (swing_rot, swing_pos) = match motion.swing {
        Some(t) => {
            let s = (t * PI).sin();
            let lift = (t * PI * 2.0).sin().max(0.0);
            (
                Quat::from_euler(EulerRot::YXZ, 0.3 * s, 0.15 * lift - 0.2 * s, 0.1 * s),
                Vec3::new(-0.03 * s, 0.03 * s, -0.025 * s),
            )
        }
        None => (Quat::IDENTITY, Vec3::ZERO),
    };
    let recoil = motion.recoil * (0.5 + 0.5 * (time.elapsed_secs() * 40.0).sin());
    let recoil_pos = Vec3::new(0.0, 0.0, 0.02) * recoil;
    let recoil_rot = Quat::from_rotation_x(0.08 * recoil);
    // Garde : l'objet monte au centre, en travers.
    let guard_pos = Vec3::new(-0.05, 0.03, 0.0) * motion.guard;
    let guard_rot = Quat::from_euler(
        EulerRot::YXZ,
        0.3 * motion.guard,
        0.15 * motion.guard,
        0.5 * motion.guard,
    );
    let equip = (motion.equip * PI).sin();
    let equip_rot = Quat::from_rotation_x(-0.5 * equip);

    transform.translation = SHOULDER + bob + swing_pos + recoil_pos + guard_pos;
    transform.rotation = swing_rot * recoil_rot * guard_rot * equip_rot;
    transform.scale = Vec3::splat(SCALE);
}
