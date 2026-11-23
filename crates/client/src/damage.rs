//! Chiffres de dégâts : à chaque coup, la vie perdue par le zombie s'élève au-dessus de lui
//! puis s'efface. Les gros coups (et le coup fatal) sont plus grands et dorés.

use bevy::prelude::*;

use crate::AppState;

/// Durée de vie d'un chiffre, en secondes.
const LIFETIME: f32 = 0.9;
/// Vitesse de montée, en blocs par seconde.
const RISE: f32 = 1.4;
/// À partir de ces dégâts, le chiffre est doré.
const BIG_HIT: f32 = 40.0;

/// Un zombie vient de perdre de la vie.
#[derive(Message, Clone, Copy)]
pub struct DamagePopup {
    pub pos: Vec3,
    pub amount: f32,
    /// Coup fatal.
    pub kill: bool,
}

pub struct DamagePlugin;

impl Plugin for DamagePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<DamagePopup>()
            .add_systems(
                Update,
                (spawn_popups, float_popups).chain().run_if(in_state(AppState::Playing)),
            )
            .add_systems(OnExit(AppState::Playing), clear_popups);
    }
}

#[derive(Component)]
struct Floating {
    pos: Vec3,
    age: f32,
    color: Color,
}

fn spawn_popups(mut commands: Commands, mut popups: MessageReader<DamagePopup>) {
    for popup in popups.read() {
        let amount = popup.amount.round();
        if amount < 1.0 {
            continue;
        }
        let big = popup.kill || amount >= BIG_HIT;
        let color = if popup.kill {
            Color::srgb(1.0, 0.35, 0.2)
        } else if big {
            Color::srgb(1.0, 0.85, 0.25)
        } else {
            Color::WHITE
        };
        // Un léger décalage pour que deux coups rapprochés ne se recouvrent pas.
        let jitter = Vec3::new(fastrand::f32() - 0.5, 0.0, fastrand::f32() - 0.5) * 0.5;
        commands.spawn((
            Floating {
                pos: popup.pos + jitter,
                age: 0.0,
                color,
            },
            Node {
                position_type: PositionType::Absolute,
                ..default()
            },
            Text::new(format!("{amount:.0}")),
            TextFont {
                font_size: FontSize::Px(if big { 26.0 } else { 19.0 }),
                ..default()
            },
            TextColor(color),
            TextShadow::default(),
            Visibility::Hidden,
        ));
    }
}

fn float_popups(
    mut commands: Commands,
    time: Res<Time>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut popups: Query<(Entity, &mut Floating, &mut Node, &mut TextColor, &mut Visibility)>,
) {
    let dt = time.delta_secs();
    let camera = camera.iter().find(|(c, _)| c.is_active);
    for (entity, mut popup, mut node, mut color, mut visibility) in &mut popups {
        popup.age += dt;
        if popup.age > LIFETIME {
            commands.entity(entity).despawn();
            continue;
        }
        let pos = popup.pos + Vec3::Y * RISE * popup.age;
        let screen = camera.and_then(|(c, t)| {
            // Derrière la caméra, pas de chiffre.
            let ahead = (pos - t.translation()).dot(*t.forward()) > 0.0;
            ahead.then(|| c.world_to_viewport(t, pos).ok()).flatten()
        });
        let Some(screen) = screen else {
            *visibility = Visibility::Hidden;
            continue;
        };
        *visibility = Visibility::Inherited;
        node.left = px(screen.x - 10.0);
        node.top = px(screen.y - 12.0);
        let fade = 1.0 - (popup.age / LIFETIME).powi(2);
        color.0 = popup.color.with_alpha(fade);
    }
}

fn clear_popups(mut commands: Commands, popups: Query<Entity, With<Floating>>) {
    for entity in &popups {
        commands.entity(entity).despawn();
    }
}
