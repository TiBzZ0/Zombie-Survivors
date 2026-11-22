//! Discussion : `T` ouvre la saisie, Entrée envoie, Échap annule.

use bevy::input_focus::{FocusCause, InputFocus};
use bevy::prelude::*;
use bevy::text::EditableText;
use bevy::ui_widgets::TextInput;
use bevy_renet::RenetClient;
use shared::protocol::ClientMessage;

use crate::AppState;
use crate::net::send_reliable;
use crate::player::LocalPlayer;

pub struct ChatPlugin;

impl Plugin for ChatPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Playing), spawn_input)
            .add_systems(OnExit(AppState::Playing), despawn_input)
            .add_systems(Update, chat_input.run_if(in_state(AppState::Playing)));
    }
}

#[derive(Component)]
struct ChatBox;

#[derive(Component)]
struct ChatField;

fn spawn_input(mut commands: Commands) {
    commands.spawn((
        ChatBox,
        Node {
            position_type: PositionType::Absolute,
            left: px(12),
            bottom: px(120),
            width: px(460),
            padding: px(6).all(),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
        Visibility::Hidden,
        children![(
            ChatField,
            Node {
                width: percent(100),
                ..default()
            },
            TextInput,
            EditableText::new(""),
            TextLayout::no_wrap(),
            TextFont {
                font_size: FontSize::Px(18.0),
                ..default()
            },
        )],
    ));
}

fn despawn_input(mut commands: Commands, boxes: Query<Entity, With<ChatBox>>) {
    for entity in &boxes {
        commands.entity(entity).despawn();
    }
}

fn chat_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut player: ResMut<LocalPlayer>,
    mut client: ResMut<RenetClient>,
    mut focus: ResMut<InputFocus>,
    mut chat_box: Single<&mut Visibility, With<ChatBox>>,
    mut field: Single<(Entity, &mut EditableText), With<ChatField>>,
) {
    if !player.typing {
        if keys.just_pressed(KeyCode::KeyT) && !player.panel_open() && player.dead.is_none() {
            player.typing = true;
            field.1.clear();
            focus.set(field.0, FocusCause::Navigated);
            **chat_box = Visibility::Visible;
        }
        return;
    }
    // La touche T qui a ouvert la saisie ne doit pas s'y écrire.
    if keys.just_pressed(KeyCode::KeyT) && field.1.value() == "t" {
        field.1.clear();
    }
    let send = keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter);
    if send || keys.just_pressed(KeyCode::Escape) {
        let text = field.1.value().to_string();
        let text = text.trim();
        if send && !text.is_empty() {
            send_reliable(&mut client, &ClientMessage::Chat { text: text.to_string() });
        }
        field.1.clear();
        focus.clear();
        player.typing = false;
        **chat_box = Visibility::Hidden;
    }
}
