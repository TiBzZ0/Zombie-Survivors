//! Carnet de survie (touche J) : statistiques du joueur et succès, débloqués ou en cours.
//! Un succès débloqué s'annonce par une bannière dorée et un petit carillon.

use bevy::prelude::*;
use shared::achievements::{Achievement, Record};
use shared::zombie::ZombieKind;

use crate::AppState;
use crate::net::NetEvent;
use crate::player::LocalPlayer;

const GOLD: Color = Color::srgb(1.0, 0.82, 0.3);
const DIM: Color = Color::srgb(0.55, 0.55, 0.55);
const CARD_BG: Color = Color::srgba(0.1, 0.1, 0.1, 0.95);
const DONE_BG: Color = Color::srgba(0.22, 0.17, 0.06, 0.95);

pub struct JournalPlugin;

impl Plugin for JournalPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Playing), spawn_journal)
            .add_systems(Update, (announce, update_journal).run_if(in_state(AppState::Playing)));
    }
}

#[derive(Component)]
struct JournalPanel;

#[derive(Component)]
struct StatsText;

#[derive(Component)]
struct JournalHeader;

#[derive(Component)]
struct Card(Achievement);

/// Texte d'avancement d'une carte.
#[derive(Component)]
struct CardProgress(Achievement);

/// Barre d'avancement d'une carte.
#[derive(Component)]
struct CardBar(Achievement);

fn label(value: &str, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(value),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
    )
}

fn spawn_journal(mut commands: Commands) {
    let panel = commands
        .spawn((
            DespawnOnExit(AppState::Playing),
            JournalPanel,
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
                padding: px(16).all(),
                max_height: vh(92),
                ..default()
            },
            BackgroundColor(Color::srgba(0.04, 0.04, 0.06, 0.97)),
            ChildOf(panel),
        ))
        .id();
    commands.spawn((
        Node {
            column_gap: px(16),
            align_items: AlignItems::Center,
            ..default()
        },
        ChildOf(content),
        children![
            label("CARNET DE SURVIE", 22.0, Color::WHITE),
            (label("", 15.0, GOLD), JournalHeader),
            label("J ou Échap : fermer", 13.0, DIM),
        ],
    ));
    let body = commands
        .spawn((
            Node {
                column_gap: px(18),
                ..default()
            },
            ChildOf(content),
        ))
        .id();
    commands.spawn((
        Node {
            width: px(230),
            flex_direction: FlexDirection::Column,
            padding: px(10).all(),
            ..default()
        },
        BackgroundColor(CARD_BG),
        ChildOf(body),
        children![
            label("STATISTIQUES", 15.0, GOLD),
            (
                label("", 14.0, Color::WHITE),
                StatsText,
                Node {
                    margin: UiRect::top(px(8)),
                    ..default()
                }
            ),
        ],
    ));
    let grid = commands
        .spawn((
            Node {
                width: px(3.0 * 196.0 + 2.0 * 6.0),
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(6),
                row_gap: px(6),
                align_content: AlignContent::FlexStart,
                ..default()
            },
            ChildOf(body),
        ))
        .id();
    for achievement in Achievement::ALL {
        let info = achievement.info();
        commands.spawn((
            Card(achievement),
            Node {
                width: px(196),
                flex_direction: FlexDirection::Column,
                padding: UiRect::axes(px(7), px(4)),
                border: px(2).all(),
                row_gap: px(1),
                ..default()
            },
            BackgroundColor(CARD_BG),
            BorderColor::all(Color::srgba(0.3, 0.3, 0.3, 0.9)),
            ChildOf(grid),
            children![
                label(info.title, 14.0, Color::WHITE),
                label(info.description, 11.0, DIM),
                (
                    Node {
                        column_gap: px(6),
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    children![
                        (
                            Node {
                                width: px(80),
                                height: px(5),
                                ..default()
                            },
                            BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.1)),
                            children![(
                                CardBar(achievement),
                                Node {
                                    width: percent(0),
                                    height: percent(100),
                                    ..default()
                                },
                                BackgroundColor(GOLD),
                            )],
                        ),
                        (label("", 11.0, DIM), CardProgress(achievement)),
                    ],
                ),
            ],
        ));
    }
}

/// Lignes de statistiques du carnet.
fn stats_text(r: &Record) -> String {
    let kinds = [
        (ZombieKind::Walker, "Rôdeurs"),
        (ZombieKind::Runner, "Coureurs"),
        (ZombieKind::Brute, "Brutes"),
        (ZombieKind::Spitter, "Cracheurs"),
        (ZombieKind::Armored, "Blindés"),
        (ZombieKind::Howler, "Hurleurs"),
        (ZombieKind::Bloater, "Gonflés"),
        (ZombieKind::Colossus, "Colosses"),
    ];
    let mut lines = vec![format!("Zombies tués : {}", r.total_kills())];
    for (kind, name) in kinds {
        let n = r.kills_of(kind);
        if n > 0 {
            lines.push(format!("   {name} : {n}"));
        }
    }
    lines.extend([
        format!("Dégâts infligés : {}", r.damage),
        String::new(),
        format!("Nuits survécues : {}", r.nights),
        format!("D'affilée : {}", r.streak),
        format!("Lunes de sang : {}", r.blood_moons),
        format!("Morts : {}", r.deaths),
        String::new(),
        format!("Objets fabriqués : {}", r.crafted),
        format!("Blocs posés : {}", r.placed),
        format!("Blocs cassés : {}", r.mined),
        format!("Poissons : {}", r.fish),
        format!("Gibier : {}", r.animals),
        format!("Survivants recrutés : {}", r.recruited),
    ]);
    lines.join("\n")
}

#[allow(clippy::type_complexity)]
fn update_journal(
    player: Res<LocalPlayer>,
    mut panel: Single<&mut Visibility, With<JournalPanel>>,
    mut cards: Query<(&Card, &mut BackgroundColor, &mut BorderColor)>,
    mut bars: Query<(&CardBar, &mut Node)>,
    mut texts: ParamSet<(
        Single<&mut Text, With<StatsText>>,
        Single<&mut Text, With<JournalHeader>>,
        Query<(&CardProgress, &mut Text, &mut TextColor)>,
    )>,
) {
    let open = player.journal_open && player.dead.is_none();
    let wanted = if open { Visibility::Visible } else { Visibility::Hidden };
    if **panel != wanted {
        **panel = wanted;
    }
    if !open {
        return;
    }
    let r = &player.record;
    let stats = stats_text(r);
    if texts.p0().0 != stats {
        texts.p0().0 = stats;
    }
    let header = format!("{}/{} succès", r.unlocked.len(), Achievement::ALL.len());
    if texts.p1().0 != header {
        texts.p1().0 = header;
    }
    for (card, mut bg, mut border) in &mut cards {
        let done = r.has(card.0);
        bg.0 = if done { DONE_BG } else { CARD_BG };
        *border = BorderColor::all(if done { GOLD } else { Color::srgba(0.3, 0.3, 0.3, 0.9) });
    }
    for (bar, mut node) in &mut bars {
        let (value, goal) = if r.has(bar.0) { (1, 1) } else { bar.0.progress(r) };
        node.width = percent(100.0 * value as f32 / goal.max(1) as f32);
    }
    for (p, mut text, mut color) in &mut texts.p2() {
        let info = p.0.info();
        let label = if r.has(p.0) {
            color.0 = GOLD;
            format!("Débloqué · +{} XP", info.xp)
        } else {
            color.0 = DIM;
            let (value, goal) = p.0.progress(r);
            format!("{value}/{goal} · +{} XP", info.xp)
        };
        if text.0 != label {
            text.0 = label;
        }
    }
}

/// Bannière dorée quand un succès tombe.
fn announce(mut commands: Commands, mut events: MessageReader<NetEvent>) {
    for event in events.read() {
        if let NetEvent::Achievement(achievement) = event {
            let info = achievement.info();
            crate::skills::popup(
                &mut commands,
                format!(
                    "SUCCÈS DÉBLOQUÉ : {}\n{} · +{} XP (J : carnet)",
                    info.title, info.description, info.xp
                ),
                24.0,
                GOLD,
                16.0,
                4.5,
            );
        }
    }
}
