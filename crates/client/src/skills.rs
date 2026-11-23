//! Expérience et compétences : barre d'expérience, annonces de gain et de niveau,
//! et arbre de compétences (touche K) rangé par branche et par palier.

use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui_widgets::{Activate, Button, observe};
use bevy_renet::RenetClient;
use shared::protocol::ClientMessage;
use shared::skills::{BRANCHES, Branch, Progress, SKILLS, Skill, TIER_LEVEL, tier_name, xp_for_next};

use crate::AppState;
use crate::net::{NetEvent, send_reliable};
use crate::player::LocalPlayer;

pub struct SkillsPlugin;

impl Plugin for SkillsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HoveredSkill>()
            .add_systems(OnEnter(AppState::Playing), (spawn_xp_bar, spawn_tree))
            .add_systems(
                Update,
                (announce_progress, update_xp_bar, update_tree, fade_popups).run_if(in_state(AppState::Playing)),
            );
    }
}

/// Compétence survolée dans l'arbre, détaillée en bas du panneau.
#[derive(Resource, Default)]
struct HoveredSkill(Option<Skill>);

#[derive(Component)]
struct XpFill;

#[derive(Component)]
struct XpText;

/// Texte qui s'efface tout seul (gain d'expérience, nouveau niveau).
#[derive(Component)]
struct Popup {
    age: f32,
    life: f32,
    rise: f32,
}

#[derive(Component)]
struct TreePanel;

#[derive(Component)]
struct TreeHeader;

#[derive(Component)]
struct SkillCard(Skill);

#[derive(Component)]
struct SkillRank(Skill);

#[derive(Component)]
struct SkillDetail;

#[derive(Component)]
struct TierLabel(Branch, u8);

const CARD_BG: Color = Color::srgba(0.1, 0.1, 0.1, 0.95);
const DIM: Color = Color::srgb(0.55, 0.55, 0.55);
const GOOD: Color = Color::srgb(0.55, 0.9, 0.5);
const XP_COLOR: Color = Color::srgb(0.65, 0.45, 0.95);

fn label(value: &str, size: f32, color: Color) -> impl Bundle {
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

fn branch_color(branch: Branch, alpha: f32) -> Color {
    let [r, g, b] = branch.color();
    Color::srgba(r, g, b, alpha)
}

fn spawn_xp_bar(mut commands: Commands) {
    commands.spawn((
        DespawnOnExit(AppState::Playing),
        Node {
            position_type: PositionType::Absolute,
            left: px(16),
            bottom: px(10),
            width: px(220),
            height: px(16),
            border: px(2).all(),
            ..default()
        },
        BorderColor::from(Color::srgba(0.0, 0.0, 0.0, 0.8)),
        BackgroundColor(Color::srgba(0.06, 0.02, 0.1, 0.6)),
        Pickable::IGNORE,
        children![
            (
                Node {
                    width: percent(0),
                    height: percent(100),
                    ..default()
                },
                BackgroundColor(XP_COLOR),
                XpFill,
            ),
            (
                Node {
                    position_type: PositionType::Absolute,
                    left: px(8),
                    top: px(-1),
                    ..default()
                },
                label("Niv. 1", 12.0, Color::WHITE),
                XpText,
            ),
        ],
    ));
}

fn update_xp_bar(
    player: Res<LocalPlayer>,
    mut fill: Single<&mut Node, With<XpFill>>,
    mut text: Single<&mut Text, With<XpText>>,
) {
    if !player.is_changed() {
        return;
    }
    let progress = &player.progress;
    let (level, into) = progress.level();
    let next = xp_for_next(level);
    let part = if level >= shared::skills::MAX_LEVEL {
        1.0
    } else {
        into as f32 / next as f32
    };
    fill.width = percent(part * 100.0);
    let points = progress.points();
    text.0 = if points > 0 {
        format!("Niv. {level} · {into}/{next} XP · {points} point(s) : K")
    } else {
        format!("Niv. {level} · {into}/{next} XP")
    };
}

/// Annonce les gains d'expérience et les niveaux en comparant avec la progression précédente.
fn announce_progress(mut commands: Commands, mut events: MessageReader<NetEvent>, mut last: Local<Option<Progress>>) {
    for event in events.read() {
        let NetEvent::Progress(progress) = event else {
            continue;
        };
        if let Some(before) = last.as_ref() {
            let gained = progress.xp.saturating_sub(before.xp);
            if gained > 0 {
                popup(&mut commands, format!("+{gained} XP"), 18.0, XP_COLOR, 56.0, 1.4);
            }
            let (old, new) = (before.level().0, progress.level().0);
            if new > old {
                popup(
                    &mut commands,
                    format!("NIVEAU {new} !\nPoint de compétence à dépenser (touche K)"),
                    30.0,
                    Color::srgb(1.0, 0.85, 0.35),
                    28.0,
                    3.5,
                );
            }
        }
        *last = Some(progress.clone());
    }
}

pub(crate) fn popup(commands: &mut Commands, text: String, size: f32, color: Color, top: f32, life: f32) {
    commands.spawn((
        DespawnOnExit(AppState::Playing),
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            top: percent(top),
            justify_content: JustifyContent::Center,
            ..default()
        },
        Pickable::IGNORE,
        Popup {
            age: 0.0,
            life,
            rise: 0.0,
        },
        children![(label(&text, size, color), TextLayout::justify(Justify::Center))],
    ));
}

fn fade_popups(
    mut commands: Commands,
    time: Res<Time>,
    mut popups: Query<(Entity, &mut Popup, &mut Node, &Children)>,
    mut colors: Query<&mut TextColor>,
) {
    let dt = time.delta_secs();
    for (entity, mut popup, mut node, children) in &mut popups {
        popup.age += dt;
        if popup.age >= popup.life {
            commands.entity(entity).despawn();
            continue;
        }
        // Monte doucement puis s'efface sur la dernière demi-seconde.
        popup.rise += dt * 18.0;
        node.margin.top = px(-popup.rise);
        let alpha = ((popup.life - popup.age) / 0.5).clamp(0.0, 1.0);
        for child in children.iter() {
            if let Ok(mut color) = colors.get_mut(child) {
                color.0.set_alpha(alpha);
            }
        }
    }
}

fn spawn_tree(mut commands: Commands) {
    let panel = commands
        .spawn((
            DespawnOnExit(AppState::Playing),
            TreePanel,
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
                ..default()
            },
            BackgroundColor(Color::srgba(0.04, 0.04, 0.06, 0.97)),
            ChildOf(panel),
        ))
        .id();
    // En-tête : niveau, points, bouton de remise à zéro.
    commands.spawn((
        Node {
            column_gap: px(16),
            align_items: AlignItems::Center,
            ..default()
        },
        ChildOf(content),
        children![
            label("COMPÉTENCES", 22.0, Color::WHITE),
            (label("", 16.0, Color::WHITE), TreeHeader),
            (
                Button,
                Hovered::default(),
                Node {
                    padding: UiRect::axes(px(8), px(4)),
                    border: UiRect::all(px(2)),
                    margin: UiRect::left(px(20)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.2, 0.12, 0.12)),
                BorderColor::all(Color::srgb(0.5, 0.3, 0.3)),
                children![(label("Réinitialiser (gratuit)", 13.0, Color::WHITE), Pickable::IGNORE)],
                observe(|_: On<Activate>, mut client: ResMut<RenetClient>| {
                    send_reliable(&mut client, &ClientMessage::ResetSkills);
                }),
            ),
        ],
    ));
    let columns = commands
        .spawn((
            Node {
                column_gap: px(10),
                align_items: AlignItems::Start,
                ..default()
            },
            ChildOf(content),
        ))
        .id();
    for branch in BRANCHES {
        let column = commands
            .spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(4),
                    width: px(206),
                    ..default()
                },
                ChildOf(columns),
            ))
            .id();
        commands.spawn((
            label(&branch.name().to_uppercase(), 17.0, branch_color(branch, 1.0)),
            ChildOf(column),
        ));
        for tier in 1..=4u8 {
            let skills: Vec<Skill> = SKILLS
                .into_iter()
                .filter(|s| s.info().branch == branch && s.info().tier == tier)
                .collect();
            if skills.is_empty() {
                continue;
            }
            commands.spawn((
                label(
                    &format!("Palier {} · niveau {}", tier_name(tier), TIER_LEVEL[tier as usize - 1]),
                    12.0,
                    DIM,
                ),
                TierLabel(branch, tier),
                ChildOf(column),
            ));
            for skill in skills {
                spawn_card(&mut commands, column, skill);
            }
        }
    }
    commands.spawn((
        Node {
            min_height: px(54),
            width: px(5.0 * 206.0 + 40.0),
            ..default()
        },
        label("", 14.0, Color::srgb(0.85, 0.85, 0.75)),
        SkillDetail,
        ChildOf(content),
    ));
    commands.spawn((
        label(
            "K / Échap : fermer · Clic : apprendre un rang · Un point par niveau, gagné en tuant des zombies et en survivant aux nuits",
            12.0,
            DIM,
        ),
        ChildOf(content),
    ));
}

fn spawn_card(commands: &mut Commands, column: Entity, skill: Skill) {
    let info = skill.info();
    commands.spawn((
        Button,
        Hovered::default(),
        SkillCard(skill),
        Node {
            flex_direction: FlexDirection::Column,
            padding: UiRect::axes(px(6), px(3)),
            border: UiRect::all(px(2)),
            ..default()
        },
        BackgroundColor(CARD_BG),
        BorderColor::all(DIM),
        ChildOf(column),
        children![
            (
                Node {
                    justify_content: JustifyContent::SpaceBetween,
                    ..default()
                },
                Pickable::IGNORE,
                children![
                    (label(info.name, 14.0, Color::WHITE), Pickable::IGNORE),
                    (label("", 13.0, Color::WHITE), SkillRank(skill), Pickable::IGNORE),
                ],
            ),
            (label(info.effect, 11.0, DIM), Pickable::IGNORE),
        ],
        observe(
            move |_: On<Activate>, mut client: ResMut<RenetClient>, player: Res<LocalPlayer>| {
                if player.progress.can_learn(skill).is_ok() {
                    send_reliable(&mut client, &ClientMessage::LearnSkill { skill });
                }
            },
        ),
    ));
}

#[allow(clippy::type_complexity)]
fn update_tree(
    player: Res<LocalPlayer>,
    mut hovered: ResMut<HoveredSkill>,
    mut panel: Single<&mut Visibility, With<TreePanel>>,
    mut cards: Query<(&SkillCard, &Hovered, &mut BackgroundColor, &mut BorderColor)>,
    mut texts: ParamSet<(
        Single<&mut Text, With<TreeHeader>>,
        Single<&mut Text, With<SkillDetail>>,
        Query<(&SkillRank, &mut Text, &mut TextColor)>,
        Query<(&TierLabel, &mut TextColor)>,
    )>,
) {
    let open = player.skills_open && player.dead.is_none();
    **panel = if open { Visibility::Visible } else { Visibility::Hidden };
    if !open {
        return;
    }
    let progress = &player.progress;
    let (level, into) = progress.level();
    texts.p0().0 = format!(
        "Niveau {level} · {into}/{} XP · {} point(s) à dépenser",
        xp_for_next(level),
        progress.points()
    );
    hovered.0 = None;
    for (card, over, mut bg, mut border) in &mut cards {
        let skill = card.0;
        let branch = skill.info().branch;
        let rank = progress.rank(skill);
        let learnable = progress.can_learn(skill).is_ok();
        if over.get() {
            hovered.0 = Some(skill);
        }
        bg.0 = match (rank > 0, over.get()) {
            (true, true) => branch_color(branch, 0.4),
            (true, false) => branch_color(branch, 0.22),
            (false, true) => Color::srgba(0.2, 0.2, 0.2, 0.95),
            (false, false) => CARD_BG,
        };
        *border = BorderColor::all(if learnable {
            GOOD
        } else if rank > 0 {
            branch_color(branch, 1.0)
        } else {
            Color::srgba(0.3, 0.3, 0.3, 0.9)
        });
    }
    for (rank, mut text, mut color) in &mut texts.p2() {
        let info = rank.0.info();
        let r = progress.rank(rank.0);
        text.0 = format!("{r}/{}", info.max_rank);
        color.0 = if r >= info.max_rank {
            Color::srgb(1.0, 0.85, 0.35)
        } else if r > 0 {
            Color::WHITE
        } else {
            DIM
        };
    }
    for (tier, mut color) in &mut texts.p3() {
        color.0 = if level >= TIER_LEVEL[tier.1 as usize - 1] {
            branch_color(tier.0, 0.9)
        } else {
            DIM
        };
    }
    let detail = match hovered.0 {
        Some(skill) => {
            let info = skill.info();
            let requires: Vec<String> = info
                .requires
                .iter()
                .map(|(s, r)| format!("{} {r}", s.info().name))
                .collect();
            let requires = if requires.is_empty() {
                String::new()
            } else {
                format!(" · Demande : {}", requires.join(", "))
            };
            let state = match progress.can_learn(skill) {
                Ok(()) => "Clic : apprendre le rang suivant".to_string(),
                Err(locked) => locked.describe(),
            };
            format!(
                "{} ({} · palier {}) : {}{requires}\n{state}",
                info.name,
                info.branch.name(),
                tier_name(info.tier),
                info.effect
            )
        }
        None => "Survolez une compétence pour la détailler.".into(),
    };
    texts.p1().0 = detail;
}
