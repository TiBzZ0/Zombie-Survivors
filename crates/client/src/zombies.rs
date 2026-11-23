//! Affichage des zombies envoyés par le serveur : modèle simple par type,
//! interpolation, animation de marche et d'attaque.

use std::collections::HashMap;

use bevy::prelude::*;
use shared::protocol::ZombieState;
use shared::zombie::ZombieKind;

use crate::AppState;
use crate::net::NetEvent;

pub struct ZombiesPlugin;

impl Plugin for ZombiesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ZombieRegistry>()
            .add_systems(Startup, setup_assets)
            .add_systems(
                Update,
                (receive_zombies, animate_zombies, expire_zombies)
                    .chain()
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(OnExit(AppState::Playing), clear_zombies);
    }
}

/// Zombies affichés, par identifiant serveur.
#[derive(Resource, Default)]
pub struct ZombieRegistry {
    pub entities: HashMap<u32, Entity>,
}

/// Un zombie affiché.
#[derive(Component)]
pub struct ZombieView {
    pub id: u32,
    pub kind: ZombieKind,
    target: Vec3,
    target_yaw: f32,
    pub flags: u8,
    /// Part de vie restante (0 à 1).
    pub health: f32,
    last_seen: f32,
    walk_phase: f32,
}

#[derive(Component)]
struct Arm(f32);

/// Partie du corps, avec sa couleur normale (pour revenir après le flash rouge).
#[derive(Component)]
struct ZombieBodyPart(Handle<StandardMaterial>);

/// Œil : terne d'habitude, rouge vif quand le zombie est enragé.
#[derive(Component)]
struct Eye;

#[derive(Resource)]
struct ZombieAssets {
    cube: Handle<Mesh>,
    skin: [Handle<StandardMaterial>; 8],
    clothes: [Handle<StandardMaterial>; 8],
    hurt: Handle<StandardMaterial>,
    /// Plaques du blindé.
    metal: Handle<StandardMaterial>,
    /// Bouche grande ouverte du hurleur.
    mouth: Handle<StandardMaterial>,
    /// Pustules du gonflé, et sa lueur quand la mèche brûle.
    pustule: Handle<StandardMaterial>,
    fuse: Handle<StandardMaterial>,
    eye: Handle<StandardMaterial>,
    rage: Handle<StandardMaterial>,
}

fn setup_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut glow = |r: f32, g: f32, b: f32, light: f32| {
        materials.add(StandardMaterial {
            base_color: Color::srgb(r, g, b),
            emissive: LinearRgba::rgb(r, g, b) * light,
            perceptual_roughness: 1.0,
            ..default()
        })
    };
    let pustule = glow(0.85, 0.55, 0.15, 0.6);
    let fuse = glow(1.0, 0.45, 0.1, 6.0);
    let eye = glow(0.55, 0.5, 0.2, 0.4);
    let rage = glow(1.0, 0.1, 0.05, 8.0);
    let metal = materials.add(StandardMaterial {
        base_color: Color::srgb(0.5, 0.52, 0.55),
        metallic: 0.8,
        perceptual_roughness: 0.45,
        ..default()
    });
    let mut mat = |r, g, b| {
        materials.add(StandardMaterial {
            base_color: Color::srgb(r, g, b),
            perceptual_roughness: 1.0,
            ..default()
        })
    };
    commands.insert_resource(ZombieAssets {
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        // Marcheur, coureur, brute, cracheur (verdâtre), colosse (gris pâle), blindé,
        // hurleur (blafard et décharné), gonflé (jaunâtre).
        skin: [
            mat(0.38, 0.55, 0.30),
            mat(0.55, 0.62, 0.36),
            mat(0.30, 0.40, 0.25),
            mat(0.62, 0.78, 0.20),
            mat(0.55, 0.55, 0.50),
            mat(0.33, 0.45, 0.30),
            mat(0.72, 0.74, 0.66),
            mat(0.74, 0.76, 0.38),
        ],
        clothes: [
            mat(0.25, 0.22, 0.35),
            mat(0.45, 0.20, 0.18),
            mat(0.18, 0.16, 0.14),
            mat(0.30, 0.36, 0.12),
            mat(0.12, 0.10, 0.10),
            mat(0.20, 0.22, 0.18),
            mat(0.30, 0.26, 0.26),
            mat(0.42, 0.36, 0.22),
        ],
        hurt: mat(0.9, 0.15, 0.1),
        metal,
        mouth: mat(0.12, 0.03, 0.03),
        pustule,
        fuse,
        eye,
        rage,
    });
}

/// Hauteur d'un zombie selon son type (le modèle est mis à l'échelle).
/// Au-delà (en mètres), un zombie glisse sans animer ses bras.
const DETAIL_DIST: f32 = 40.0;

fn scale(kind: ZombieKind) -> Vec3 {
    let s = kind.stats().size;
    Vec3::new(s.half_width / 0.3, s.height / 1.8, s.half_width / 0.3)
}

fn spawn_zombie(commands: &mut Commands, assets: &ZombieAssets, state: &ZombieState, now: f32) -> Entity {
    let kind = state.kind();
    let i = kind as usize;
    let part = |mat: &Handle<StandardMaterial>, size: Vec3, at: Vec3| {
        (
            ZombieBodyPart(mat.clone()),
            Mesh3d(assets.cube.clone()),
            MeshMaterial3d(mat.clone()),
            Transform::from_translation(at).with_scale(size),
        )
    };
    commands
        .spawn((
            ZombieView {
                id: state.id,
                kind,
                target: state.position(),
                target_yaw: state.yaw(),
                flags: state.flags,
                health: state.health_fraction(),
                last_seen: now,
                walk_phase: (state.id % 7) as f32,
            },
            Transform::from_translation(state.position())
                .with_rotation(Quat::from_rotation_y(state.yaw()))
                .with_scale(scale(kind)),
            Visibility::default(),
        ))
        .with_children(|z| {
            // Jambes, torse, tête. Le zombie regarde vers -Z.
            z.spawn(part(
                &assets.clothes[i],
                Vec3::new(0.22, 0.8, 0.22),
                Vec3::new(-0.13, 0.4, 0.0),
            ));
            z.spawn(part(
                &assets.clothes[i],
                Vec3::new(0.22, 0.8, 0.22),
                Vec3::new(0.13, 0.4, 0.0),
            ));
            z.spawn(part(
                &assets.clothes[i],
                Vec3::new(0.55, 0.65, 0.3),
                Vec3::new(0.0, 1.12, 0.0),
            ));
            z.spawn(part(
                &assets.skin[i],
                Vec3::new(0.42, 0.42, 0.42),
                Vec3::new(0.0, 1.62, -0.02),
            ));
            // Yeux.
            for side in [-1.0f32, 1.0] {
                z.spawn((
                    Eye,
                    part(
                        &assets.eye,
                        Vec3::new(0.08, 0.06, 0.04),
                        Vec3::new(side * 0.1, 1.67, -0.24),
                    ),
                ));
            }
            // Détails propres à chaque type.
            match kind {
                ZombieKind::Armored => {
                    // Plastron, dossière, casque et épaulières.
                    z.spawn(part(
                        &assets.metal,
                        Vec3::new(0.6, 0.5, 0.06),
                        Vec3::new(0.0, 1.15, -0.17),
                    ));
                    z.spawn(part(
                        &assets.metal,
                        Vec3::new(0.6, 0.5, 0.06),
                        Vec3::new(0.0, 1.15, 0.17),
                    ));
                    z.spawn(part(
                        &assets.metal,
                        Vec3::new(0.48, 0.16, 0.48),
                        Vec3::new(0.0, 1.88, -0.02),
                    ));
                    z.spawn(part(
                        &assets.metal,
                        Vec3::new(0.06, 0.12, 0.44),
                        Vec3::new(0.0, 1.62, -0.22),
                    ));
                    for side in [-1.0f32, 1.0] {
                        z.spawn(part(
                            &assets.metal,
                            Vec3::new(0.22, 0.1, 0.3),
                            Vec3::new(side * 0.36, 1.5, 0.0),
                        ));
                    }
                }
                ZombieKind::Howler => {
                    // Bouche béante et côtes saillantes.
                    z.spawn(part(
                        &assets.mouth,
                        Vec3::new(0.2, 0.18, 0.04),
                        Vec3::new(0.0, 1.5, -0.235),
                    ));
                    for rib in 0..3 {
                        z.spawn(part(
                            &assets.skin[i],
                            Vec3::new(0.5, 0.04, 0.04),
                            Vec3::new(0.0, 1.0 + rib as f32 * 0.12, -0.16),
                        ));
                    }
                }
                ZombieKind::Bloater => {
                    // Ventre énorme couvert de pustules.
                    z.spawn(part(
                        &assets.skin[i],
                        Vec3::new(0.8, 0.7, 0.7),
                        Vec3::new(0.0, 1.05, -0.08),
                    ));
                    for (x, y, zz) in [
                        (-0.2, 1.2, -0.44),
                        (0.18, 0.95, -0.44),
                        (0.42, 1.1, -0.1),
                        (-0.42, 0.9, 0.0),
                        (0.05, 1.32, -0.4),
                    ] {
                        z.spawn(part(&assets.pustule, Vec3::splat(0.12), Vec3::new(x, y, zz)));
                    }
                }
                _ => {}
            }
            // Bras tendus vers l'avant, pivot à l'épaule.
            for side in [-1.0f32, 1.0] {
                z.spawn((
                    Arm(side),
                    Transform::from_xyz(side * 0.36, 1.38, 0.0),
                    Visibility::default(),
                ))
                .with_children(|arm| {
                    arm.spawn(part(
                        &assets.skin[i],
                        Vec3::new(0.16, 0.16, 0.7),
                        Vec3::new(0.0, 0.0, -0.32),
                    ));
                });
            }
        })
        .id()
}

/// Hauteur où naissent les chiffres de dégâts, juste au-dessus de la tête.
fn popup_height(kind: ZombieKind) -> f32 {
    2.1 * scale(kind).y
}

fn receive_zombies(
    mut commands: Commands,
    mut events: MessageReader<NetEvent>,
    mut registry: ResMut<ZombieRegistry>,
    mut views: Query<&mut ZombieView>,
    assets: Res<ZombieAssets>,
    time: Res<Time>,
    mut popups: MessageWriter<crate::damage::DamagePopup>,
) {
    let now = time.elapsed_secs();
    for event in events.read() {
        match event {
            NetEvent::Zombies(states) => {
                for state in states {
                    match registry.entities.get(&state.id).and_then(|e| views.get_mut(*e).ok()) {
                        Some(mut view) => {
                            let health = state.health_fraction();
                            if health < view.health - 0.001 {
                                popups.write(crate::damage::DamagePopup {
                                    pos: view.target + Vec3::Y * popup_height(view.kind),
                                    amount: (view.health - health) * view.kind.stats().health,
                                    kill: false,
                                });
                            }
                            view.target = state.position();
                            view.target_yaw = state.yaw();
                            view.flags = state.flags;
                            view.health = state.health_fraction();
                            view.last_seen = now;
                        }
                        None => {
                            let entity = spawn_zombie(&mut commands, &assets, state, now);
                            registry.entities.insert(state.id, entity);
                        }
                    }
                }
            }
            NetEvent::ZombieKilled(id) => {
                if let Some(entity) = registry.entities.remove(id) {
                    if let Ok(view) = views.get(entity) {
                        popups.write(crate::damage::DamagePopup {
                            pos: view.target + Vec3::Y * popup_height(view.kind),
                            amount: view.health * view.kind.stats().health,
                            kill: true,
                        });
                    }
                    commands.entity(entity).despawn();
                }
            }
            _ => {}
        }
    }
}

fn animate_zombies(
    time: Res<Time>,
    assets: Res<ZombieAssets>,
    mut zombies: Query<(&mut ZombieView, &mut Transform, &Children), Without<Arm>>,
    mut arms: Query<(&Arm, &mut Transform), Without<ZombieView>>,
    mut parts: Query<(&mut MeshMaterial3d<StandardMaterial>, &ZombieBodyPart, Has<Eye>)>,
    children: Query<&Children>,
    camera: Query<&GlobalTransform, With<Camera3d>>,
) {
    let dt = time.delta_secs();
    let t = (dt * 10.0).min(1.0);
    let eye = camera.iter().next().map(|c| c.translation());
    for (mut view, mut transform, kids) in &mut zombies {
        let moved = transform.translation.distance(view.target);
        transform.translation = transform.translation.lerp(view.target, t);
        transform.rotation = transform.rotation.slerp(Quat::from_rotation_y(view.target_yaw), t);
        let speed = view.kind.stats().speed;
        view.walk_phase += (moved / dt.max(1e-3)).min(speed) * dt * 3.0;
        // De loin, les bras et le flash rouge ne se voient pas : on s'épargne ce travail.
        if eye.is_some_and(|e| e.distance(transform.translation) > DETAIL_DIST) {
            continue;
        }
        let attacking = view.flags & ZombieState::ATTACKING != 0;
        let hurt = view.flags & ZombieState::HURT != 0;
        let frenzy = view.flags & ZombieState::FRENZY != 0;
        // Mèche allumée : le gonflé gonfle et clignote de plus en plus vite.
        let fuse = view.flags & ZombieState::FUSE != 0;
        let blink = fuse && (time.elapsed_secs() * 18.0).sin() > 0.0;
        let base = scale(view.kind);
        transform.scale = if fuse {
            base * (1.0 + 0.12 * (time.elapsed_secs() * 18.0).sin().abs())
        } else {
            base
        };
        for kid in kids.iter() {
            if let Ok((arm, mut arm_transform)) = arms.get_mut(kid) {
                let climbing = view.flags & ZombieState::CLIMBING != 0;
                let swing = if climbing {
                    // Bras levés, qui s'agrippent à tour de rôle.
                    -2.4 + (time.elapsed_secs() * 8.0 + arm.0 * 1.5).sin() * 0.4
                } else if attacking {
                    (time.elapsed_secs() * 14.0 + arm.0).sin() * 0.7 - 0.2
                } else {
                    (view.walk_phase + arm.0).sin() * 0.15
                };
                arm_transform.rotation = Quat::from_rotation_x(swing);
            }
        }
        // Flash rouge quand il est touché.
        for kid in kids.iter() {
            let grandchildren = children.get(kid).into_iter().flat_map(|c| c.iter());
            for e in std::iter::once(kid).chain(grandchildren) {
                if let Ok((mut material, part, eye)) = parts.get_mut(e) {
                    let wanted = if blink {
                        &assets.fuse
                    } else if hurt {
                        &assets.hurt
                    } else if eye && frenzy {
                        &assets.rage
                    } else {
                        &part.0
                    };
                    if material.0 != *wanted {
                        material.0 = wanted.clone();
                    }
                }
            }
        }
    }
}

/// Retire les zombies que le serveur n'envoie plus (morts ou trop loin).
fn expire_zombies(
    mut commands: Commands,
    time: Res<Time>,
    mut registry: ResMut<ZombieRegistry>,
    views: Query<(Entity, &ZombieView)>,
) {
    let now = time.elapsed_secs();
    for (entity, view) in &views {
        if now - view.last_seen > 1.0 {
            registry.entities.remove(&view.id);
            commands.entity(entity).despawn();
        }
    }
}

fn clear_zombies(mut commands: Commands, mut registry: ResMut<ZombieRegistry>) {
    for entity in registry.entities.values() {
        commands.entity(*entity).despawn();
    }
    registry.entities.clear();
}
