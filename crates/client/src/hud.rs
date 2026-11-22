//! Interface en jeu : viseur, bloc sélectionné, messages, contour du bloc visé.

use bevy::prelude::*;

use crate::AppState;
use crate::net::NetEvent;
use crate::player::{HOTBAR, LocalPlayer};
use crate::remote::RemotePlayers;
use crate::sky::GameClock;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChatLog>()
            .add_systems(OnEnter(AppState::Playing), spawn_hud)
            .add_systems(OnExit(AppState::Playing), despawn_hud)
            .add_systems(
                Update,
                (collect_chat, update_hud, update_status, draw_target).run_if(in_state(AppState::Playing)),
            );
    }
}

#[derive(Component)]
struct HudRoot;

#[derive(Component)]
struct HotbarText;

#[derive(Component)]
struct ChatText;

#[derive(Component)]
struct ClockText;

#[derive(Component)]
struct HealthFill;

#[derive(Component)]
struct HealthText;

#[derive(Component)]
struct HurtOverlay;

#[derive(Component)]
struct DeathScreen;

#[derive(Component)]
struct DeathText;

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
            // Heure et nombre de zombies, en haut à droite.
            (
                Node {
                    position_type: PositionType::Absolute,
                    right: px(12),
                    top: px(10),
                    ..default()
                },
                text("", 18.0),
                TextLayout::justify(Justify::Right),
                ClockText,
            ),
            // Barre de vie, en bas à gauche.
            (
                Node {
                    position_type: PositionType::Absolute,
                    left: px(16),
                    bottom: px(52),
                    width: px(220),
                    height: px(20),
                    border: px(2).all(),
                    ..default()
                },
                BorderColor::from(Color::srgba(0.0, 0.0, 0.0, 0.8)),
                BackgroundColor(Color::srgba(0.1, 0.0, 0.0, 0.6)),
                children![
                    (
                        Node {
                            width: percent(100),
                            height: percent(100),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.75, 0.12, 0.1)),
                        HealthFill,
                    ),
                    (
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(8),
                            ..default()
                        },
                        text("100", 15.0),
                        HealthText,
                    ),
                ],
            ),
            // Voile rouge quand on est touché.
            (
                Node {
                    position_type: PositionType::Absolute,
                    width: percent(100),
                    height: percent(100),
                    ..default()
                },
                BackgroundColor(Color::NONE),
                HurtOverlay,
            ),
            // Écran de mort.
            (
                Node {
                    position_type: PositionType::Absolute,
                    width: percent(100),
                    height: percent(100),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    row_gap: px(16),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.25, 0.0, 0.0, 0.7)),
                Visibility::Hidden,
                DeathScreen,
                children![
                    text("VOUS ÊTES MORT", 52.0),
                    (text("", 22.0), DeathText),
                    text("Appuyez sur Entrée pour réapparaître", 20.0),
                ],
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
            let name = b.name();
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

fn format_secs(secs: f32) -> String {
    let s = secs.max(0.0) as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

fn update_status(
    clock: Res<GameClock>,
    player: Res<LocalPlayer>,
    mut clock_text: Single<&mut Text, (With<ClockText>, Without<HealthText>, Without<DeathText>)>,
    mut health_text: Single<&mut Text, (With<HealthText>, Without<ClockText>, Without<DeathText>)>,
    mut death_text: Single<&mut Text, (With<DeathText>, Without<ClockText>, Without<HealthText>)>,
    mut fill: Single<&mut Node, With<HealthFill>>,
    mut overlay: Single<&mut BackgroundColor, With<HurtOverlay>>,
    mut death: Single<&mut Visibility, With<DeathScreen>>,
) {
    let t = clock.time;
    clock_text.0 = if t.is_night() {
        let horde = if t.is_horde_night() { " · GRANDE HORDE" } else { "" };
        format!(
            "Nuit {}{horde}\nAube dans {}\nZombies : {}",
            t.day,
            format_secs(t.secs_until_dawn()),
            clock.zombies_alive
        )
    } else {
        let next = if (t.day).is_multiple_of(shared::daytime::HORDE_EVERY) {
            "\nCette nuit : grande horde !"
        } else {
            ""
        };
        format!("Jour {}\nNuit dans {}{next}", t.day, format_secs(t.secs_until_night()))
    };
    health_text.0 = format!("{:.0}", player.health.ceil());
    fill.width = percent(player.health.clamp(0.0, 100.0));
    overlay.0 = Color::srgba(0.8, 0.0, 0.0, player.hurt_flash * 0.35);
    match &player.dead {
        Some(cause) => {
            **death = Visibility::Visible;
            death_text.0 = cause.clone();
        }
        None => **death = Visibility::Hidden,
    }
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
