//! Panneau d'un coffre ouvert : clic sur un objet du coffre pour le prendre,
//! sur un objet de l'inventaire pour le déposer (toute la pile, ou une seule unité avec Maj).

use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui_widgets::{Activate, Button, observe};
use bevy_renet::RenetClient;
use shared::item::{Inventory, Item};
use shared::protocol::ClientMessage;

use crate::AppState;
use crate::net::send_reliable;
use crate::player::LocalPlayer;

/// Nombre de cases affichées de chaque côté.
const SLOTS: usize = 30;

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

/// Case : côté coffre (`true`) ou inventaire, et rang dans la liste triée.
#[derive(Component, Clone, Copy)]
struct Slot {
    in_chest: bool,
    index: usize,
}

#[derive(Component)]
struct SlotLabel(Slot);

const SLOT_COLOR: Color = Color::srgb(0.16, 0.18, 0.16);
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
        commands.spawn((label(title, 22.0), ChildOf(column)));
        let grid = commands
            .spawn((
                Node {
                    width: px(420),
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
                    width: px(136),
                    height: px(26),
                    padding: UiRect::horizontal(px(6)),
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(SLOT_COLOR),
                Visibility::Hidden,
                ChildOf(grid),
                children![(label("", 14.0), SlotLabel(slot))],
                observe(on_slot_click(slot)),
            ));
        }
    }
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

fn update_panel(
    player: Res<LocalPlayer>,
    mut panel: Single<&mut Visibility, (With<ChestPanel>, Without<Slot>)>,
    mut slots: Query<(&Slot, &Hovered, &mut BackgroundColor, &mut Visibility), Without<ChestPanel>>,
    mut labels: Query<(&SlotLabel, &mut Text)>,
) {
    let Some((_, chest)) = &player.chest else {
        **panel = Visibility::Hidden;
        return;
    };
    **panel = Visibility::Visible;
    let source = |slot: &Slot| if slot.in_chest { chest } else { &player.inventory };
    for (slot, hovered, mut color, mut visibility) in &mut slots {
        let used = nth(source(slot), slot.index).is_some();
        *visibility = if used {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        color.0 = if hovered.get() { SLOT_HOVER } else { SLOT_COLOR };
    }
    for (SlotLabel(slot), mut text) in &mut labels {
        text.0 = nth(source(slot), slot.index)
            .map(|(item, n)| format!("{} ×{n}", item.name()))
            .unwrap_or_default();
    }
}
