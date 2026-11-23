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
                (
                    collect_chat,
                    update_hud,
                    update_condition,
                    update_status,
                    update_thirst,
                    update_buffs,
                    update_buff_labels,
                    draw_target,
                )
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

#[derive(Component)]
struct HudRoot;

/// Rangée des pastilles d'effets temporaires.
#[derive(Component)]
struct BuffBar;

/// Une pastille déjà affichée (pour la rafraîchir).
#[derive(Component)]
struct BuffPip(shared::buff::Buff);

/// Température et infection, au-dessus de la barre de vie.
#[derive(Component)]
struct ConditionText;

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
struct ThirstFill;

#[derive(Component)]
struct ThirstText;

/// Jauge de soif entière (masquée dans les mondes sans lacs).
#[derive(Component)]
struct ThirstGauge;

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
            // Température et infection, au-dessus de la vie.
            (
                Node {
                    position_type: PositionType::Absolute,
                    left: px(16),
                    bottom: px(112),
                    ..default()
                },
                text("", 17.0),
                ConditionText,
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
            (
                gauge(
                    32.0,
                    Color::srgba(0.0, 0.04, 0.1, 0.6),
                    Color::srgb(0.2, 0.5, 0.9),
                    ThirstFill,
                    ThirstText
                ),
                ThirstGauge,
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
            // Effets temporaires (café, bière...), en bas à droite au-dessus de la barre.
            (
                Node {
                    position_type: PositionType::Absolute,
                    right: px(16),
                    bottom: px(84),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::FlexEnd,
                    row_gap: px(4),
                    ..default()
                },
                Pickable::IGNORE,
                BuffBar,
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

fn update_hud(remotes: Res<RemotePlayers>, chat: Res<ChatLog>, mut chat_text: Single<&mut Text, With<ChatText>>) {
    let mut lines = vec![format!("Joueurs en ligne : {}", remotes.names.len() + 1)];
    lines.extend(chat.0.iter().map(|(_, l)| l.clone()));
    chat_text.0 = lines.join("\n");
}

fn format_secs(secs: f32) -> String {
    let s = secs.max(0.0) as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

#[allow(clippy::too_many_arguments)]
fn update_status(
    session: Option<Res<crate::net::Session>>,
    claims: Option<Res<crate::net::Claims>>,
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
        let fog = if clock.fog {
            "\nBrouillard : les zombies ne brûlent pas"
        } else {
            ""
        };
        format!(
            "Jour {}\nNuit dans {}{next}{fog}",
            t.day,
            format_secs(t.secs_until_night())
        )
    };
    // JcJ : rappel permanent, et la base où l'on se trouve.
    if session.is_some_and(|s| s.pvp) {
        let mine = claims.as_ref().and_then(|c| c.owner_at(player.position));
        let place = match mine {
            Some(owner) => format!(" · base de {owner}"),
            None => String::new(),
        };
        clock_text.0.push_str(&format!("\nJcJ activé{place}"));
    }
    let armor = player.inventory.armor_reduction();
    health_text.0 = if armor > 0.0 {
        format!("{:.0} · armure {:.0} %", player.health.ceil(), armor * 100.0)
    } else {
        format!("{:.0}", player.health.ceil())
    };
    fill.width = percent(player.health.clamp(0.0, 100.0));
    let hunger = if player.hunger <= 0.0 {
        "Affamé !".to_string()
    } else {
        format!("Faim {:.0}", player.hunger.ceil())
    };
    hunger_text.0 = hunger;
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
    let aimed_block = player
        .target
        .zip(world.as_deref())
        .map(|(hit, world)| world.block(hit.block));
    let aimed_gun = player
        .target
        .zip(world.as_deref())
        .map(|(hit, world)| world.block(hit.block))
        .filter(|b| b.emplacement().is_some());
    let mounted_gun = player
        .mounted
        .zip(world.as_deref())
        .and_then(|(pos, world)| world.block(pos).emplacement().map(|g| (world.block(pos), g)));
    hint_text.0 = if let Some((block, gun)) = mounted_gun {
        format!(
            "{} : clic gauche pour tirer ({} : {}) · clic droit : descendre",
            block.name(),
            gun.ammo.name(),
            player.inventory.count(gun.ammo)
        )
    } else if let Some(hint) = &player.survivor_hint {
        hint.clone()
    } else if aimed_block == Some(shared::block::Block::RipeFruitLeaves) {
        "Pommier : clic droit pour cueillir les pommes".into()
    } else if aimed_block == Some(shared::block::Block::FallenFruit) {
        "Pomme tombée : clic gauche pour la ramasser avant qu'elle ne pourrisse".into()
    } else if aimed_block.is_some_and(|b| b.is_sapling()) {
        "Pousse d'arbre : elle grandira en quelques minutes".into()
    } else if let Some(block) = aimed_gun {
        let ammo = block.emplacement().map(|g| g.ammo.name()).unwrap_or_default();
        format!(
            "{} : clic droit pour prendre les commandes (munitions : {ammo})",
            block.name()
        )
    } else if player.needs_pickaxe {
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
    } else if player.held().is_some_and(|i| i.drink().is_some()) {
        "Clic droit : boire".into()
    } else if player.held() == Some(shared::item::Item::Antidote) {
        "Clic droit : prendre l'antidote".into()
    } else if player.aimed_water.is_some() && player.held() == Some(shared::item::Item::Bucket) {
        "Clic droit : remplir le seau".into()
    } else if player.held() == Some(shared::item::Item::WaterBucket) && player.target.is_some() {
        "Clic droit : verser l'eau".into()
    } else if player.aimed_water.is_some() && player.held() == Some(shared::item::Item::Flask) {
        "Clic droit : remplir la gourde".into()
    } else if player.aimed_water.is_some() && !player.held().is_some_and(|i| matches!(i, shared::item::Item::Block(_)))
    {
        "Clic droit : boire".into()
    } else if player.swimming {
        "Vous nagez · Espace : remonter".into()
    } else if let Some(t) = player.held().and_then(|i| i.throwable()) {
        let blocks = if t.blast_blocks > 0.0 {
            ", détruit les blocs"
        } else {
            ""
        };
        format!("Clic gauche : lancer (explose après {:.0} s{blocks})", t.fuse)
    } else if player.held() == Some(shared::item::Item::Seeds) {
        "Clic droit sur la terre ou l'herbe : semer (le blé mûrit en quelques minutes)".into()
    } else if player.guarding {
        "Garde levée".into()
    } else if player.held().is_some_and(|i| i.is_melee_weapon()) {
        "Clic droit maintenu : garde (levée juste avant le coup, elle le pare)".into()
    } else {
        String::new()
    };
    overlay.0 = if player.parry_flash > player.hurt_flash {
        Color::srgba(0.85, 0.9, 1.0, player.parry_flash * 0.3)
    } else {
        Color::srgba(0.8, 0.0, 0.0, player.hurt_flash * 0.35)
    };
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

/// Température du corps (si elle s'écarte de la normale ou si l'air est rude) et infection.
fn update_condition(player: Res<LocalPlayer>, mut text: Single<(&mut Text, &mut TextColor), With<ConditionText>>) {
    use shared::climate::*;
    let mut parts: Vec<String> = Vec::new();
    let mut color = Color::WHITE;
    let feeling = feeling(player.felt);
    if feeling.is_some() || (player.body_temp - BODY_NORMAL).abs() >= 0.1 {
        let temp = format!("{:.1} °C", player.body_temp).replace('.', ",");
        let state = if player.body_temp < HYPOTHERMIA {
            color = Color::srgb(0.5, 0.75, 1.0);
            " · Hypothermie ! Un feu, un toit ou un manteau".to_string()
        } else if player.body_temp > HEATSTROKE {
            color = Color::srgb(1.0, 0.5, 0.3);
            " · Coup de chaleur ! De l'ombre ou un chapeau".to_string()
        } else {
            feeling.map(|f| format!(" · {f}")).unwrap_or_default()
        };
        parts.push(format!("{temp}{state}"));
    }
    if player.infection > 0.0 {
        if player.infection >= INFECTION_HARMFUL {
            color = Color::srgb(0.55, 0.9, 0.3);
        }
        parts.push(format!("Infecté {:.0} % · un antidote guérit", player.infection.ceil()));
    }
    let joined = parts.join("\n");
    if text.0.0 != joined {
        text.0.0 = joined;
    }
    if text.1.0 != color {
        text.1.0 = color;
    }
}

/// Affiche une pastille par effet actif, avec son nom et le temps restant.
fn update_buffs(
    mut commands: Commands,
    player: Res<LocalPlayer>,
    bar: Single<Entity, With<BuffBar>>,
    pips: Query<(Entity, &BuffPip)>,
) {
    use std::collections::HashSet;
    let active: HashSet<shared::buff::Buff> = player.buffs.0.iter().map(|(b, _)| *b).collect();
    // Retire les pastilles des effets expirés.
    for (entity, pip) in &pips {
        if !active.contains(&pip.0) {
            commands.entity(entity).despawn();
        }
    }
    // Ajoute celles qui manquent, dans l'ordre fixe.
    let shown: HashSet<shared::buff::Buff> = pips.iter().map(|(_, p)| p.0).collect();
    for buff in shared::buff::ALL_BUFFS {
        if active.contains(&buff) && !shown.contains(&buff) {
            let [r, g, b] = buff.color();
            commands.spawn((
                BuffPip(buff),
                Node {
                    padding: UiRect::axes(px(8), px(3)),
                    border: px(2).all(),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
                BorderColor::from(Color::srgb(r, g, b)),
                Pickable::IGNORE,
                children![(colored_text("", 14.0, Color::srgb(r, g, b)), BuffLabel(buff))],
                ChildOf(*bar),
            ));
        }
    }
}

#[derive(Component)]
struct BuffLabel(shared::buff::Buff);

/// Met à jour le texte de chaque pastille (nom et secondes restantes).
fn update_buff_labels(player: Res<LocalPlayer>, mut labels: Query<(&BuffLabel, &mut Text)>) {
    for (label, mut text) in &mut labels {
        let left = player
            .buffs
            .0
            .iter()
            .find(|(b, _)| *b == label.0)
            .map(|(_, t)| *t)
            .unwrap_or(0.0);
        let value = format!("{} · {}s", label.0.name(), left.ceil() as i32);
        if text.0 != value {
            text.0 = value;
        }
    }
}

/// Jauge de soif, sous celle de la faim ; la soif n'existe que dans les mondes à lacs.
fn update_thirst(
    player: Res<LocalPlayer>,
    world: Option<Res<crate::net::ClientWorld>>,
    mut gauge: Single<&mut Visibility, With<ThirstGauge>>,
    mut fill: Single<&mut Node, With<ThirstFill>>,
    mut label: Single<&mut Text, With<ThirstText>>,
) {
    let lakes = world.as_deref().is_some_and(|w| w.generator().water_level().is_some());
    **gauge = if lakes {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    fill.width = percent(player.thirst.clamp(0.0, 100.0));
    let value = if player.thirst <= 0.0 {
        "Assoiffé !".to_string()
    } else {
        format!("Soif {:.0}", player.thirst.ceil())
    };
    if label.0 != value {
        label.0 = value;
    }
}
