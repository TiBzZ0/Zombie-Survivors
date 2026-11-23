//! Évacuation côté client : bandeau de l'appel de détresse, hélicoptère qui se pose près de
//! la radio puis repart, et écran de victoire pour ceux qui sont montés à bord.

use bevy::prelude::*;
use shared::rescue::{BOARD_RADIUS, PAD_OFFSET, RescueState};

use crate::AppState;
use crate::net::{ClientWorld, NetEvent};
use crate::player::LocalPlayer;
use crate::worldmap::Waypoint;

/// Hauteur d'où l'hélicoptère descend.
const APPROACH_HEIGHT: f32 = 45.0;
/// Vitesse de descente et de montée, en blocs par seconde.
const CLIMB: f32 = 7.0;

pub struct RescuePlugin;

impl Plugin for RescuePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RescueView>()
            .add_systems(OnEnter(AppState::Playing), (spawn_banner, reset))
            .add_systems(
                Update,
                (receive, fly_helicopter, update_banner, close_victory)
                    .chain()
                    .run_if(in_state(AppState::Playing))
                    .run_if(resource_exists::<ClientWorld>),
            );
    }
}

#[derive(Resource, Default)]
struct RescueView {
    state: Option<RescueState>,
    /// Secondes restantes au sol, décomptées localement.
    left: f32,
}

#[derive(Component)]
struct Banner;

#[derive(Component)]
struct Helicopter {
    /// Altitude du sol sous l'hélicoptère.
    ground: f32,
    leaving: bool,
    age: f32,
}

#[derive(Component)]
struct Rotor(f32);

#[derive(Component)]
struct Victory;

fn reset(mut view: ResMut<RescueView>) {
    *view = RescueView::default();
}

fn spawn_banner(mut commands: Commands) {
    commands.spawn((
        DespawnOnExit(AppState::Playing),
        Node {
            position_type: PositionType::Absolute,
            top: px(64),
            width: percent(100),
            justify_content: JustifyContent::Center,
            ..default()
        },
        Pickable::IGNORE,
        children![(
            Banner,
            Text::new(""),
            TextFont {
                font_size: FontSize::Px(18.0),
                ..default()
            },
            TextColor(Color::srgb(1.0, 0.75, 0.3)),
            TextShadow::default(),
            TextLayout::justify(Justify::Center),
        )],
    ));
}

fn pad(state: &RescueState) -> Vec3 {
    (state.radio + PAD_OFFSET).as_vec3() + Vec3::new(0.5, 0.0, 0.5)
}

/// Sol sous le point d'atterrissage : premier bloc solide en descendant.
fn ground_at(world: &ClientWorld, at: Vec3) -> f32 {
    let mut p = at.floor().as_ivec3() + IVec3::Y * 6;
    for _ in 0..24 {
        if world.block(p).is_solid() {
            return p.y as f32 + 1.0;
        }
        p.y -= 1;
    }
    at.y
}

#[allow(clippy::too_many_arguments)]
fn receive(
    mut commands: Commands,
    mut events: MessageReader<NetEvent>,
    mut view: ResMut<RescueView>,
    mut waypoint: ResMut<Waypoint>,
    world: Res<ClientWorld>,
    player: Res<LocalPlayer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut helicopters: Query<&mut Helicopter>,
) {
    for event in events.read() {
        match event {
            NetEvent::Rescue(state) => {
                let was_landed = view.state.is_some_and(|s| s.landed.is_some());
                if let Some(s) = state {
                    let p = pad(s);
                    waypoint.0 = Some(Vec2::new(p.x, p.z));
                    if let Some(left) = s.landed {
                        view.left = left;
                        if helicopters.is_empty() {
                            let ground = ground_at(&world, p);
                            spawn_helicopter(&mut commands, &mut meshes, &mut materials, p, ground);
                        }
                    }
                } else if was_landed {
                    for mut heli in &mut helicopters {
                        heli.leaving = true;
                        heli.age = 0.0;
                    }
                }
                view.state = *state;
            }
            NetEvent::Evacuated(day) => {
                spawn_victory(&mut commands, *day, &player);
            }
            _ => {}
        }
    }
}

fn spawn_helicopter(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    at: Vec3,
    ground: f32,
) {
    let mut mat = |r: f32, g: f32, b: f32| {
        materials.add(StandardMaterial {
            base_color: Color::srgb(r, g, b),
            perceptual_roughness: 0.7,
            ..default()
        })
    };
    let olive = mat(0.28, 0.33, 0.2);
    let dark = mat(0.12, 0.12, 0.12);
    let glass = mat(0.45, 0.65, 0.8);
    let red = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.1, 0.1),
        emissive: LinearRgba::rgb(6.0, 0.3, 0.3),
        ..default()
    });
    let mut cube = |size: Vec3| meshes.add(Cuboid::from_size(size));
    let part = |mesh: Handle<Mesh>, material: Handle<StandardMaterial>, at: Vec3| {
        (Mesh3d(mesh), MeshMaterial3d(material), Transform::from_translation(at))
    };
    // Fuselage le long de Z, cockpit vitré vers -Z, poutre de queue vers +Z.
    let body = cube(Vec3::new(2.4, 1.9, 4.4));
    let nose = cube(Vec3::new(2.0, 1.3, 1.2));
    let boom = cube(Vec3::new(0.45, 0.45, 4.6));
    let fin = cube(Vec3::new(0.15, 1.4, 0.8));
    let skid = cube(Vec3::new(0.15, 0.15, 4.8));
    let strut = cube(Vec3::new(0.12, 0.5, 0.12));
    let mast = cube(Vec3::new(0.25, 0.45, 0.25));
    let blade = cube(Vec3::new(0.35, 0.06, 11.0));
    let tail_blade = cube(Vec3::new(0.05, 1.6, 0.2));
    let light = cube(Vec3::splat(0.2));
    let rotor = commands
        .spawn((
            Rotor(14.0),
            Transform::from_xyz(0.0, 2.95, 0.0),
            Visibility::Inherited,
            children![
                part(blade.clone(), dark.clone(), Vec3::ZERO),
                (
                    Mesh3d(blade),
                    MeshMaterial3d(dark.clone()),
                    Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)),
                ),
            ],
        ))
        .id();
    let tail_rotor = commands
        .spawn((
            Rotor(-30.0),
            Transform::from_xyz(0.3, 1.9, 6.6).with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
            Visibility::Inherited,
            children![part(tail_blade, dark.clone(), Vec3::ZERO)],
        ))
        .id();
    commands
        .spawn((
            DespawnOnExit(AppState::Playing),
            Helicopter {
                ground,
                leaving: false,
                age: 0.0,
            },
            Transform::from_translation(Vec3::new(at.x, ground + APPROACH_HEIGHT, at.z)),
            Visibility::Inherited,
            children![
                part(body, olive.clone(), Vec3::new(0.0, 1.55, 0.0)),
                part(nose, glass, Vec3::new(0.0, 1.45, -2.7)),
                part(boom, olive.clone(), Vec3::new(0.0, 1.9, 4.4)),
                part(fin, olive.clone(), Vec3::new(0.0, 2.4, 6.6)),
                part(skid.clone(), dark.clone(), Vec3::new(-1.1, 0.08, -0.2)),
                part(skid, dark.clone(), Vec3::new(1.1, 0.08, -0.2)),
                part(strut.clone(), dark.clone(), Vec3::new(-1.0, 0.4, -1.3)),
                part(strut.clone(), dark.clone(), Vec3::new(1.0, 0.4, -1.3)),
                part(strut.clone(), dark.clone(), Vec3::new(-1.0, 0.4, 1.0)),
                part(strut, dark.clone(), Vec3::new(1.0, 0.4, 1.0)),
                part(mast, dark, Vec3::new(0.0, 2.7, 0.0)),
                part(light, red, Vec3::new(0.0, 2.6, 6.9)),
            ],
        ))
        .add_children(&[rotor, tail_rotor]);
}

fn fly_helicopter(
    mut commands: Commands,
    time: Res<Time>,
    mut helicopters: Query<(Entity, &mut Helicopter, &mut Transform), Without<Rotor>>,
    mut rotors: Query<(&Rotor, &mut Transform), Without<Helicopter>>,
) {
    let dt = time.delta_secs();
    for (entity, mut heli, mut transform) in &mut helicopters {
        heli.age += dt;
        if heli.leaving {
            transform.translation.y += CLIMB * dt;
            transform.translation.z -= (heli.age * 0.5).min(12.0) * dt;
            transform.rotation = Quat::from_rotation_x(-0.12);
            if transform.translation.y > heli.ground + 80.0 {
                commands.entity(entity).despawn();
            }
        } else {
            // Descente qui ralentit près du sol, léger balancement.
            let height = transform.translation.y - heli.ground;
            let speed = (height * 0.6).clamp(0.4, CLIMB);
            transform.translation.y = (transform.translation.y - speed * dt).max(heli.ground);
            let sway = (heli.age * 1.3).sin() * 0.03 * (height / 10.0).min(1.0);
            transform.rotation = Quat::from_rotation_z(sway);
        }
    }
    for (rotor, mut transform) in &mut rotors {
        let spin = Quat::from_rotation_y(rotor.0 * dt);
        transform.rotation *= spin;
    }
}

fn update_banner(
    time: Res<Time>,
    mut view: ResMut<RescueView>,
    player: Res<LocalPlayer>,
    mut text: Single<&mut Text, With<Banner>>,
) {
    let label = match view.state {
        Some(s) if s.landed.is_some() => {
            view.left = (view.left - time.delta_secs()).max(0.0);
            let p = pad(&s);
            let d = Vec2::new(player.position.x - p.x, player.position.z - p.z).length();
            let aboard = if d <= BOARD_RADIUS {
                "Tu es près de l'hélicoptère : reste là !".to_string()
            } else {
                format!("Rejoins-le : {d:.0} m (repère rose)")
            };
            format!("HÉLICOPTÈRE POSÉ · décollage dans {:.0} s\n{aboard}", view.left.ceil())
        }
        Some(s) => format!(
            "Appel de détresse : l'hélicoptère se posera près de la radio le jour {} au matin.\nTiens bon, le signal attire les morts.",
            s.day
        ),
        None => String::new(),
    };
    if text.0 != label {
        text.0 = label;
    }
}

fn spawn_victory(commands: &mut Commands, day: u32, player: &LocalPlayer) {
    let r = &player.record;
    let line = |value: String, size: f32, color: Color| {
        (
            Text::new(value),
            TextFont {
                font_size: FontSize::Px(size),
                ..default()
            },
            TextColor(color),
            TextShadow::default(),
            TextLayout::justify(Justify::Center),
        )
    };
    let stats = format!(
        "Zombies tués : {} · Nuits survécues : {} · Morts : {}\nObjets fabriqués : {} · Blocs posés : {} · Survivants recrutés : {}\nSuccès : {}/{}",
        r.total_kills(),
        r.nights,
        r.deaths,
        r.crafted,
        r.placed,
        r.recruited,
        r.unlocked.len() + 1,
        shared::achievements::Achievement::ALL.len()
    );
    commands.spawn((
        DespawnOnExit(AppState::Playing),
        Victory,
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            row_gap: px(14),
            ..default()
        },
        BackgroundColor(Color::srgba(0.02, 0.03, 0.02, 0.8)),
        GlobalZIndex(30),
        children![
            line("ÉVACUÉ !".into(), 64.0, Color::srgb(1.0, 0.82, 0.3)),
            line(
                format!("L'hélicoptère t'emporte loin des morts, au jour {day}. Tu as survécu."),
                22.0,
                Color::WHITE,
            ),
            line(stats, 17.0, Color::srgba(1.0, 1.0, 1.0, 0.85)),
            line(
                "Entrée : continuer à jouer dans ce monde".into(),
                16.0,
                Color::srgba(1.0, 1.0, 1.0, 0.6),
            ),
        ],
    ));
}

fn close_victory(mut commands: Commands, keys: Res<ButtonInput<KeyCode>>, screens: Query<Entity, With<Victory>>) {
    if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Escape) {
        for entity in &screens {
            commands.entity(entity).despawn();
        }
    }
}
