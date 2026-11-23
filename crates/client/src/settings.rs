//! Réglages du joueur : sensibilité de la souris, volume, plein écran, distance d'affichage,
//! carte et tutoriel.
//! Enregistrés dans un petit fichier texte à côté de la sauvegarde locale.

use std::path::PathBuf;

use bevy::audio::{GlobalVolume, Volume};
use bevy::prelude::*;
use bevy::window::{MonitorSelection, PrimaryWindow, WindowMode};

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Settings::load())
            .add_systems(Update, (toggle_fullscreen_key, apply_settings).chain());
    }
}

#[derive(Resource, Debug, Clone, PartialEq)]
pub struct Settings {
    /// Multiplicateur de la sensibilité de la souris.
    pub sensitivity: f32,
    /// Volume général, de 0 à 1.
    pub volume: f32,
    pub fullscreen: bool,
    /// Distance d'affichage, en chunks (moins = plus fluide).
    pub view_distance: i32,
    /// Carte en haut à droite (touche M).
    pub minimap: bool,
    /// Les objectifs du tutoriel sont affichés (on peut les masquer avec H).
    pub tutorial: bool,
    /// Ombres projetées par le soleil et la lune (à couper sur un petit ordinateur).
    pub shadows: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            sensitivity: 1.0,
            volume: 0.8,
            fullscreen: false,
            view_distance: 7,
            minimap: true,
            tutorial: true,
            shadows: true,
        }
    }
}

pub const SENSITIVITY_RANGE: (f32, f32) = (0.25, 3.0);
/// Distances d'affichage possibles, en chunks.
pub const VIEW_DISTANCE_RANGE: (i32, i32) = (3, 12);

impl Settings {
    fn path() -> PathBuf {
        let local = server::ServerConfig::local().data_dir;
        local
            .parent()
            .map_or(local.clone(), |p| p.to_path_buf())
            .join("reglages.txt")
    }

    fn load() -> Self {
        std::fs::read_to_string(Self::path())
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    /// Lit des lignes `clé = valeur` ; une ligne invalide garde la valeur par défaut.
    pub fn parse(text: &str) -> Self {
        let mut settings = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "sensibilite" => {
                    if let Ok(v) = value.parse::<f32>() {
                        settings.sensitivity = v.clamp(SENSITIVITY_RANGE.0, SENSITIVITY_RANGE.1);
                    }
                }
                "volume" => {
                    if let Ok(v) = value.parse::<f32>() {
                        settings.volume = v.clamp(0.0, 1.0);
                    }
                }
                "plein_ecran" => settings.fullscreen = value == "oui",
                "distance" => {
                    if let Ok(v) = value.parse::<i32>() {
                        settings.view_distance = v.clamp(VIEW_DISTANCE_RANGE.0, VIEW_DISTANCE_RANGE.1);
                    }
                }
                "carte" => settings.minimap = value != "non",
                "tutoriel" => settings.tutorial = value != "non",
                "ombres" => settings.shadows = value != "non",
                _ => {}
            }
        }
        settings
    }

    pub fn to_text(&self) -> String {
        let yes = |b: bool| if b { "oui" } else { "non" };
        format!(
            "sensibilite = {:.2}\nvolume = {:.2}\nplein_ecran = {}\ndistance = {}\ncarte = {}\ntutoriel = {}\nombres = {}\n",
            self.sensitivity,
            self.volume,
            yes(self.fullscreen),
            self.view_distance,
            yes(self.minimap),
            yes(self.tutorial),
            yes(self.shadows)
        )
    }

    pub fn save(&self) {
        let path = Self::path();
        let result = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|_| std::fs::write(&path, self.to_text()));
        if let Err(error) = result {
            warn!("Réglages non enregistrés dans {} : {error}", path.display());
        }
    }

    /// Change la sensibilité d'un cran (`steps` positif ou négatif).
    pub fn nudge_sensitivity(&mut self, steps: f32) {
        let v = (self.sensitivity + steps * 0.25).clamp(SENSITIVITY_RANGE.0, SENSITIVITY_RANGE.1);
        self.sensitivity = (v * 4.0).round() / 4.0;
    }

    /// Change la distance d'affichage d'un chunk.
    pub fn nudge_view_distance(&mut self, steps: f32) {
        self.view_distance = (self.view_distance + steps as i32).clamp(VIEW_DISTANCE_RANGE.0, VIEW_DISTANCE_RANGE.1);
    }

    /// Change le volume de 10 %.
    pub fn nudge_volume(&mut self, steps: f32) {
        self.volume = ((self.volume + steps * 0.1).clamp(0.0, 1.0) * 10.0).round() / 10.0;
    }
}

/// F11 bascule le plein écran, partout.
fn toggle_fullscreen_key(keys: Res<ButtonInput<KeyCode>>, mut settings: ResMut<Settings>) {
    if keys.just_pressed(KeyCode::F11) {
        settings.fullscreen = !settings.fullscreen;
        settings.save();
    }
}

fn apply_settings(
    settings: Res<Settings>,
    mut volume: ResMut<GlobalVolume>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
) {
    if !settings.is_changed() {
        return;
    }
    volume.volume = Volume::Linear(settings.volume);
    let mode = if settings.fullscreen {
        WindowMode::BorderlessFullscreen(MonitorSelection::Current)
    } else {
        WindowMode::Windowed
    };
    if window.mode != mode {
        window.mode = mode;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_clamp() {
        let settings = Settings {
            sensitivity: 1.5,
            volume: 0.3,
            fullscreen: true,
            view_distance: 5,
            minimap: false,
            tutorial: false,
            shadows: false,
        };
        assert_eq!(Settings::parse(&settings.to_text()), settings);
        let odd = Settings::parse("sensibilite = 99\nvolume = abc\nautre = 1\ndistance = 40");
        assert_eq!(odd.sensitivity, SENSITIVITY_RANGE.1);
        assert_eq!(odd.view_distance, VIEW_DISTANCE_RANGE.1);
        // Un ancien fichier de réglages garde la carte et le tutoriel.
        assert!(odd.minimap && odd.tutorial && odd.shadows);
        assert_eq!(odd.volume, Settings::default().volume);
        let mut s = Settings::default();
        s.nudge_volume(5.0);
        assert_eq!(s.volume, 1.0);
        s.nudge_sensitivity(-10.0);
        assert_eq!(s.sensitivity, SENSITIVITY_RANGE.0);
    }
}
