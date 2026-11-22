//! Affichage des survivants envoyés par le serveur : un personnage humain,
//! interpolé et animé, avec un repère au-dessus de la tête.

use std::collections::HashMap;

use bevy::prelude::*;
use shared::item::Item;
use shared::protocol::SurvivorState;
use shared::raycast::ray_aabb;
use shared::survivor::{Order, SURVIVOR_SIZE, SurvivorStats};

use crate::AppState;
use crate::net::{NetEvent, Session};
use crate::player::LocalPlayer;

pub struct SurvivorsPlugin;

impl Plugin for SurvivorsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SurvivorRegistry>()
            .add_systems(Startup, setup_assets)
            .add_systems(
                Update,
                (
                    receive_survivors,
                    animate_survivors,
                    expire_survivors,
                    draw_markers,
                    update_hint,
                )
                    .chain()
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(OnExit(AppState::Playing), clear_survivors);
    }
}

/// Identité d'un survivant.
#[derive(Clone, Debug)]
pub struct SurvivorInfo {
    pub name: String,
    pub stats: SurvivorStats,
    pub leader: Option<String>,
    pub order: Order,
    pub weapon: Option<Item>,
    pub armor: [Option<Item>; 2],
    /// Marchés d'un marchand (vide sinon).
    pub offers: Vec<shared::survivor::Offer>,
}

/// Survivants affichés et identités connues, par identifiant serveur.
#[derive(Resource, Default)]
pub struct SurvivorRegistry {
    entities: HashMap<u32, Entity>,
    pub infos: HashMap<u32, SurvivorInfo>,
}

/// Un survivant affiché.
#[derive(Component)]
pub struct SurvivorView {
    pub id: u32,
    target: Vec3,
    target_yaw: f32,
    flags: u8,
    /// Moral, de 0 à 100.
    morale: f32,
    /// Part de vie restante (0 à 1).
    pub health: f32,
    last_seen: f32,
    walk_phase: f32,
}

impl SurvivorView {
    /// Le survivant est en train de travailler (récolte, réparation, cuisine, semis).
    pub fn working(&self) -> bool {
        self.flags & SurvivorState::WORKING != 0
    }
}

#[derive(Component)]
struct Limb {
    side: f32,
    arm: bool,
}

#[derive(Resource)]
struct SurvivorAssets {
    cube: Handle<Mesh>,
    skin: [Handle<StandardMaterial>; 3],
    shirts: [Handle<StandardMaterial>; 4],
    pants: Handle<StandardMaterial>,
    hair: Handle<StandardMaterial>,
}

fn setup_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut mat = |r, g, b| {
        materials.add(StandardMaterial {
            base_color: Color::srgb(r, g, b),
            perceptual_roughness: 1.0,
            ..default()
        })
    };
    commands.insert_resource(SurvivorAssets {
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        skin: [mat(0.93, 0.76, 0.62), mat(0.72, 0.53, 0.38), mat(0.45, 0.31, 0.22)],
        shirts: [
            mat(0.75, 0.45, 0.12),
            mat(0.20, 0.45, 0.30),
            mat(0.55, 0.15, 0.15),
            mat(0.35, 0.35, 0.40),
        ],
        pants: mat(0.18, 0.22, 0.35),
        hair: mat(0.15, 0.10, 0.07),
    });
}

fn spawn_survivor(commands: &mut Commands, assets: &SurvivorAssets, state: &SurvivorState, now: f32) -> Entity {
    let id = state.id as usize;
    let skin = &assets.skin[id % 3];
    let shirt = &assets.shirts[(id * 7 / 3) % 4];
    let part = |mat: &Handle<StandardMaterial>, size: Vec3, at: Vec3| {
        (
            Mesh3d(assets.cube.clone()),
            MeshMaterial3d(mat.clone()),
            Transform::from_translation(at).with_scale(size),
        )
    };
    commands
        .spawn((
            SurvivorView {
                id: state.id,
                target: state.position(),
                target_yaw: state.yaw(),
                flags: state.flags,
                morale: state.morale as f32,
                health: state.health_fraction(),
                last_seen: now,
                walk_phase: 0.0,
            },
            Transform::from_translation(state.position()).with_rotation(Quat::from_rotation_y(state.yaw())),
            Visibility::default(),
        ))
        .with_children(|s| {
            // Torse, tête et cheveux. Le survivant regarde vers -Z.
            s.spawn(part(shirt, Vec3::new(0.5, 0.65, 0.28), Vec3::new(0.0, 1.12, 0.0)));
            s.spawn(part(skin, Vec3::new(0.4, 0.4, 0.4), Vec3::new(0.0, 1.62, 0.0)));
            s.spawn(part(
                &assets.hair,
                Vec3::new(0.42, 0.12, 0.42),
                Vec3::new(0.0, 1.86, 0.02),
            ));
            // Jambes et bras, pivot à la hanche et à l'épaule.
            for side in [-1.0f32, 1.0] {
                s.spawn((
                    Limb { side, arm: false },
                    Transform::from_xyz(side * 0.13, 0.8, 0.0),
                    Visibility::default(),
                ))
                .with_children(|leg| {
                    leg.spawn(part(
                        &assets.pants,
                        Vec3::new(0.22, 0.8, 0.24),
                        Vec3::new(0.0, -0.4, 0.0),
                    ));
                });
                s.spawn((
                    Limb { side, arm: true },
                    Transform::from_xyz(side * 0.34, 1.4, 0.0),
                    Visibility::default(),
                ))
                .with_children(|arm| {
                    arm.spawn(part(shirt, Vec3::new(0.16, 0.5, 0.18), Vec3::new(0.0, -0.22, 0.0)));
                    arm.spawn(part(skin, Vec3::new(0.15, 0.15, 0.15), Vec3::new(0.0, -0.53, 0.0)));
                });
            }
        })
        .id()
}

fn receive_survivors(
    mut commands: Commands,
    mut events: MessageReader<NetEvent>,
    mut registry: ResMut<SurvivorRegistry>,
    mut views: Query<&mut SurvivorView>,
    assets: Res<SurvivorAssets>,
    time: Res<Time>,
) {
    let now = time.elapsed_secs();
    for event in events.read() {
        match event {
            NetEvent::Survivors(states) => {
                for state in states {
                    match registry.entities.get(&state.id).and_then(|e| views.get_mut(*e).ok()) {
                        Some(mut view) => {
                            view.target = state.position();
                            view.target_yaw = state.yaw();
                            view.flags = state.flags;
                            view.morale = state.morale as f32;
                            view.health = state.health_fraction();
                            view.last_seen = now;
                        }
                        None => {
                            let entity = spawn_survivor(&mut commands, &assets, state, now);
                            registry.entities.insert(state.id, entity);
                        }
                    }
                }
            }
            NetEvent::SurvivorInfo {
                id,
                name,
                stats,
                leader,
                order,
                weapon,
                armor,
                offers,
            } => {
                registry.infos.insert(
                    *id,
                    SurvivorInfo {
                        name: name.clone(),
                        stats: *stats,
                        leader: leader.clone(),
                        order: *order,
                        weapon: *weapon,
                        armor: *armor,
                        offers: offers.clone(),
                    },
                );
            }
            _ => {}
        }
    }
}

fn animate_survivors(
    time: Res<Time>,
    mut survivors: Query<(&mut SurvivorView, &mut Transform, &Children), Without<Limb>>,
    mut limbs: Query<(&Limb, &mut Transform), Without<SurvivorView>>,
) {
    let dt = time.delta_secs();
    let t = (dt * 10.0).min(1.0);
    let now = time.elapsed_secs();
    for (mut view, mut transform, kids) in &mut survivors {
        let moved = transform.translation.distance(view.target);
        transform.translation = transform.translation.lerp(view.target, t);
        transform.rotation = transform.rotation.slerp(Quat::from_rotation_y(view.target_yaw), t);
        let speed = (moved / dt.max(1e-3)).min(6.0);
        view.walk_phase += speed * dt * 3.0;
        let attacking = view.flags & SurvivorState::ATTACKING != 0;
        let working = view.flags & SurvivorState::WORKING != 0;
        let walking = speed > 0.3;
        for kid in kids.iter() {
            let Ok((limb, mut limb_transform)) = limbs.get_mut(kid) else {
                continue;
            };
            let swing = (view.walk_phase + if limb.side > 0.0 { 0.0 } else { std::f32::consts::PI }).sin();
            let angle = if !limb.arm {
                if walking { swing * 0.6 } else { 0.0 }
            } else if attacking && limb.side > 0.0 {
                // Frappe avec le bras droit, de haut en bas.
                1.2 + (now * 14.0).sin() * 1.0
            } else if working && limb.side > 0.0 && !walking {
                0.9 + (now * 9.0).sin() * 0.5
            } else if walking {
                -swing * 0.5
            } else {
                0.0
            };
            limb_transform.rotation = Quat::from_rotation_x(angle);
        }
    }
}

/// Retire les survivants que le serveur n'envoie plus (morts ou trop loin).
fn expire_survivors(
    mut commands: Commands,
    time: Res<Time>,
    mut registry: ResMut<SurvivorRegistry>,
    views: Query<(Entity, &SurvivorView)>,
) {
    let now = time.elapsed_secs();
    for (entity, view) in &views {
        if now - view.last_seen > 1.0 {
            registry.entities.remove(&view.id);
            commands.entity(entity).despawn();
        }
    }
}

/// Repère au-dessus des survivants : jaune pour ceux à recruter, vert pour les vôtres.
fn draw_markers(
    registry: Res<SurvivorRegistry>,
    session: Option<Res<Session>>,
    views: Query<(&SurvivorView, &Transform)>,
    time: Res<Time>,
    mut gizmos: Gizmos,
) {
    let bob = (time.elapsed_secs() * 2.0).sin() * 0.06;
    for (view, transform) in &views {
        let Some(info) = registry.infos.get(&view.id) else {
            continue;
        };
        let top = transform.translation + Vec3::Y * (SURVIVOR_SIZE.height + 0.45 + bob);
        let mine = session
            .as_ref()
            .is_some_and(|s| info.leader.as_ref() == Some(&s.my_name));
        let color = match (&info.leader, mine) {
            (None, _) => Color::srgb(1.0, 0.85, 0.2),
            (Some(_), true) => Color::srgb(0.3, 0.95, 0.4),
            (Some(_), false) => Color::srgb(0.4, 0.6, 1.0),
        };
        // Petit losange.
        let r = 0.12;
        let points = [
            top + Vec3::Y * r,
            top + Vec3::X * r,
            top - Vec3::Y * r,
            top - Vec3::X * r,
            top + Vec3::Y * r,
        ];
        gizmos.linestrip(points, color);
        let points = [
            top + Vec3::Y * r,
            top + Vec3::Z * r,
            top - Vec3::Y * r,
            top - Vec3::Z * r,
            top + Vec3::Y * r,
        ];
        gizmos.linestrip(points, color);
    }
}

/// Texte d'aide sur le survivant visé : qui il est et ce qu'un clic droit fera.
fn update_hint(
    registry: Res<SurvivorRegistry>,
    session: Option<Res<Session>>,
    views: Query<&SurvivorView>,
    mut player: ResMut<LocalPlayer>,
) {
    let hungry = |id: u32| views.iter().any(|v| v.id == id && v.flags & SurvivorState::HUNGRY != 0);
    let morale = |id: u32| {
        views
            .iter()
            .find(|v| v.id == id)
            .map(|v| format!(" · {}", shared::survivor::morale_label(v.morale)))
            .unwrap_or_default()
    };
    let hint = player
        .aimed_survivor
        .and_then(|id| registry.infos.get(&id).map(|info| (id, info)))
        .map(|(id, info)| {
            let hunger = if hungry(id) {
                " · AFFAMÉ : mets de la nourriture dans ton sac ou dans le coffre de son poste"
            } else {
                ""
            };
            let me = session.as_ref().map(|s| s.my_name.as_str());
            let armed: String = info
                .weapon
                .iter()
                .chain(info.armor.iter().flatten())
                .map(|i| format!(", {}", i.name().to_lowercase()))
                .collect();
            match &info.leader {
                None if !info.offers.is_empty() => {
                    let list: Vec<String> = info.offers.iter().map(|o| format!("· {}", o.describe())).collect();
                    format!("{}\n{}\nClic droit : échanger (le premier marché que tu peux payer)", info.name, list.join("\n"))
                }
                None => format!(
                    "{} ({})\nClic droit : recruter contre 1 ration ou 3 pommes",
                    info.name,
                    info.stats.describe()
                ),
                Some(leader) if Some(leader.as_str()) == me => format!(
                    "{} {} ({}{armed}){}{hunger}\nClic droit : ordre suivant ({}) · Ctrl : confier l'arme ou l'armure tenue · Maj : renvoyer",
                    info.name,
                    info.order.label(),
                    info.stats.describe(),
                    morale(id),
                    info.order.next().label()
                ),
                Some(leader) => format!("{}, survivant de {leader} ({}{armed})", info.name, info.order.label()),
            }
        });
    if player.survivor_hint != hint {
        player.survivor_hint = hint;
    }
}

fn clear_survivors(mut commands: Commands, mut registry: ResMut<SurvivorRegistry>) {
    for entity in registry.entities.values() {
        commands.entity(*entity).despawn();
    }
    *registry = SurvivorRegistry::default();
}

/// Survivant visé à moins de `reach`, et sa distance.
pub fn aimed_survivor(
    eye: Vec3,
    dir: Vec3,
    reach: f32,
    survivors: &Query<(&SurvivorView, &Transform)>,
) -> Option<(u32, f32)> {
    let half = Vec3::new(SURVIVOR_SIZE.half_width + 0.1, 0.0, SURVIVOR_SIZE.half_width + 0.1);
    survivors
        .iter()
        .filter_map(|(view, transform)| {
            let min = transform.translation - half;
            let max = transform.translation + half + Vec3::Y * SURVIVOR_SIZE.height;
            ray_aabb(eye, dir, min, max)
                .filter(|t| *t <= reach)
                .map(|t| (view.id, t))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
}
