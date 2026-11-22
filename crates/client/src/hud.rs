//! Interface en jeu : viseur, bloc sélectionné, messages, contour du bloc visé.

use bevy::prelude::*;

use crate::AppState;
use crate::net::NetEvent;
use crate::player::{HOTBAR, LocalPlayer};
use crate::remote::RemotePlayers;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChatLog>()
            .add_systems(OnEnter(AppState::Playing), spawn_hud)
            .add_systems(OnExit(AppState::Playing), despawn_hud)
            .add_systems(
                Update,
                (collect_chat, update_hud, draw_target).run_if(in_state(AppState::Playing)),
            );
    }
}

#[derive(Component)]
struct HudRoot;

#[derive(Component)]
struct HotbarText;

#[derive(Component)]
struct ChatText;

/// Derniers messages affichés, avec leur heure d'arrivée.
#[derive(Resource, Default)]
struct ChatLog(Vec<(f32, String)>);

fn text(value: &str, size: f32) -> impl Bundle {
    (
        Text::new(value),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(Color::WHITE),
        TextShadow::default(),
    )
}

fn spawn_hud(mut commands: Commands, mut chat: ResMut<ChatLog>) {
    chat.0.clear();
    commands.spawn((
        HudRoot,
        Node {
            width: percent(100),
            height: percent(100),
            ..default()
        },
        Pickable::IGNORE,
        children![
            // Viseur
            (
                Node {
                    position_type: PositionType::Absolute,
                    left: percent(50),
                    top: percent(50),
                    margin: UiRect::new(px(-10), px(0), px(-14), px(0)),
                    ..default()
                },
                text("+", 28.0),
            ),
            (
                Node {
                    position_type: PositionType::Absolute,
                    left: px(12),
                    top: px(10),
                    ..default()
                },
                text("", 16.0),
                ChatText,
            ),
            (
                Node {
                    position_type: PositionType::Absolute,
                    bottom: px(16),
                    width: percent(100),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                children![(text("", 20.0), HotbarText)],
            ),
        ],
    ));
}

fn despawn_hud(mut commands: Commands, roots: Query<Entity, With<HudRoot>>) {
    for root in &roots {
        commands.entity(root).despawn();
    }
}

fn collect_chat(mut events: MessageReader<NetEvent>, mut chat: ResMut<ChatLog>, time: Res<Time>) {
    for event in events.read() {
        if let NetEvent::Chat(line) = event {
            chat.0.push((time.elapsed_secs(), line.clone()));
        }
    }
    let now = time.elapsed_secs();
    chat.0.retain(|(at, _)| now - at < 8.0);
}

fn update_hud(
    player: Res<LocalPlayer>,
    remotes: Res<RemotePlayers>,
    chat: Res<ChatLog>,
    mut hotbar: Single<&mut Text, (With<HotbarText>, Without<ChatText>)>,
    mut chat_text: Single<&mut Text, (With<ChatText>, Without<HotbarText>)>,
) {
    let slots: Vec<String> = HOTBAR
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let name = format!("{b:?}");
            if i == player.selected {
                format!("[{} {}]", i + 1, name)
            } else {
                format!(" {} {} ", i + 1, name)
            }
        })
        .collect();
    hotbar.0 = slots.join("  ");

    let mut lines = vec![format!("Joueurs en ligne : {}", remotes.names.len() + 1)];
    lines.extend(chat.0.iter().map(|(_, l)| l.clone()));
    chat_text.0 = lines.join("\n");
}

fn draw_target(player: Res<LocalPlayer>, mut gizmos: Gizmos) {
    if let Some(hit) = player.target {
        let center = hit.block.as_vec3() + Vec3::splat(0.5);
        gizmos.cube(
            Transform::from_translation(center).with_scale(Vec3::splat(1.003)),
            Color::srgba(0.0, 0.0, 0.0, 0.8),
        );
    }
}
