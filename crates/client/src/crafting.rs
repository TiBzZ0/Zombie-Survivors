//! Panneau de fabrication (touche C ou Tab) : recettes rangées par rubrique, détail de la recette
//! choisie (composants possédés et manquants, atelier requis), inventaire et barre d'outils.

use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui_widgets::{Activate, Button, observe};
use bevy_renet::RenetClient;
use shared::block::Block;
use shared::crafting::{CATEGORIES, Category, RECIPES, Recipe, station_near};
use shared::protocol::ClientMessage;

use crate::AppState;
use crate::icons::ItemIcons;
use crate::net::{ClientWorld, send_reliable};
use crate::player::LocalPlayer;

pub struct CraftingPlugin;

impl Plugin for CraftingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CraftingView>()
            .add_systems(OnEnter(AppState::Playing), spawn_panel)
            .add_systems(OnExit(AppState::Playing), despawn_panel)
            .add_systems(
                Update,
                (update_panel, update_detail)
                    .chain()
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

/// Rubrique affichée et recette choisie.
#[derive(Resource)]
struct CraftingView {
    category: Category,
    selected: Option<usize>,
}

impl Default for CraftingView {
    fn default() -> Self {
        Self {
            category: Category::Materials,
            selected: Some(0),
        }
    }
}

#[derive(Component)]
struct CraftingPanel;

#[derive(Component)]
struct RecipeButton(usize);

#[derive(Component)]
struct RecipeIcon(usize);

#[derive(Component)]
struct CategoryTab(Category);

#[derive(Component)]
struct DetailIcon;

#[derive(Component)]
struct DetailName;

#[derive(Component)]
struct DetailText;

/// Ligne d'un composant de la recette choisie.
#[derive(Component)]
struct ComponentRow(usize);

#[derive(Component)]
struct ComponentIcon(usize);

#[derive(Component)]
struct ComponentText(usize);

#[derive(Component)]
struct StationText;

/// Palier technologique de la recette choisie.
#[derive(Component)]
struct TechText;

/// Bouton « Fabriquer » (1) ou « ×5 » (5).
#[derive(Component)]
struct CraftButton(u32);

/// Composants affichés au plus pour une recette.
const MAX_INPUTS: usize = 4;

const AVAILABLE: Color = Color::srgb(0.18, 0.26, 0.18);
const AVAILABLE_HOVER: Color = Color::srgb(0.28, 0.40, 0.26);
const UNAVAILABLE: Color = Color::srgb(0.14, 0.14, 0.14);
const SELECTED: Color = Color::srgb(1.0, 0.85, 0.3);
const BORDER: Color = Color::srgba(0.3, 0.3, 0.3, 0.9);
const GOOD: Color = Color::srgb(0.55, 0.9, 0.5);
const BAD: Color = Color::srgb(0.95, 0.45, 0.4);
const DIM: Color = Color::srgb(0.6, 0.6, 0.6);

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

pub fn station_name(station: Block) -> &'static str {
    match station {
        Block::Workbench => "Établi",
        Block::Forge => "Forge",
        Block::Campfire => "Feu de camp",
        other => other.name(),
    }
}

/// Vrai si l'atelier demandé par la recette est à portée du joueur (le serveur vérifie aussi).
fn station_ok(recipe: &Recipe, player: &LocalPlayer, world: Option<&ClientWorld>) -> bool {
    let Some(station) = recipe.station else {
        return true;
    };
    let Some(world) = world else { return false };
    let feet = player.position.floor().as_ivec3();
    station_near(feet, station, |p| world.0.block(p))
}

fn craftable(recipe: &Recipe, player: &LocalPlayer, world: Option<&ClientWorld>) -> bool {
    recipe.can_craft(&player.inventory) && station_ok(recipe, player, world) && player.progress.has_tech(recipe.tech())
}

/// Combien de fois on peut fabriquer la recette avec l'inventaire actuel.
fn times(recipe: &Recipe, player: &LocalPlayer) -> u32 {
    recipe
        .inputs
        .iter()
        .map(|(item, n)| player.inventory.count(*item) / n)
        .min()
        .unwrap_or(0)
}

fn small_button(text: &str, marker: impl Component, width: f32) -> impl Bundle {
    (
        Button,
        Hovered::default(),
        marker,
        Node {
            width: px(width),
            padding: UiRect::axes(px(6), px(4)),
            border: UiRect::all(px(2)),
            justify_content: JustifyContent::Center,
            ..default()
        },
        BackgroundColor(UNAVAILABLE),
        BorderColor::all(BORDER),
        children![(label(text, 14.0), Pickable::IGNORE)],
    )
}

fn spawn_panel(mut commands: Commands, mut icons: ResMut<ItemIcons>, mut images: ResMut<Assets<Image>>) {
    let panel = commands
        .spawn((
            CraftingPanel,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            Visibility::Hidden,
            GlobalZIndex(10),
        ))
        .id();
    let content = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: px(20),
                padding: px(18).all(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.04, 0.05, 0.04, 0.97)),
            ChildOf(panel),
        ))
        .id();

    // Colonne 1 : rubriques et recettes.
    let recipes = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                width: px(8.0 * 50.0),
                ..default()
            },
            ChildOf(content),
        ))
        .id();
    commands.spawn((label("FABRICATION", 22.0), ChildOf(recipes)));
    let tabs = commands
        .spawn((
            Node {
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(4),
                row_gap: px(4),
                ..default()
            },
            ChildOf(recipes),
        ))
        .id();
    for category in CATEGORIES {
        commands.spawn((
            small_button(category.name(), CategoryTab(category), 96.0),
            ChildOf(tabs),
            observe(move |_: On<Activate>, mut view: ResMut<CraftingView>| {
                view.category = category;
                view.selected = RECIPES.iter().position(|r| r.category() == category);
            }),
        ));
    }
    let grid = commands
        .spawn((
            Node {
                display: Display::Grid,
                grid_template_columns: RepeatedGridTrack::px(8, 46.0),
                column_gap: px(4),
                row_gap: px(4),
                min_height: px(3.0 * 50.0),
                align_content: AlignContent::Start,
                ..default()
            },
            ChildOf(recipes),
        ))
        .id();
    for (index, recipe) in RECIPES.iter().enumerate() {
        let count = if recipe.count > 1 {
            recipe.count.to_string()
        } else {
            String::new()
        };
        commands.spawn((
            Button,
            Hovered::default(),
            RecipeButton(index),
            Node {
                width: px(46),
                height: px(46),
                border: UiRect::all(px(2)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(UNAVAILABLE),
            BorderColor::all(BORDER),
            ChildOf(grid),
            children![
                (
                    Node {
                        width: px(34),
                        height: px(34),
                        ..default()
                    },
                    ImageNode::new(icons.get(&mut images, recipe.output)),
                    RecipeIcon(index),
                    Pickable::IGNORE,
                ),
                (
                    Node {
                        position_type: PositionType::Absolute,
                        right: px(3),
                        bottom: px(1),
                        ..default()
                    },
                    label(&count, 13.0),
                    Pickable::IGNORE,
                )
            ],
            observe(
                move |_: On<Activate>,
                      mut view: ResMut<CraftingView>,
                      keys: Res<ButtonInput<KeyCode>>,
                      mut client: ResMut<RenetClient>,
                      player: Res<LocalPlayer>,
                      world: Option<Res<ClientWorld>>| {
                    view.selected = Some(index);
                    // Maj + clic : fabrique tout de suite.
                    let quick = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
                    if quick && craftable(recipe, &player, world.as_deref()) {
                        send_reliable(&mut client, &ClientMessage::Craft { recipe: index as u16 });
                    }
                },
            ),
        ));
    }
    commands.spawn((
        colored_label(
            "C / Tab : fermer · Clic : choisir · Maj + clic : fabriquer\nSurvoler un objet de l'inventaire + touche 1 à 9 : le ranger dans la barre",
            13.0,
            DIM,
        ),
        ChildOf(recipes),
    ));

    // Colonne 2 : détail de la recette choisie.
    let detail = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                width: px(290),
                padding: px(10).all(),
                border: UiRect::all(px(2)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.08, 0.09, 0.08, 0.9)),
            BorderColor::all(BORDER),
            ChildOf(content),
        ))
        .id();
    commands.spawn((
        Node {
            column_gap: px(10),
            align_items: AlignItems::Center,
            ..default()
        },
        ChildOf(detail),
        children![
            (
                Node {
                    width: px(56),
                    height: px(56),
                    ..default()
                },
                ImageNode::default(),
                DetailIcon,
            ),
            (label("", 18.0), DetailName),
        ],
    ));
    commands.spawn((
        Node {
            min_height: px(36),
            ..default()
        },
        colored_label("", 14.0, Color::srgb(0.85, 0.85, 0.75)),
        DetailText,
        ChildOf(detail),
    ));
    commands.spawn((colored_label("Il faut :", 15.0, DIM), ChildOf(detail)));
    for i in 0..MAX_INPUTS {
        commands.spawn((
            ComponentRow(i),
            Node {
                column_gap: px(8),
                align_items: AlignItems::Center,
                ..default()
            },
            ChildOf(detail),
            children![
                (
                    Node {
                        width: px(26),
                        height: px(26),
                        ..default()
                    },
                    ImageNode::default(),
                    ComponentIcon(i),
                ),
                (label("", 15.0), ComponentText(i)),
            ],
        ));
    }
    commands.spawn((
        Node {
            min_height: px(22),
            ..default()
        },
        label("", 15.0),
        StationText,
        ChildOf(detail),
    ));
    commands.spawn((label("", 15.0), TechText, ChildOf(detail)));
    commands.spawn((
        Node {
            column_gap: px(8),
            ..default()
        },
        ChildOf(detail),
        children![
            (
                small_button("Fabriquer", CraftButton(1), 150.0),
                observe(craft_selected(1)),
            ),
            (small_button("×5", CraftButton(5), 70.0), observe(craft_selected(5))),
        ],
    ));

    // Colonne 3 : inventaire et barre d'outils.
    let inventory = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                ..default()
            },
            ChildOf(content),
            children![label("INVENTAIRE", 22.0)],
        ))
        .id();
    crate::inventory_ui::spawn_grid(&mut commands, inventory);
}

/// Fabrique la recette choisie jusqu'à `n` fois (autant que l'inventaire le permet).
fn craft_selected(
    n: u32,
) -> impl Fn(On<Activate>, Res<CraftingView>, ResMut<RenetClient>, Res<LocalPlayer>, Option<Res<ClientWorld>>) {
    move |_, view, mut client, player, world| {
        let Some(index) = view.selected else { return };
        let recipe = &RECIPES[index];
        if !craftable(recipe, &player, world.as_deref()) {
            return;
        }
        for _ in 0..n.min(times(recipe, &player)) {
            send_reliable(&mut client, &ClientMessage::Craft { recipe: index as u16 });
        }
    }
}

fn despawn_panel(mut commands: Commands, panels: Query<Entity, With<CraftingPanel>>) {
    for panel in &panels {
        commands.entity(panel).despawn();
    }
}

#[allow(clippy::type_complexity)]
fn update_panel(
    player: Res<LocalPlayer>,
    view: Res<CraftingView>,
    mut panel: Single<&mut Visibility, With<CraftingPanel>>,
    mut buttons: Query<(
        &RecipeButton,
        &Hovered,
        &mut BackgroundColor,
        &mut BorderColor,
        &mut Node,
    )>,
    mut icons: Query<(&RecipeIcon, &mut ImageNode)>,
    mut tabs: Query<
        (&CategoryTab, &Hovered, &mut BackgroundColor, &mut BorderColor),
        (Without<RecipeButton>, Without<CraftButton>),
    >,
    world: Option<Res<ClientWorld>>,
) {
    **panel = if player.crafting_open && player.dead.is_none() {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    if !player.crafting_open {
        return;
    }
    for (button, over, mut color, mut border, mut node) in &mut buttons {
        let recipe = &RECIPES[button.0];
        node.display = if recipe.category() == view.category {
            Display::Flex
        } else {
            Display::None
        };
        let ok = craftable(recipe, &player, world.as_deref());
        color.0 = match (ok, over.get()) {
            (true, true) => AVAILABLE_HOVER,
            (true, false) => AVAILABLE,
            (false, true) => Color::srgb(0.22, 0.22, 0.22),
            (false, false) => UNAVAILABLE,
        };
        *border = BorderColor::all(if view.selected == Some(button.0) {
            SELECTED
        } else {
            BORDER
        });
    }
    for (icon, mut image) in &mut icons {
        let recipe = &RECIPES[icon.0];
        // Recette d'un palier technologique pas encore appris : icône rougie.
        image.color = if !player.progress.has_tech(recipe.tech()) {
            Color::srgba(0.9, 0.35, 0.3, 0.35)
        } else if craftable(recipe, &player, world.as_deref()) {
            Color::WHITE
        } else {
            Color::srgba(1.0, 1.0, 1.0, 0.4)
        };
    }
    for (tab, over, mut color, mut border) in &mut tabs {
        let current = tab.0 == view.category;
        color.0 = if current {
            AVAILABLE
        } else if over.get() {
            Color::srgb(0.22, 0.22, 0.22)
        } else {
            UNAVAILABLE
        };
        *border = BorderColor::all(if current { SELECTED } else { BORDER });
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn update_detail(
    player: Res<LocalPlayer>,
    view: Res<CraftingView>,
    world: Option<Res<ClientWorld>>,
    mut icons: ResMut<ItemIcons>,
    mut images: ResMut<Assets<Image>>,
    mut detail_icon: Single<&mut ImageNode, (With<DetailIcon>, Without<ComponentIcon>)>,
    mut texts: ParamSet<(
        Single<&mut Text, With<DetailName>>,
        Single<&mut Text, With<DetailText>>,
        Single<(&mut Text, &mut TextColor), With<StationText>>,
        Query<(&ComponentText, &mut Text, &mut TextColor)>,
        Single<(&mut Text, &mut TextColor), With<TechText>>,
    )>,
    mut rows: Query<(&ComponentRow, &mut Node)>,
    mut component_icons: Query<(&ComponentIcon, &mut ImageNode), Without<DetailIcon>>,
    mut craft_buttons: Query<(&CraftButton, &Hovered, &mut BackgroundColor)>,
) {
    if !player.crafting_open {
        return;
    }
    let Some(recipe) = view.selected.and_then(|i| RECIPES.get(i)) else {
        return;
    };
    let handle = icons.get(&mut images, recipe.output);
    if detail_icon.image != handle {
        detail_icon.image = handle;
    }
    let name = if recipe.count > 1 {
        format!("{} ×{}", recipe.output.name(), recipe.count)
    } else {
        recipe.output.name()
    };
    texts.p0().0 = name;
    texts.p1().0 = recipe.output.description();
    let station_here = station_ok(recipe, &player, world.as_deref());
    {
        let mut station = texts.p2();
        let (text, color) = &mut *station;
        match recipe.station {
            Some(s) if station_here => {
                text.0 = format!("Atelier : {} (à portée)", station_name(s));
                color.0 = GOOD;
            }
            Some(s) => {
                text.0 = format!("Atelier : {} (à moins de 4 blocs)", station_name(s));
                color.0 = BAD;
            }
            None => {
                text.0 = "À la main, sans atelier".into();
                color.0 = DIM;
            }
        }
    }
    {
        // Palier technologique : une compétence de la branche Technologie à apprendre.
        let mut tech_text = texts.p4();
        let (text, color) = &mut *tech_text;
        let tech = recipe.tech();
        match tech.skill() {
            Some(_) if player.progress.has_tech(tech) => {
                text.0 = format!("Palier {} : maîtrisé", tech.name());
                color.0 = GOOD;
            }
            Some(skill) => {
                text.0 = format!("Palier {} : apprenez « {} » (K)", tech.name(), skill.info().name);
                color.0 = BAD;
            }
            None => {
                text.0 = "Palier primitif".into();
                color.0 = DIM;
            }
        }
    }
    for (row, mut node) in &mut rows {
        node.display = if row.0 < recipe.inputs.len() {
            Display::Flex
        } else {
            Display::None
        };
    }
    for (icon, mut image) in &mut component_icons {
        if let Some((item, _)) = recipe.inputs.get(icon.0) {
            let handle = icons.get(&mut images, *item);
            if image.image != handle {
                image.image = handle;
            }
        }
    }
    for (row, mut text, mut color) in &mut texts.p3() {
        if let Some((item, n)) = recipe.inputs.get(row.0) {
            let have = player.inventory.count(*item);
            text.0 = format!("{} : {have} / {n}", item.name());
            color.0 = if have >= *n { GOOD } else { BAD };
        }
    }
    let ok = craftable(recipe, &player, world.as_deref());
    let possible = times(recipe, &player);
    for (button, over, mut color) in &mut craft_buttons {
        let usable = ok && (button.0 == 1 || possible >= 2);
        color.0 = match (usable, over.get()) {
            (true, true) => AVAILABLE_HOVER,
            (true, false) => AVAILABLE,
            _ => UNAVAILABLE,
        };
    }
}
