//! Menu principal : pseudo, adresse du serveur, solo ou rejoindre.

use bevy::input_focus::InputFocus;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::text::EditableText;
use bevy::ui_widgets::{Activate, Button, TextInput, observe};

use crate::{AppState, LaunchOptions, MenuNotice};

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Menu), (spawn_menu, auto_start).chain())
            .add_systems(OnExit(AppState::Menu), despawn_menu)
            .add_systems(Update, button_colors.run_if(in_state(AppState::Menu)));
    }
}

#[derive(Component)]
struct MenuRoot;

#[derive(Component)]
struct NameInput;

#[derive(Component)]
struct AddressInput;

#[derive(Component)]
struct MenuButton;

const BUTTON_COLOR: Color = Color::srgb(0.18, 0.22, 0.18);
const BUTTON_HOVER: Color = Color::srgb(0.28, 0.36, 0.26);
const PANEL_COLOR: Color = Color::srgba(0.05, 0.06, 0.05, 0.85);

/// Lance directement la partie si la ligne de commande le demande (une seule fois).
fn auto_start(mut options: ResMut<LaunchOptions>, mut next: ResMut<NextState<AppState>>) {
    if options.auto_start {
        options.auto_start = false;
        next.set(AppState::Connecting);
    }
}

fn label(text: &str, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(text),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
    )
}

fn input(value: &str) -> impl Bundle {
    (
        Node {
            width: px(320),
            padding: px(8).all(),
            border: px(2).all(),
            ..default()
        },
        BorderColor::from(Color::srgb(0.4, 0.45, 0.4)),
        BackgroundColor(Color::srgb(0.1, 0.1, 0.1)),
        TextInput,
        EditableText::new(value),
        TextLayout::no_wrap(),
        TextFont {
            font_size: FontSize::Px(20.0),
            ..default()
        },
    )
}

fn button(text: &str) -> impl Bundle {
    (
        Button,
        MenuButton,
        Hovered::default(),
        Node {
            width: px(320),
            padding: px(12).all(),
            justify_content: JustifyContent::Center,
            ..default()
        },
        BackgroundColor(BUTTON_COLOR),
        children![label(text, 22.0, Color::WHITE)],
    )
}

fn spawn_menu(mut commands: Commands, options: Res<LaunchOptions>, mut notice: ResMut<MenuNotice>) {
    let notice_text = notice.0.take().unwrap_or_default();
    commands.spawn((
        MenuRoot,
        Node {
            width: percent(100),
            height: percent(100),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        children![(
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(12),
                padding: px(32).all(),
                ..default()
            },
            BackgroundColor(PANEL_COLOR),
            children![
                label("ZOMBIE SURVIVORS", 44.0, Color::srgb(0.75, 0.9, 0.55)),
                label("Survivre ensemble. Construire. Tenir la nuit.", 18.0, Color::srgb(0.7, 0.7, 0.7)),
                label("Pseudo", 18.0, Color::WHITE),
                (input(&options.name), NameInput),
                (
                    button("Jouer en solo / héberger"),
                    observe(|_: On<Activate>, mut commands: Commands| {
                        commands.run_system_cached_with(start_game, true);
                    }),
                ),
                label("Adresse du serveur", 18.0, Color::WHITE),
                (input(&options.address), AddressInput),
                (
                    button("Rejoindre"),
                    observe(|_: On<Activate>, mut commands: Commands| {
                        commands.run_system_cached_with(start_game, false);
                    }),
                ),
                label(&notice_text, 18.0, Color::srgb(1.0, 0.55, 0.45)),
                label(
                    "ZQSD/WASD : se déplacer · Espace : sauter · Clic gauche : frapper / miner · Clic droit : poser / manger · 1-9, molette : objet · C : fabriquer",
                    14.0,
                    Color::srgb(0.6, 0.6, 0.6)
                ),
            ],
        )],
    ));
}

fn start_game(
    In(host_locally): In<bool>,
    names: Query<&EditableText, With<NameInput>>,
    addresses: Query<&EditableText, With<AddressInput>>,
    mut options: ResMut<LaunchOptions>,
    mut focus: ResMut<InputFocus>,
    mut next: ResMut<NextState<AppState>>,
) {
    if let Ok(name) = names.single() {
        let name = name.value().to_string();
        if !name.trim().is_empty() {
            options.name = name.trim().to_string();
        }
    }
    if let Ok(address) = addresses.single() {
        options.address = address.value().to_string().trim().to_string();
    }
    options.host_locally = host_locally;
    focus.clear();
    next.set(AppState::Connecting);
}

fn button_colors(mut buttons: Query<(&Hovered, &mut BackgroundColor), With<MenuButton>>) {
    for (hovered, mut color) in &mut buttons {
        color.0 = if hovered.get() { BUTTON_HOVER } else { BUTTON_COLOR };
    }
}

fn despawn_menu(mut commands: Commands, roots: Query<Entity, With<MenuRoot>>) {
    for root in &roots {
        commands.entity(root).despawn();
    }
}
