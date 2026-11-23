//! Interface de la nage : bulles de souffle et voile d'eau quand la tête est sous la surface.

use bevy::prelude::*;
use shared::swim::{BREATH_SECS, Immersion};

use crate::AppState;
use crate::player::LocalPlayer;

pub struct SwimUiPlugin;

impl Plugin for SwimUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Playing), spawn_swim_ui)
            .add_systems(Update, update_swim_ui.run_if(in_state(AppState::Playing)));
    }
}

/// Bulles affichées pour un souffle plein.
const BUBBLES: usize = 10;

#[derive(Component)]
struct BreathRow;

#[derive(Component)]
struct Bubble(usize);

#[derive(Component)]
struct WaterVeil;

fn spawn_swim_ui(mut commands: Commands) {
    commands.spawn((
        DespawnOnExit(AppState::Playing),
        WaterVeil,
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            ..default()
        },
        BackgroundColor(Color::srgba(0.05, 0.3, 0.4, 0.3)),
        Visibility::Hidden,
        Pickable::IGNORE,
        GlobalZIndex(1),
    ));
    let row = commands
        .spawn((
            DespawnOnExit(AppState::Playing),
            BreathRow,
            Node {
                position_type: PositionType::Absolute,
                bottom: px(96),
                width: percent(100),
                justify_content: JustifyContent::Center,
                column_gap: px(3),
                ..default()
            },
            Visibility::Hidden,
            Pickable::IGNORE,
        ))
        .id();
    for i in 0..BUBBLES {
        commands.spawn((
            Bubble(i),
            Node {
                width: px(16),
                height: px(16),
                border: px(2).all(),
                border_radius: BorderRadius::MAX,
                ..default()
            },
            BorderColor::all(Color::srgba(0.85, 0.95, 1.0, 0.9)),
            BackgroundColor(Color::srgba(0.45, 0.75, 1.0, 0.6)),
            Pickable::IGNORE,
            ChildOf(row),
        ));
    }
}

fn update_swim_ui(
    player: Res<LocalPlayer>,
    mut veil: Single<&mut Visibility, (With<WaterVeil>, Without<BreathRow>)>,
    mut row: Single<&mut Visibility, (With<BreathRow>, Without<WaterVeil>)>,
    mut bubbles: Query<(&Bubble, &mut BackgroundColor, &mut BorderColor)>,
) {
    let under = player.immersion == Immersion::Under && player.dead.is_none();
    **veil = if under { Visibility::Visible } else { Visibility::Hidden };
    let capacity = BREATH_SECS * player.skills().breath;
    let part = (player.breath / capacity).clamp(0.0, 1.0);
    **row = if under || part < 0.999 {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    // Une bulle par dixième de souffle ; la dernière éclate en rouge quand on se noie.
    let full = part * BUBBLES as f32;
    for (bubble, mut bg, mut border) in &mut bubbles {
        let fill = (full - bubble.0 as f32).clamp(0.0, 1.0);
        let drowning = part <= 0.0;
        bg.0 = if drowning {
            Color::srgba(0.9, 0.2, 0.15, 0.5)
        } else {
            Color::srgba(0.45, 0.75, 1.0, 0.65 * fill)
        };
        *border = BorderColor::all(Color::srgba(0.85, 0.95, 1.0, 0.25 + 0.65 * fill));
    }
}
