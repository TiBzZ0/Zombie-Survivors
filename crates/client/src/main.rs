//! Client du jeu Zombie Survivors.
//!
//! Lancement :
//! - sans argument : menu principal (solo/hébergement local ou rejoindre un serveur) ;
//! - `--solo` : lance directement une partie locale ;
//! - `--connect <adresse:port>` : rejoint directement un serveur ;
//! - `--name <pseudo>` : choisit le pseudo ;
//! - `--pvp` : la partie hébergée autorise le combat entre joueurs (bases protégées par bannière).

// Sous Windows, pas de fenêtre de console en plus du jeu (en version release).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod animals;
mod chat;
mod chest;
mod crafting;
mod effects;
mod electricity;
mod hud;
mod icons;
mod inventory_ui;
mod menu;
mod meshing;
mod minimap;
mod net;
mod player;
mod remote;
mod settings;
mod sky;
mod sound;
mod survivors;
mod terrain;
mod textures;
mod tutorial;
mod zombies;

use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions};

/// Écrans du jeu.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    #[default]
    Menu,
    Connecting,
    Playing,
}

/// Paramètres de lancement, venant de la ligne de commande ou du menu.
#[derive(Resource, Debug, Clone)]
pub struct LaunchOptions {
    pub name: String,
    pub address: String,
    /// Partie locale : le client démarre son propre serveur.
    pub host_locally: bool,
    /// Saute le menu au démarrage.
    pub auto_start: bool,
    /// Partie locale hébergée avec le combat entre joueurs.
    pub pvp: bool,
}

impl LaunchOptions {
    fn from_args() -> Self {
        let mut options = Self {
            name: std::env::var("USERNAME")
                .or_else(|_| std::env::var("USER"))
                .unwrap_or_else(|_| "Survivant".into()),
            address: format!("127.0.0.1:{}", shared::DEFAULT_PORT),
            host_locally: true,
            auto_start: false,
            pvp: false,
        };
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--solo" => {
                    options.host_locally = true;
                    options.auto_start = true;
                }
                "--connect" => {
                    if let Some(address) = args.next() {
                        options.address = address;
                        options.host_locally = false;
                        options.auto_start = true;
                    }
                }
                "--pvp" => options.pvp = true,
                "--name" => {
                    if let Some(name) = args.next() {
                        options.name = name;
                    }
                }
                other => eprintln!("Argument inconnu ignoré : {other}"),
            }
        }
        options
    }
}

/// Message affiché dans le menu après un échec (connexion refusée, serveur perdu).
#[derive(Resource, Default, Debug)]
pub struct MenuNotice(pub Option<String>);

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Zombie Survivors".into(),
                ..default()
            }),
            ..default()
        }))
        .init_state::<AppState>()
        .insert_resource(LaunchOptions::from_args())
        .init_resource::<MenuNotice>()
        .insert_resource(ClearColor(Color::srgb(0.53, 0.72, 0.90)))
        .add_plugins((
            menu::MenuPlugin,
            net::NetPlugin,
            terrain::TerrainPlugin,
            player::PlayerPlugin,
            remote::RemotePlayersPlugin,
            hud::HudPlugin,
            sky::SkyPlugin,
            zombies::ZombiesPlugin,
            animals::AnimalsPlugin,
            crafting::CraftingPlugin,
            effects::EffectsPlugin,
            sound::SoundPlugin,
            chat::ChatPlugin,
            chest::ChestPlugin,
            survivors::SurvivorsPlugin,
        ))
        .add_plugins((
            settings::SettingsPlugin,
            minimap::MinimapPlugin,
            tutorial::TutorialPlugin,
            electricity::ElectricityPlugin,
            inventory_ui::InventoryUiPlugin,
        ))
        .add_systems(PreStartup, install_font)
        .add_systems(Startup, setup_scene)
        .add_systems(OnExit(AppState::Playing), release_cursor)
        .run();
}

/// Police Inter (licence OFL) intégrée au binaire : la police par défaut de Bevy n'a pas les accents.
const UI_FONT: &[u8] = include_bytes!("../assets/fonts/Inter-SemiBold.otf");

fn install_font(mut fonts: ResMut<Assets<Font>>) {
    let _ = fonts.insert(AssetId::default(), Font::from_bytes(UI_FONT.to_vec()));
}

fn setup_scene(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 80.0, 0.0).looking_to(Vec3::new(1.0, -0.3, 1.0), Vec3::Y),
        DistanceFog {
            color: Color::srgb(0.53, 0.72, 0.90),
            falloff: FogFalloff::Linear {
                start: 120.0,
                end: 250.0,
            },
            ..default()
        },
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 8_000.0,
            ..default()
        },
        Transform::default().looking_to(Vec3::new(-0.4, -1.0, -0.3), Vec3::Y),
    ));
    commands.insert_resource(GlobalAmbientLight {
        brightness: 600.0,
        ..default()
    });
}

fn release_cursor(mut cursor: Single<&mut CursorOptions>) {
    cursor.visible = true;
    cursor.grab_mode = CursorGrabMode::None;
}
