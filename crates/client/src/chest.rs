//! Panneau d'un coffre ouvert : clic sur un objet du coffre pour le prendre,
//! sur un objet de l'inventaire pour le déposer (toute la pile, ou une seule unité avec Maj).

use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui_widgets::{Activate, Button, observe};
use bevy_renet::RenetClient;
use shared::block::Block;
use shared::item::{Inventory, Item};
use shared::protocol::ClientMessage;

use crate::AppState;
use crate::icons::ItemIcons;
use crate::net::{ClientWorld, send_reliable};
use crate::player::LocalPlayer;

/// Nombre de cases affichées de chaque côté.
const SLOTS: usize = 32;

pub struct ChestPlugin;

impl Plugin for ChestPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Playing), spawn_panel)
            .add_systems(OnExit(AppState::Playing), despawn_panel)
            .add_systems(Update, update_panel.run_if(in_state(AppState::Playing)));
    }
}

#[derive(Component)]
struct ChestPanel;

/// Titre de la colonne du stockage : coffre ou tourelle.
#[derive(Component)]
struct StorageTitle;

/// Case : côté coffre (`true`) ou inventaire, et rang dans la liste triée.
#[derive(Component, Clone, Copy)]
struct Slot {
    in_chest: bool,
    index: usize,
}

#[derive(Component)]
struct SlotIcon(Slot);

#[derive(Component)]
struct SlotCount(Slot);

/// Nom de l'objet survolé.
#[derive(Component)]
struct ChestHint;

const SLOT_COLOR: Color = Color::srgba(0.05, 0.05, 0.05, 0.65);
const SLOT_HOVER: Color = Color::srgb(0.28, 0.36, 0.26);

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

fn nth(inventory: &Inventory, index: usize) -> Option<(Item, u32)> {
    inventory.iter().nth(index)
}

fn on_slot_click(
    slot: Slot,
) -> impl Fn(On<Activate>, ResMut<RenetClient>, Res<LocalPlayer>, Res<ButtonInput<KeyCode>>) {
    move |_, mut client, player, keys| {
        let Some((_, chest)) = &player.chest else { return };
        let source = if slot.in_chest { chest } else { &player.inventory };
        let Some((item, count)) = nth(source, slot.index) else {
            return;
        };
        let one = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
        let count = if one { 1 } else { count };
        let message = if slot.in_chest {
            ClientMessage::ChestTake { item, count }
        } else {
            ClientMessage::ChestPut { item, count }
        };
        send_reliable(&mut client, &message);
    }
}

fn spawn_panel(mut commands: Commands) {
    let panel = commands
        .spawn((
            ChestPanel,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            Visibility::Hidden,
            GlobalZIndex(11),
        ))
        .id();
    let content = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(10),
                padding: px(20).all(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.04, 0.05, 0.04, 0.92)),
            ChildOf(panel),
        ))
        .id();
    let columns = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: px(24),
                ..default()
            },
            ChildOf(content),
        ))
        .id();
    for (in_chest, title) in [(true, "COFFRE"), (false, "INVENTAIRE")] {
        let column = commands
            .spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(6),
                    ..default()
                },
                ChildOf(columns),
            ))
            .id();
        let title = commands.spawn((label(title, 22.0), ChildOf(column))).id();
        if in_chest {
            commands.entity(title).insert(StorageTitle);
        }
        let grid = commands
            .spawn((
                Node {
                    width: px(8.0 * 52.0),
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: px(4),
                    row_gap: px(4),
                    ..default()
                },
                ChildOf(column),
            ))
            .id();
        for index in 0..SLOTS {
            let slot = Slot { in_chest, index };
            commands.spawn((
                Button,
                Hovered::default(),
                slot,
                Node {
                    width: px(48),
                    height: px(48),
                    border: UiRect::all(px(2)),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                BackgroundColor(SLOT_COLOR),
                BorderColor::all(Color::srgba(0.35, 0.35, 0.35, 0.9)),
                ChildOf(grid),
                children![
                    (
                        Node {
                            width: px(36),
                            height: px(36),
                            ..default()
                        },
                        ImageNode::default(),
                        Visibility::Hidden,
                        SlotIcon(slot),
                        Pickable::IGNORE,
                    ),
                    (
                        Node {
                            position_type: PositionType::Absolute,
                            right: px(3),
                            bottom: px(1),
                            ..default()
                        },
                        label("", 13.0),
                        SlotCount(slot),
                        Pickable::IGNORE,
                    )
                ],
                observe(on_slot_click(slot)),
            ));
        }
    }
    commands.spawn((label("", 15.0), ChestHint, ChildOf(content)));
    commands.spawn((
        colored_label(
            "Clic : toute la pile · Maj + clic : une unité · Échap : fermer",
            14.0,
            Color::srgb(0.6, 0.6, 0.6),
        ),
        ChildOf(content),
    ));
}

fn despawn_panel(mut commands: Commands, panels: Query<Entity, With<ChestPanel>>) {
    for panel in &panels {
        commands.entity(panel).despawn();
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn update_panel(
    player: Res<LocalPlayer>,
    mut icons: ResMut<ItemIcons>,
    mut images: ResMut<Assets<Image>>,
    mut panel: Single<&mut Visibility, (With<ChestPanel>, Without<SlotIcon>)>,
    mut slots: Query<(&Slot, &Hovered, &mut BackgroundColor)>,
    mut icon_nodes: Query<(&SlotIcon, &mut ImageNode, &mut Visibility), Without<ChestPanel>>,
    mut counts: Query<(&SlotCount, &mut Text), (Without<StorageTitle>, Without<ChestHint>)>,
    mut title: Single<&mut Text, (With<StorageTitle>, Without<ChestHint>)>,
    mut hint: Single<&mut Text, With<ChestHint>>,
    world: Res<ClientWorld>,
) {
    let Some((pos, chest)) = &player.chest else {
        **panel = Visibility::Hidden;
        return;
    };
    **panel = Visibility::Visible;
    let heading = match world.block(*pos) {
        Block::Turret => "TOURELLE (flèches ou cartouches)",
        Block::Generator => "GÉNÉRATEUR (1 charbon = 2 minutes de courant, 12 blocs autour)",
        _ => "COFFRE",
    };
    if title.0 != heading {
        title.0 = heading.into();
    }
    let source = |slot: &Slot| if slot.in_chest { chest } else { &player.inventory };
    let mut hovered = None;
    for (slot, over, mut color) in &mut slots {
        let item = nth(source(slot), slot.index);
        if over.get() && item.is_some() {
            hovered = item.map(|(i, _)| (i, slot.in_chest));
        }
        color.0 = if over.get() && item.is_some() {
            SLOT_HOVER
        } else {
            SLOT_COLOR
        };
    }
    for (SlotIcon(slot), mut image, mut visibility) in &mut icon_nodes {
        match nth(source(slot), slot.index) {
            Some((item, _)) => {
                let handle = icons.get(&mut images, item);
                if image.image != handle {
                    image.image = handle;
                }
                *visibility = Visibility::Inherited;
            }
            None => *visibility = Visibility::Hidden,
        }
    }
    for (SlotCount(slot), mut text) in &mut counts {
        let label = nth(source(slot), slot.index)
            .filter(|(_, n)| *n > 1)
            .map(|(_, n)| n.to_string())
            .unwrap_or_default();
        if text.0 != label {
            text.0 = label;
        }
    }
    hint.0 = match hovered {
        Some((item, true)) => format!("{} : clic pour le prendre", item.name()),
        Some((item, false)) => format!("{} : clic pour le déposer", item.name()),
        None => String::new(),
    };
}
