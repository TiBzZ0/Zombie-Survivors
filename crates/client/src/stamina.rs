//! Jauge d'endurance : une fine barre au-dessus de la barre d'outils, visible quand elle n'est pas pleine.

use bevy::prelude::*;

use crate::AppState;
use crate::player::LocalPlayer;

pub struct StaminaUiPlugin;

impl Plugin for StaminaUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Playing), spawn_stamina_bar)
            .add_systems(Update, update_stamina_bar.run_if(in_state(AppState::Playing)));
    }
}

const WIDTH: f32 = 220.0;

#[derive(Component)]
struct StaminaRow;

#[derive(Component)]
struct StaminaFill;

fn spawn_stamina_bar(mut commands: Commands) {
    let row = commands
        .spawn((
            DespawnOnExit(AppState::Playing),
            StaminaRow,
            Node {
                position_type: PositionType::Absolute,
                bottom: px(86),
                width: percent(100),
                justify_content: JustifyContent::Center,
                ..default()
            },
            Visibility::Hidden,
            Pickable::IGNORE,
        ))
        .id();
    let track = commands
        .spawn((
            Node {
                width: px(WIDTH),
                height: px(6),
                border_radius: BorderRadius::MAX,
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
            Pickable::IGNORE,
            ChildOf(row),
        ))
        .id();
    commands.spawn((
        StaminaFill,
        Node {
            width: percent(100),
            height: percent(100),
            border_radius: BorderRadius::MAX,
            ..default()
        },
        BackgroundColor(Color::srgb(0.95, 0.8, 0.25)),
        Pickable::IGNORE,
        ChildOf(track),
    ));
}

fn update_stamina_bar(
    time: Res<Time>,
    player: Res<LocalPlayer>,
    mut row: Single<&mut Visibility, With<StaminaRow>>,
    mut fill: Single<(&mut Node, &mut BackgroundColor), With<StaminaFill>>,
) {
    let part = (player.stamina.value / player.max_stamina()).clamp(0.0, 1.0);
    **row = if part < 0.999 && player.dead.is_none() {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    fill.0.width = percent(part * 100.0);
    // À bout de souffle, la barre clignote en rouge jusqu'à ce qu'on ait récupéré.
    fill.1.0 = if player.stamina.exhausted {
        let blink = 0.6 + 0.4 * (time.elapsed_secs() * 8.0).sin().abs();
        Color::srgba(0.9, 0.25, 0.2, blink)
    } else {
        Color::srgb(0.95, 0.8, 0.25)
    };
}
