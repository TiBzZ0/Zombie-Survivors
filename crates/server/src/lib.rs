//! Serveur de jeu autoritaire.
//!
//! Le même code sert deux usages :
//! - le binaire `server`, lancé seul dans Docker (serveur dédié) ;
//! - le mode local, où le client démarre ce serveur dans un thread à côté de lui.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use bevy::app::ScheduleRunnerPlugin;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use shared::TICK_RATE_HZ;

pub mod animals;
pub mod config;
pub mod daynight;
pub mod electricity;
pub mod events;
pub mod explosives;
pub mod farming;
pub mod net;
pub mod save;
pub mod survivors;
pub mod turrets;
pub mod water;
pub mod world;
pub mod zombies;

pub use config::ServerConfig;

/// Logique de jeu côté serveur, indépendante de la façon dont il est hébergé.
pub struct ServerPlugin {
    pub config: ServerConfig,
}

impl Plugin for ServerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.config.clone())
            .insert_resource(Time::<Fixed>::from_hz(TICK_RATE_HZ))
            .init_resource::<TickCount>()
            .init_resource::<ShutdownFlag>()
            .add_plugins((
                world::WorldPlugin,
                net::NetPlugin,
                daynight::DayNightPlugin,
                zombies::ZombiePlugin,
                save::SavePlugin,
                survivors::SurvivorPlugin,
                turrets::TurretPlugin,
                explosives::ExplosivesPlugin,
                farming::FarmingPlugin,
                animals::AnimalPlugin,
                water::WaterPlugin,
                electricity::ElectricityPlugin,
                events::EventsPlugin,
            ))
            .add_systems(Startup, announce)
            .add_systems(FixedUpdate, count_ticks)
            .add_systems(Update, report_status)
            .add_systems(Last, exit_on_shutdown);
    }
}

/// Nombre de ticks de simulation écoulés depuis le démarrage.
#[derive(Resource, Default, Debug)]
pub struct TickCount(pub u64);

fn announce(config: Res<ServerConfig>) {
    info!(
        "Serveur prêt : port {}, {} joueurs max, graine {}, carte de {} m de côté",
        config.port,
        config.max_players,
        config.world.seed,
        config.world.size_blocks()
    );
}

fn count_ticks(mut ticks: ResMut<TickCount>) {
    ticks.0 += 1;
}

fn report_status(ticks: Res<TickCount>, time: Res<Time>, mut last: Local<f32>) {
    let now = time.elapsed_secs();
    if now - *last >= 30.0 {
        *last = now;
        info!("{} ticks simulés en {:.0} s", ticks.0, now);
    }
}

/// Construit l'application serveur sans rendu. `with_logs` est faux en mode local,
/// car le client a déjà installé son propre journal.
pub fn build_app(config: ServerConfig, with_logs: bool) -> App {
    let mut app = App::new();
    // La boucle tourne plus vite que le tick : `FixedUpdate` garde lui le rythme de 30 Hz.
    app.add_plugins(
        MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(
            1.0 / (TICK_RATE_HZ * 2.0),
        ))),
    );
    if with_logs {
        app.add_plugins(LogPlugin::default());
    }
    app.add_plugins(ServerPlugin { config });
    app
}

/// Drapeau d'arrêt partagé : quand il passe à vrai, le serveur se ferme proprement
/// à la fin du tick en cours (et pourra sauvegarder avant).
#[derive(Resource, Clone, Default)]
pub struct ShutdownFlag(pub Arc<AtomicBool>);

impl ShutdownFlag {
    pub fn request(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

pub(crate) fn exit_on_shutdown(flag: Res<ShutdownFlag>, mut exit: MessageWriter<AppExit>) {
    if flag.0.load(Ordering::Relaxed) {
        info!("Arrêt demandé, fermeture du serveur");
        exit.write(AppExit::Success);
    }
}

/// Arrête proprement le serveur sur SIGTERM (`docker stop`) ou Ctrl+C.
pub fn install_shutdown_handler(app: &mut App) {
    let flag = app.world().resource::<ShutdownFlag>().clone();
    if let Err(error) = ctrlc::set_handler(move || flag.request()) {
        warn!("Impossible d'intercepter les signaux d'arrêt : {error}");
    }
}

/// Serveur local lancé par le client. Le serveur s'arrête quand la poignée est abandonnée.
pub struct LocalServer {
    flag: ShutdownFlag,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for LocalServer {
    fn drop(&mut self) {
        self.flag.request();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Démarre le serveur dans un thread, pour le mode local (solo ou LAN).
pub fn spawn_local(config: ServerConfig) -> std::io::Result<LocalServer> {
    let flag = ShutdownFlag::default();
    let thread_flag = flag.clone();
    let thread = std::thread::Builder::new()
        .name("serveur-local".into())
        .spawn(move || {
            let mut app = build_app(config, false);
            app.insert_resource(thread_flag);
            app.run();
        })?;
    Ok(LocalServer {
        flag,
        thread: Some(thread),
    })
}
