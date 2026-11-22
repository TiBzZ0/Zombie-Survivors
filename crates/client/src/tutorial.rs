//! Objectifs pour débuter, affichés à gauche de l'écran : ils se cochent tout seuls.
//! La touche H les masque ou les réaffiche (réglage enregistré).

use bevy::prelude::*;
use shared::block::Block;
use shared::item::{Inventory, Item};

use crate::AppState;
use crate::net::ClientWorld;
use crate::player::LocalPlayer;
use crate::settings::Settings;
use crate::sky::GameClock;

pub struct TutorialPlugin;

impl Plugin for TutorialPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Progress>()
            .add_systems(OnEnter(AppState::Playing), spawn_panel)
            .add_systems(OnExit(AppState::Playing), despawn_panel)
            .add_systems(Update, update_panel.run_if(in_state(AppState::Playing)));
    }
}

/// Étapes du tutoriel, dans l'ordre conseillé.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Wood,
    Workbench,
    Pickaxe,
    Weapon,
    Eat,
    Drink,
    Night,
}

impl Step {
    pub const ALL: [Step; 7] = [
        Step::Wood,
        Step::Workbench,
        Step::Pickaxe,
        Step::Weapon,
        Step::Eat,
        Step::Drink,
        Step::Night,
    ];

    fn text(self) -> &'static str {
        match self {
            Step::Wood => "Récolter du bois (clic gauche maintenu sur un tronc)",
            Step::Workbench => "Fabriquer un établi (C)",
            Step::Pickaxe => "Fabriquer une pioche",
            Step::Weapon => "Se fabriquer une arme",
            Step::Eat => "Manger (clic droit, nourriture en main)",
            Step::Drink => "Boire à un lac (clic droit sur l'eau)",
            Step::Night => "Survivre à la première nuit",
        }
    }

    /// Vrai si l'inventaire montre que l'étape est faite.
    pub fn done_by(self, inventory: &Inventory) -> bool {
        let any = |f: fn(Item) -> bool| inventory.iter().any(|(item, _)| f(item));
        match self {
            Step::Wood => any(|i| matches!(i, Item::Block(Block::Wood | Block::Planks))),
            Step::Workbench => inventory.has(Item::Block(Block::Workbench), 1),
            Step::Pickaxe => any(|i| matches!(i, Item::Pickaxe(_))),
            Step::Weapon => any(Item::is_weapon),
            _ => false,
        }
    }
}

/// Étapes faites dans cette partie.
#[derive(Resource, Default)]
struct Progress {
    done: Vec<Step>,
    last_hunger: f32,
    last_thirst: f32,
}

#[derive(Component)]
struct TutorialText;

fn spawn_panel(mut commands: Commands, mut progress: ResMut<Progress>) {
    *progress = Progress {
        last_hunger: 100.0,
        last_thirst: 100.0,
        ..default()
    };
    commands.spawn((
        TutorialText,
        Node {
            position_type: PositionType::Absolute,
            left: px(12),
            top: percent(28),
            padding: px(8).all(),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.35)),
        Text::new(""),
        TextFont::from_font_size(15.0),
        TextColor(Color::WHITE),
        Pickable::IGNORE,
    ));
}

fn despawn_panel(mut commands: Commands, panels: Query<Entity, With<TutorialText>>) {
    for entity in &panels {
        commands.entity(entity).despawn();
    }
}

fn update_panel(
    keys: Res<ButtonInput<KeyCode>>,
    player: Res<LocalPlayer>,
    clock: Res<GameClock>,
    world: Option<Res<ClientWorld>>,
    mut settings: ResMut<Settings>,
    mut progress: ResMut<Progress>,
    mut panel: Single<(&mut Text, &mut Visibility), With<TutorialText>>,
) {
    if keys.just_pressed(KeyCode::KeyH) && !player.typing {
        settings.tutorial = !settings.tutorial;
        settings.save();
    }
    // La soif n'existe que dans les mondes à lacs.
    let lakes = world.is_some_and(|w| w.generator().water_level().is_some());
    let steps: Vec<Step> = Step::ALL.into_iter().filter(|s| lakes || *s != Step::Drink).collect();
    let mut newly = Vec::new();
    for step in &steps {
        let done = match step {
            Step::Eat => player.hunger > progress.last_hunger + 1.0,
            Step::Drink => player.thirst > progress.last_thirst + 1.0,
            Step::Night => clock.time.day >= 2,
            other => other.done_by(&player.inventory),
        };
        if done && !progress.done.contains(step) {
            newly.push(*step);
        }
    }
    progress.done.extend(newly);
    progress.last_hunger = player.hunger;
    progress.last_thirst = player.thirst;

    let (text, visibility) = &mut *panel;
    let finished = steps.iter().all(|s| progress.done.contains(s));
    let wanted = if settings.tutorial && !finished && player.dead.is_none() {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if **visibility != wanted {
        **visibility = wanted;
    }
    let mut lines = vec!["Objectifs (H pour masquer)".to_string()];
    for step in &steps {
        let mark = if progress.done.contains(step) { "[x]" } else { "[  ]" };
        lines.push(format!("{mark} {}", step.text()));
    }
    let value = lines.join("\n");
    if text.0 != value {
        text.0 = value;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::item::Tier;

    #[test]
    fn steps_follow_the_inventory() {
        let mut inventory = Inventory::default();
        assert!(!Step::Wood.done_by(&inventory));
        inventory.add(Item::Block(Block::Wood), 3);
        assert!(Step::Wood.done_by(&inventory));
        assert!(!Step::Pickaxe.done_by(&inventory));
        inventory.add(Item::Pickaxe(Tier::Wood), 1);
        assert!(Step::Pickaxe.done_by(&inventory));
        assert!(!Step::Weapon.done_by(&inventory));
        inventory.add(Item::Spear(Tier::Stone), 1);
        assert!(Step::Weapon.done_by(&inventory));
    }
}
