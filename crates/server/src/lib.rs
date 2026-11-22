//! Serveur de jeu autoritaire.
//!
//! Le même code sert deux usages :
//! - le binaire `server`, lancé seul dans Docker (serveur dédié) ;
//! - le mode local, où le client démarre ce serveur dans un thread à côté de lui.

use std::time::Duration;

use bevy::app::ScheduleRunnerPlugin;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use shared::TICK_RATE_HZ;

pub mod config;
pub mod world;

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
            .add_plugins(world::WorldPlugin)
            .add_systems(Startup, announce)
            .add_systems(FixedUpdate, count_ticks)
            .add_systems(Update, report_status);
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

/// Arrête proprement le serveur sur SIGTERM (`docker stop`) ou Ctrl+C, pour laisser
/// le temps de sauvegarder au lieu d'être tué en plein tick.
pub fn install_shutdown_handler(app: &mut App) {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[derive(Resource, Clone)]
    struct ShutdownRequested(Arc<AtomicBool>);

    fn exit_on_signal(flag: Res<ShutdownRequested>, mut exit: MessageWriter<AppExit>) {
        if flag.0.load(Ordering::Relaxed) {
            info!("Signal d'arrêt reçu, fermeture du serveur");
            exit.write(AppExit::Success);
        }
    }

    let flag = Arc::new(AtomicBool::new(false));
    let handler_flag = flag.clone();
    if let Err(error) = ctrlc::set_handler(move || handler_flag.store(true, Ordering::Relaxed)) {
        warn!("Impossible d'intercepter les signaux d'arrêt : {error}");
    }
    app.insert_resource(ShutdownRequested(flag))
        .add_systems(Last, exit_on_signal);
}

/// Démarre le serveur dans un thread, pour le mode local (solo ou LAN).
pub fn spawn_local(config: ServerConfig) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("serveur-local".into())
        .spawn(move || {
            build_app(config, false).run();
        })
        .expect("impossible de démarrer le thread du serveur local")
}
