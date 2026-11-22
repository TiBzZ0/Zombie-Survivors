//! Serveur dédié : `cargo run -p server --release`, ou l'image Docker.

use server::{ServerConfig, build_app, install_shutdown_handler};

fn main() {
    let config = match ServerConfig::from_env() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("Configuration invalide : {error}");
            std::process::exit(2);
        }
    };
    let mut app = build_app(config, true);
    install_shutdown_handler(&mut app);
    app.run();
}
