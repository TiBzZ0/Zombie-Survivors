//! Panneau de fabrication (touche C ou Tab) : recettes et contenu de l'inventaire.

use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui_widgets::{Activate, Button, observe};
use bevy_renet::RenetClient;
use shared::crafting::RECIPES;
use shared::protocol::ClientMessage;

use crate::AppState;
use crate::net::send_reliable;
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
struct RecipeLabel(usize);

#[derive(Component)]
struct InventoryText;

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
    format!("{} ×{}  ←  {}", recipe.output.name(), recipe.count, inputs.join(" + "))
}

fn spawn_panel(mut commands: Commands) {
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
                row_gap: px(6),
                ..default()
            },
            ChildOf(content),
        ))
        .id();
    commands.spawn((label("FABRICATION", 24.0), ChildOf(recipes)));
    for (index, recipe) in RECIPES.iter().enumerate() {
        commands.spawn((
            Button,
            Hovered::default(),
            RecipeButton(index),
            Node {
                width: px(460),
                padding: UiRect::axes(px(10), px(6)),
                ..default()
            },
            BackgroundColor(UNAVAILABLE),
            ChildOf(recipes),
            children![(label(&recipe_text(index), 16.0), RecipeLabel(index))],
            observe(
                move |_: On<Activate>, mut client: ResMut<RenetClient>, player: Res<LocalPlayer>| {
                    debug!("Fabrication demandée : {}", recipe.output.name());
                    if recipe.can_craft(&player.inventory) {
                        send_reliable(&mut client, &ClientMessage::Craft { recipe: index as u16 });
                    }
                },
            ),
        ));
    }
    commands.spawn((
        colored_label("C / Tab : fermer · Clic : fabriquer", 14.0, Color::srgb(0.6, 0.6, 0.6)),
        ChildOf(recipes),
    ));

    commands.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(6),
            min_width: px(220),
            ..default()
        },
        ChildOf(content),
        children![label("INVENTAIRE", 24.0), (label("", 16.0), InventoryText)],
    ));
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
    mut labels: Query<(&RecipeLabel, &mut TextColor)>,
    mut inventory: Single<&mut Text, With<InventoryText>>,
) {
    **panel = if player.crafting_open && player.dead.is_none() {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    if !player.crafting_open {
        return;
    }
    for (button, hovered, mut color) in &mut buttons {
        let ok = RECIPES[button.0].can_craft(&player.inventory);
        color.0 = match (ok, hovered.get()) {
            (true, true) => AVAILABLE_HOVER,
            (true, false) => AVAILABLE,
            (false, _) => UNAVAILABLE,
        };
    }
    for (label, mut color) in &mut labels {
        let ok = RECIPES[label.0].can_craft(&player.inventory);
        color.0 = if ok { Color::WHITE } else { Color::srgb(0.5, 0.5, 0.5) };
    }
    inventory.0 = if player.inventory.is_empty() {
        "(vide)".into()
    } else {
        player
            .inventory
            .iter()
            .map(|(item, n)| format!("{} ×{n}", item.name()))
            .collect::<Vec<_>>()
            .join("\n")
    };
}
