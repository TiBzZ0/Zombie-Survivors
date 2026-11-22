//! Mesure du coût d'un tick serveur avec une grosse horde qui poursuit un joueur.
//!
//! `cargo run -p server --release --example horde_bench -- [zombies]`

use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use server::net::{ConnectedPlayer, Lobby};
use server::world::SpawnPoint;
use server::{ServerConfig, build_app};

fn main() {
    let count: usize = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(320);
    // SAFETY : aucun autre fil ne tourne encore.
    unsafe { std::env::set_var("STRESS_ZOMBIES", count.to_string()) };
    let data_dir = std::env::temp_dir().join(format!("zs-bench-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data_dir);
    let config = ServerConfig {
        port: 0,
        data_dir: data_dir.clone(),
        ..ServerConfig::default()
    };
    let mut app = build_app(config, false);
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 30.0)));
    app.finish();
    app.cleanup();
    app.update();
    let spawn = app.world().resource::<SpawnPoint>().0;
    app.world_mut()
        .resource_mut::<Lobby>()
        .players
        .insert(1, ConnectedPlayer::new(spawn));

    let ticks = 900;
    let mut worst = 0.0f64;
    let started = Instant::now();
    for _ in 0..ticks {
        if let Some(p) = app.world_mut().resource_mut::<Lobby>().players.get_mut(&1) {
            p.health = 100.0;
            p.state.position = spawn;
        }
        let t = Instant::now();
        app.update();
        worst = worst.max(t.elapsed().as_secs_f64() * 1000.0);
    }
    let avg = started.elapsed().as_secs_f64() * 1000.0 / ticks as f64;
    let alive = app
        .world_mut()
        .query::<&server::zombies::Zombie>()
        .iter(app.world())
        .count();
    println!("{alive} zombies encore en vie à la fin");
    println!("{count} zombies : {avg:.2} ms par tick en moyenne, {worst:.2} ms au pire (budget : 33 ms)");
    let _ = std::fs::remove_dir_all(&data_dir);
}
