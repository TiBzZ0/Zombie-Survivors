//! Panneau de fabrication (touche C ou Tab) : recettes et contenu de l'inventaire.

use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui_widgets::{Activate, Button, observe};
use bevy_renet::RenetClient;
use shared::block::Block;
use shared::crafting::{RECIPES, Recipe, station_near};
use shared::protocol::ClientMessage;

use crate::AppState;
use crate::icons::ItemIcons;
use crate::net::{ClientWorld, send_reliable};
use crate::player::LocalPlayer;

pub struct CraftingPlugin;

impl Plugin for CraftingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Playing), spawn_panel)
            .add_systems(OnExit(AppState::Playing), despawn_panel)
            .add_systems(Update, update_panel.run_if(in_state(AppState::Playing)));
    }
}

#[derive(Component)]
struct CraftingPanel;

#[derive(Component)]
struct RecipeButton(usize);

#[derive(Component)]
struct RecipeIcon(usize);

/// Détail de la recette survolée.
#[derive(Component)]
struct RecipeHint;

const AVAILABLE: Color = Color::srgb(0.18, 0.26, 0.18);
const AVAILABLE_HOVER: Color = Color::srgb(0.28, 0.40, 0.26);
const UNAVAILABLE: Color = Color::srgb(0.14, 0.14, 0.14);

fn label(value: &str, size: f32) -> impl Bundle {
    colored_label(value, size, Color::WHITE)
}

fn colored_label(value: &str, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(value),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
    )
}

fn recipe_text(index: usize) -> String {
    let recipe = &RECIPES[index];
    let inputs: Vec<String> = recipe
        .inputs
        .iter()
        .map(|(item, n)| format!("{n} {}", item.name()))
        .collect();
    let station = match recipe.station {
        Some(Block::Workbench) => "  [Établi]",
        Some(Block::Forge) => "  [Forge]",
        Some(_) => "  [Atelier]",
        None => "",
    };
    format!(
        "{} ×{}  ←  {}{station}",
        recipe.output.name(),
        recipe.count,
        inputs.join(" + ")
    )
}

/// Vrai si l'atelier demandé par la recette est à portée du joueur (le serveur vérifie aussi).
fn station_ok(recipe: &Recipe, player: &LocalPlayer, world: Option<&ClientWorld>) -> bool {
    let Some(station) = recipe.station else {
        return true;
    };
    let Some(world) = world else { return false };
    let feet = player.position.floor().as_ivec3();
    station_near(feet, station, |p| world.0.block(p))
}

fn craftable(recipe: &Recipe, player: &LocalPlayer, world: Option<&ClientWorld>) -> bool {
    recipe.can_craft(&player.inventory) && station_ok(recipe, player, world)
}

fn spawn_panel(mut commands: Commands, mut icons: ResMut<ItemIcons>, mut images: ResMut<Assets<Image>>) {
    let panel = commands
        .spawn((
            CraftingPanel,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            Visibility::Hidden,
            GlobalZIndex(10),
        ))
        .id();
    let content = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: px(24),
                padding: px(20).all(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.04, 0.05, 0.04, 0.92)),
            ChildOf(panel),
        ))
        .id();

    let recipes = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                width: px(10.0 * 52.0),
                ..default()
            },
            ChildOf(content),
        ))
        .id();
    commands.spawn((label("FABRICATION", 24.0), ChildOf(recipes)));
    let grid = commands
        .spawn((
            Node {
                display: Display::Grid,
                grid_template_columns: RepeatedGridTrack::px(10, 48.0),
                column_gap: px(4),
                row_gap: px(4),
                ..default()
            },
            ChildOf(recipes),
        ))
        .id();
    for (index, recipe) in RECIPES.iter().enumerate() {
        let count = if recipe.count > 1 {
            recipe.count.to_string()
        } else {
            String::new()
        };
        commands.spawn((
            Button,
            Hovered::default(),
            RecipeButton(index),
            Node {
                width: px(48),
                height: px(48),
                border: UiRect::all(px(2)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(UNAVAILABLE),
            BorderColor::all(Color::srgba(0.3, 0.3, 0.3, 0.9)),
            ChildOf(grid),
            children![
                (
                    Node {
                        width: px(36),
                        height: px(36),
                        ..default()
                    },
                    ImageNode::new(icons.get(&mut images, recipe.output)),
                    RecipeIcon(index),
                    Pickable::IGNORE,
                ),
                (
                    Node {
                        position_type: PositionType::Absolute,
                        right: px(3),
                        bottom: px(1),
                        ..default()
                    },
                    label(&count, 13.0),
                    Pickable::IGNORE,
                )
            ],
            observe(
                move |_: On<Activate>,
                      mut client: ResMut<RenetClient>,
                      player: Res<LocalPlayer>,
                      world: Option<Res<ClientWorld>>| {
                    debug!("Fabrication demandée : {}", recipe.output.name());
                    if craftable(recipe, &player, world.as_deref()) {
                        send_reliable(&mut client, &ClientMessage::Craft { recipe: index as u16 });
                    }
                },
            ),
        ));
    }
    commands.spawn((
        Node {
            min_height: px(44),
            ..default()
        },
        label("", 16.0),
        RecipeHint,
        ChildOf(recipes),
    ));
    commands.spawn((
        colored_label(
            "C / Tab : fermer · Clic : fabriquer · [Établi] / [Forge] : à moins de 4 blocs",
            14.0,
            Color::srgb(0.6, 0.6, 0.6),
        ),
        ChildOf(recipes),
    ));

    let inventory = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                ..default()
            },
            ChildOf(content),
            children![label("INVENTAIRE", 24.0)],
        ))
        .id();
    crate::inventory_ui::spawn_grid(&mut commands, inventory);
}

fn despawn_panel(mut commands: Commands, panels: Query<Entity, With<CraftingPanel>>) {
    for panel in &panels {
        commands.entity(panel).despawn();
    }
}

fn update_panel(
    player: Res<LocalPlayer>,
    mut panel: Single<&mut Visibility, With<CraftingPanel>>,
    mut buttons: Query<(&RecipeButton, &Hovered, &mut BackgroundColor)>,
    mut icons: Query<(&RecipeIcon, &mut ImageNode)>,
    mut hint: Single<(&mut Text, &mut TextColor), With<RecipeHint>>,
    world: Option<Res<ClientWorld>>,
) {
    **panel = if player.crafting_open && player.dead.is_none() {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    if !player.crafting_open {
        return;
    }
    let mut hovered = None;
    for (button, over, mut color) in &mut buttons {
        let ok = craftable(&RECIPES[button.0], &player, world.as_deref());
        if over.get() {
            hovered = Some((button.0, ok));
        }
        color.0 = match (ok, over.get()) {
            (true, true) => AVAILABLE_HOVER,
            (true, false) => AVAILABLE,
            (false, true) => Color::srgb(0.22, 0.22, 0.22),
            (false, false) => UNAVAILABLE,
        };
    }
    for (icon, mut image) in &mut icons {
        let ok = craftable(&RECIPES[icon.0], &player, world.as_deref());
        image.color = Color::srgba(1.0, 1.0, 1.0, if ok { 1.0 } else { 0.35 });
    }
    let (text, color) = &mut *hint;
    match hovered {
        Some((index, ok)) => {
            text.0 = recipe_text(index);
            color.0 = if ok { Color::WHITE } else { Color::srgb(0.6, 0.6, 0.6) };
        }
        None => {
            text.0 = "Survolez une recette pour voir ce qu'elle demande.".into();
            color.0 = Color::srgb(0.6, 0.6, 0.6);
        }
    }
}
