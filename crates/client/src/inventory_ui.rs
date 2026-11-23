//! Barre d'objets en bas de l'écran (icônes, quantités, case choisie) et grille d'inventaire
//! dans le panneau de fabrication : un clic sur un objet le range dans la case choisie.

use bevy::picking::events::{PointerDragDrop, PointerPress};
use bevy::picking::hover::Hovered;
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;
use bevy::ui_widgets::{Activate, Button, observe};
use shared::item::Item;

use crate::AppState;
use crate::icons::ItemIcons;
use crate::player::{HOTBAR_SLOTS, LocalPlayer};

pub struct InventoryUiPlugin;

impl Plugin for InventoryUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ItemIcons>()
            .add_systems(OnEnter(AppState::Playing), spawn_hotbar)
            .add_systems(OnExit(AppState::Playing), despawn_hotbar)
            .add_systems(
                Update,
                (update_hotbar, update_grid, assign_with_keys, update_panel_hotbar).run_if(in_state(AppState::Playing)),
            );
    }
}

/// Cases de la grille d'inventaire.
pub const GRID_COLUMNS: usize = 8;
pub const GRID_SLOTS: usize = 40;
const SLOT: f32 = 48.0;
const ICON: f32 = 36.0;

const SLOT_BG: Color = Color::srgba(0.05, 0.05, 0.05, 0.65);
const SLOT_BORDER: Color = Color::srgba(0.35, 0.35, 0.35, 0.9);
const SELECTED_BORDER: Color = Color::srgb(1.0, 0.85, 0.3);
const HOVER_BG: Color = Color::srgba(0.25, 0.3, 0.22, 0.85);

#[derive(Component)]
struct HotbarRoot;

#[derive(Component)]
struct HotbarSlot(usize);

#[derive(Component)]
struct HotbarIcon(usize);

#[derive(Component)]
struct HotbarCount(usize);

/// Nom de l'objet en main, affiché un instant au-dessus de la barre quand il change.
#[derive(Component)]
struct HeldName;

#[derive(Component)]
pub struct GridSlot(pub usize);

#[derive(Component)]
struct GridIcon(usize);

#[derive(Component)]
struct GridCount(usize);

/// Nom de l'objet survolé dans la grille.
#[derive(Component)]
struct GridHint;

/// Case de la barre d'outils reproduite dans le panneau (clic : la choisir, clic droit : la vider).
#[derive(Component)]
struct PanelSlot(usize);

#[derive(Component)]
struct PanelIcon(usize);

#[derive(Component)]
struct PanelCount(usize);

fn small_text(value: &str, size: f32) -> impl Bundle {
    colored_text(value, size, Color::WHITE)
}

fn colored_text(value: &str, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(value),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
        TextShadow::default(),
    )
}

/// Une case avec son icône et sa quantité.
fn slot_children(commands: &mut Commands, slot: Entity, icon: impl Component, count: impl Component) {
    commands.spawn((
        Node {
            width: px(ICON),
            height: px(ICON),
            ..default()
        },
        ImageNode::default(),
        Visibility::Hidden,
        icon,
        Pickable::IGNORE,
        ChildOf(slot),
    ));
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            right: px(3),
            bottom: px(1),
            ..default()
        },
        small_text("", 14.0),
        count,
        Pickable::IGNORE,
        ChildOf(slot),
    ));
}

fn slot_node() -> Node {
    Node {
        width: px(SLOT),
        height: px(SLOT),
        border: UiRect::all(px(2)),
        align_items: AlignItems::Center,
        justify_content: JustifyContent::Center,
        ..default()
    }
}

fn spawn_hotbar(mut commands: Commands) {
    let root = commands
        .spawn((
            HotbarRoot,
            Node {
                position_type: PositionType::Absolute,
                bottom: px(12),
                width: percent(100),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(4),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .id();
    commands.spawn((colored_text("", 18.0, Color::NONE), HeldName, ChildOf(root)));
    let row = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: px(4),
                ..default()
            },
            Pickable::IGNORE,
            ChildOf(root),
        ))
        .id();
    for i in 0..HOTBAR_SLOTS {
        let slot = commands
            .spawn((
                slot_node(),
                BackgroundColor(SLOT_BG),
                BorderColor::all(SLOT_BORDER),
                HotbarSlot(i),
                Pickable::IGNORE,
                ChildOf(row),
            ))
            .id();
        commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(3),
                top: px(0),
                ..default()
            },
            colored_text(&format!("{}", i + 1), 11.0, Color::srgba(1.0, 1.0, 1.0, 0.6)),
            Pickable::IGNORE,
            ChildOf(slot),
        ));
        slot_children(&mut commands, slot, HotbarIcon(i), HotbarCount(i));
    }
}

fn despawn_hotbar(mut commands: Commands, roots: Query<Entity, With<HotbarRoot>>) {
    for root in &roots {
        commands.entity(root).despawn();
    }
}

fn count_label(n: u32) -> String {
    if n > 1 { n.to_string() } else { String::new() }
}

#[allow(clippy::type_complexity)]
fn update_hotbar(
    player: Res<LocalPlayer>,
    time: Res<Time>,
    mut icons: ResMut<ItemIcons>,
    mut images: ResMut<Assets<Image>>,
    mut slots: Query<(&HotbarSlot, &mut BorderColor)>,
    mut icon_nodes: Query<(&HotbarIcon, &mut ImageNode, &mut Visibility)>,
    mut counts: Query<(&HotbarCount, &mut Text), Without<HeldName>>,
    mut name: Single<(&mut Text, &mut TextColor, &mut TextShadow), With<HeldName>>,
    mut last: Local<(Option<Item>, f32)>,
) {
    for (slot, mut border) in &mut slots {
        *border = BorderColor::all(if slot.0 == player.selected {
            SELECTED_BORDER
        } else {
            SLOT_BORDER
        });
    }
    for (icon, mut image, mut visibility) in &mut icon_nodes {
        match player.hotbar[icon.0].filter(|i| player.inventory.has(*i, 1)) {
            Some(item) => {
                let handle = icons.get(&mut images, item);
                if image.image != handle {
                    image.image = handle;
                }
                *visibility = Visibility::Inherited;
            }
            None => *visibility = Visibility::Hidden,
        }
    }
    for (count, mut text) in &mut counts {
        let label = player.hotbar[count.0].map_or(String::new(), |i| count_label(player.inventory.count(i)));
        if text.0 != label {
            text.0 = label;
        }
    }
    // Le nom de l'objet en main s'affiche deux secondes, puis s'efface.
    let held = player.held();
    if held != last.0 {
        *last = (held, 2.0);
        name.0.0 = held.map(|i| i.name().to_string()).unwrap_or_default();
    }
    last.1 -= time.delta_secs();
    let alpha = last.1.clamp(0.0, 1.0);
    name.1.0 = Color::srgba(1.0, 1.0, 1.0, alpha);
    name.2.color = Color::srgba(0.0, 0.0, 0.0, 0.75 * alpha);
}

/// Grille d'inventaire à placer dans un panneau : renvoie son entité.
pub fn spawn_grid(commands: &mut Commands, parent: Entity) {
    let grid = commands
        .spawn((
            Node {
                display: Display::Grid,
                grid_template_columns: RepeatedGridTrack::px(GRID_COLUMNS as u16, SLOT),
                column_gap: px(4),
                row_gap: px(4),
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    for i in 0..GRID_SLOTS {
        let slot = commands
            .spawn((
                slot_node(),
                Button,
                Hovered::default(),
                BackgroundColor(SLOT_BG),
                BorderColor::all(SLOT_BORDER),
                GridSlot(i),
                ChildOf(grid),
                observe(move |_: On<Activate>, mut player: ResMut<LocalPlayer>| {
                    // Range l'objet cliqué dans la case choisie de la barre.
                    let Some((item, _)) = player.inventory.iter().nth(i) else {
                        return;
                    };
                    let selected = player.selected;
                    player.assign_hotbar(selected, item);
                }),
            ))
            .id();
        slot_children(commands, slot, GridIcon(i), GridCount(i));
    }
    commands.spawn((
        Node {
            min_height: px(40),
            max_width: px(GRID_COLUMNS as f32 * (SLOT + 4.0)),
            ..default()
        },
        colored_text("", 14.0, Color::srgb(0.85, 0.85, 0.75)),
        GridHint,
        ChildOf(parent),
    ));
    commands.spawn((
        colored_text(
            "BARRE D'OUTILS · glisser un objet dessus pour l'y ranger · clic droit : vider",
            13.0,
            Color::srgb(0.6, 0.6, 0.6),
        ),
        ChildOf(parent),
    ));
    let row = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: px(4),
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    for i in 0..HOTBAR_SLOTS {
        let slot = commands
            .spawn((
                slot_node(),
                Hovered::default(),
                BackgroundColor(SLOT_BG),
                BorderColor::all(SLOT_BORDER),
                PanelSlot(i),
                ChildOf(row),
                observe(
                    move |press: On<PointerPress>, mut player: ResMut<LocalPlayer>| match press.button {
                        PointerButton::Secondary => player.hotbar[i] = None,
                        _ => player.selected = i,
                    },
                ),
            ))
            .id();
        // Glisser un objet de l'inventaire sur une case de la barre l'y range.
        commands.entity(slot).observe(
            move |drop: On<PointerDragDrop>, grid: Query<&GridSlot>, mut player: ResMut<LocalPlayer>| {
                let Ok(from) = grid.get(drop.dropped) else { return };
                let item = player.inventory.iter().nth(from.0).map(|(item, _)| item);
                if let Some(item) = item {
                    player.assign_hotbar(i, item);
                    player.selected = i;
                }
            },
        );
        commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(3),
                top: px(0),
                ..default()
            },
            colored_text(&format!("{}", i + 1), 11.0, Color::srgba(1.0, 1.0, 1.0, 0.6)),
            Pickable::IGNORE,
            ChildOf(slot),
        ));
        slot_children(commands, slot, PanelIcon(i), PanelCount(i));
    }
}

/// Survoler un objet de l'inventaire et appuyer sur 1 à 9 le range dans cette case de la barre.
fn assign_with_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut player: ResMut<LocalPlayer>,
    slots: Query<(&GridSlot, &Hovered)>,
) {
    if !player.crafting_open || player.typing {
        return;
    }
    let Some(index) = slots.iter().find(|(_, h)| h.get()).map(|(s, _)| s.0) else {
        return;
    };
    let Some((item, _)) = player.inventory.iter().nth(index) else {
        return;
    };
    for (slot, key) in [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
    ]
    .into_iter()
    .enumerate()
    {
        if keys.just_pressed(key) {
            player.assign_hotbar(slot, item);
        }
    }
}

#[allow(clippy::type_complexity)]
fn update_panel_hotbar(
    player: Res<LocalPlayer>,
    mut icons: ResMut<ItemIcons>,
    mut images: ResMut<Assets<Image>>,
    mut slots: Query<(&PanelSlot, &Hovered, &mut BackgroundColor, &mut BorderColor)>,
    mut icon_nodes: Query<(&PanelIcon, &mut ImageNode, &mut Visibility)>,
    mut counts: Query<(&PanelCount, &mut Text)>,
) {
    if !player.crafting_open {
        return;
    }
    for (slot, hovered, mut bg, mut border) in &mut slots {
        bg.0 = if hovered.get() { HOVER_BG } else { SLOT_BG };
        *border = BorderColor::all(if slot.0 == player.selected {
            SELECTED_BORDER
        } else {
            SLOT_BORDER
        });
    }
    for (icon, mut image, mut visibility) in &mut icon_nodes {
        match player.hotbar[icon.0].filter(|i| player.inventory.has(*i, 1)) {
            Some(item) => {
                let handle = icons.get(&mut images, item);
                if image.image != handle {
                    image.image = handle;
                }
                *visibility = Visibility::Inherited;
            }
            None => *visibility = Visibility::Hidden,
        }
    }
    for (count, mut text) in &mut counts {
        let label = player.hotbar[count.0].map_or(String::new(), |i| count_label(player.inventory.count(i)));
        if text.0 != label {
            text.0 = label;
        }
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn update_grid(
    player: Res<LocalPlayer>,
    mut icons: ResMut<ItemIcons>,
    mut images: ResMut<Assets<Image>>,
    mut slots: Query<(&GridSlot, &Hovered, &mut BackgroundColor, &mut BorderColor)>,
    mut icon_nodes: Query<(&GridIcon, &mut ImageNode, &mut Visibility)>,
    mut counts: Query<(&GridCount, &mut Text), Without<GridHint>>,
    mut hint: Query<&mut Text, With<GridHint>>,
) {
    if !player.crafting_open {
        return;
    }
    let items: Vec<(Item, u32)> = player.inventory.iter().collect();
    let held = player.held();
    let mut hovered_item = None;
    for (slot, hovered, mut bg, mut border) in &mut slots {
        let item = items.get(slot.0).map(|(i, _)| *i);
        if hovered.get() && item.is_some() {
            hovered_item = item;
        }
        bg.0 = if hovered.get() && item.is_some() {
            HOVER_BG
        } else {
            SLOT_BG
        };
        *border = BorderColor::all(if item.is_some() && item == held {
            SELECTED_BORDER
        } else {
            SLOT_BORDER
        });
    }
    for (icon, mut image, mut visibility) in &mut icon_nodes {
        match items.get(icon.0) {
            Some((item, _)) => {
                let handle = icons.get(&mut images, *item);
                if image.image != handle {
                    image.image = handle;
                }
                *visibility = Visibility::Inherited;
            }
            None => *visibility = Visibility::Hidden,
        }
    }
    for (count, mut text) in &mut counts {
        let label = items.get(count.0).map_or(String::new(), |(_, n)| count_label(*n));
        if text.0 != label {
            text.0 = label;
        }
    }
    for mut text in &mut hint {
        text.0 = match hovered_item {
            Some(item) => {
                let about = item.description();
                let about = if about.is_empty() {
                    String::new()
                } else {
                    format!("\n{about}")
                };
                format!(
                    "{} : clic → case {} · ou touche 1 à 9{about}",
                    item.name(),
                    player.selected + 1
                )
            }
            None => "Clic sur un objet : le ranger dans la case choisie (1 à 9)".into(),
        };
    }
}
