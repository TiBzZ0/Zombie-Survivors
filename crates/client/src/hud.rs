//! Interface en jeu : viseur, bloc sélectionné, messages, contour du bloc visé.

use bevy::prelude::*;

use crate::AppState;
use crate::net::NetEvent;
use crate::player::LocalPlayer;
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
struct HungerFill;

#[derive(Component)]
struct HungerText;

#[derive(Component)]
struct MiningBar;

#[derive(Component)]
struct MiningFill;

#[derive(Component)]
struct HintText;

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

/// Jauge horizontale avec un texte par-dessus (vie, faim).
fn gauge(
    bottom: f32,
    back: Color,
    fill: Color,
    fill_marker: impl Component,
    text_marker: impl Component,
) -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            left: px(16),
            bottom: px(bottom),
            width: px(220),
            height: px(20),
            border: px(2).all(),
            ..default()
        },
        BorderColor::from(Color::srgba(0.0, 0.0, 0.0, 0.8)),
        BackgroundColor(back),
        children![
            (
                Node {
                    width: percent(100),
                    height: percent(100),
                    ..default()
                },
                BackgroundColor(fill),
                fill_marker,
            ),
            (
                Node {
                    position_type: PositionType::Absolute,
                    left: px(8),
                    ..default()
                },
                text("100", 15.0),
                text_marker,
            ),
        ],
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
                Pickable::IGNORE,
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
            // Vie et faim, en bas à gauche.
            gauge(
                84.0,
                Color::srgba(0.1, 0.0, 0.0, 0.6),
                Color::srgb(0.75, 0.12, 0.1),
                HealthFill,
                HealthText
            ),
            gauge(
                58.0,
                Color::srgba(0.1, 0.06, 0.0, 0.6),
                Color::srgb(0.8, 0.55, 0.15),
                HungerFill,
                HungerText
            ),
            // Progression du minage, sous le viseur.
            (
                Node {
                    position_type: PositionType::Absolute,
                    left: percent(50),
                    top: percent(50),
                    width: px(80),
                    height: px(6),
                    margin: UiRect::new(px(-40), px(0), px(22), px(0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
                Visibility::Hidden,
                MiningBar,
                children![(
                    Node {
                        width: percent(0),
                        height: percent(100),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.9, 0.9, 0.9)),
                    MiningFill,
                )],
            ),
            (
                Node {
                    position_type: PositionType::Absolute,
                    top: percent(56),
                    width: percent(100),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                Pickable::IGNORE,
                children![(text("", 16.0), HintText)],
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
                Pickable::IGNORE,
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
    let slots: Vec<String> = player
        .hotbar
        .iter()
        .enumerate()
        .map(|(i, slot)| {
            let label = match slot {
                Some(item) => format!("{} {} ×{}", i + 1, item.name(), player.inventory.count(*item)),
                None => format!("{}", i + 1),
            };
            if i == player.selected {
                format!("[{label}]")
            } else {
                format!(" {label} ")
            }
        })
        .collect();
    hotbar.0 = slots.join(" ");

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
    world: Option<Res<crate::net::ClientWorld>>,
    player: Res<LocalPlayer>,
    mut clock_text: Single<
        &mut Text,
        (
            With<ClockText>,
            Without<HealthText>,
            Without<DeathText>,
            Without<HungerText>,
            Without<HintText>,
        ),
    >,
    mut health_text: Single<
        &mut Text,
        (
            With<HealthText>,
            Without<ClockText>,
            Without<DeathText>,
            Without<HungerText>,
            Without<HintText>,
        ),
    >,
    mut death_text: Single<
        &mut Text,
        (
            With<DeathText>,
            Without<ClockText>,
            Without<HealthText>,
            Without<HungerText>,
            Without<HintText>,
        ),
    >,
    mut hunger_text: Single<
        &mut Text,
        (
            With<HungerText>,
            Without<ClockText>,
            Without<HealthText>,
            Without<DeathText>,
            Without<HintText>,
        ),
    >,
    mut hint_text: Single<
        &mut Text,
        (
            With<HintText>,
            Without<ClockText>,
            Without<HealthText>,
            Without<DeathText>,
            Without<HungerText>,
        ),
    >,
    mut fill: Single<&mut Node, (With<HealthFill>, Without<HungerFill>, Without<MiningFill>)>,
    mut hunger_fill: Single<&mut Node, (With<HungerFill>, Without<HealthFill>, Without<MiningFill>)>,
    mut mining_fill: Single<&mut Node, (With<MiningFill>, Without<HealthFill>, Without<HungerFill>)>,
    mut mining_bar: Single<&mut Visibility, (With<MiningBar>, Without<DeathScreen>)>,
    mut overlay: Single<&mut BackgroundColor, With<HurtOverlay>>,
    mut death: Single<&mut Visibility, (With<DeathScreen>, Without<MiningBar>)>,
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
    hunger_text.0 = if player.hunger <= 0.0 {
        "Affamé !".into()
    } else {
        format!("Faim {:.0}", player.hunger.ceil())
    };
    hunger_fill.width = percent(player.hunger.clamp(0.0, 100.0));
    match player.mining {
        Some((_, progress)) => {
            **mining_bar = Visibility::Visible;
            mining_fill.width = percent(progress.clamp(0.0, 1.0) * 100.0);
        }
        None => **mining_bar = Visibility::Hidden,
    }
    let aimed_deposit = player.target.zip(world.as_deref()).and_then(|(hit, world)| {
        let block = world.block(hit.block);
        block.is_deposit().then(|| {
            let left = player
                .deposits
                .get(&hit.block)
                .copied()
                .unwrap_or(shared::block::DEPOSIT_HARVESTS);
            (block, left)
        })
    });
    hint_text.0 = if player.needs_pickaxe {
        match aimed_deposit {
            Some((block, _)) => format!("{} : il faut une pioche", block.name()),
            None => "Il faut une pioche".into(),
        }
    } else if let Some((block, left)) = aimed_deposit {
        format!("{} : {left} récoltes restantes", block.name())
    } else if player.inventory.is_empty() {
        String::new()
    } else if let Some((item, weapon)) = player.held().and_then(|i| i.ranged().map(|w| (i, w))) {
        let ammo = player.inventory.count(weapon.ammo);
        if ammo == 0 {
            format!("{} : plus de munitions ({})", item.name(), weapon.ammo.name())
        } else {
            format!("{} : {ammo} × {}", item.name(), weapon.ammo.name())
        }
    } else if player.held().is_some_and(|i| i.food().is_some()) {
        "Clic droit : manger".into()
    } else {
        String::new()
    };
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
