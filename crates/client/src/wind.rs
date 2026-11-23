//! Matériau « au vent » : le matériau standard, avec un shader de sommets qui fait osciller
//! le feuillage, onduler les herbes et les cultures, et rider la surface de l'eau.

use bevy::asset::embedded_asset;
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;

pub struct WindPlugin;

impl Plugin for WindPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/wind.wesl");
        app.add_plugins(MaterialPlugin::<WindMaterial>::default());
    }
}

pub type WindMaterial = ExtendedMaterial<StandardMaterial, Wind>;

/// Réglages du vent pour un matériau.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct Wind {
    /// x : balancement de tout le maillage, y : balancement du haut seulement,
    /// z : hauteur des vaguelettes de l'eau (en blocs).
    #[uniform(100)]
    pub params: Vec4,
}

impl Wind {
    /// Feuillage : tout bouge un peu.
    pub fn foliage() -> Self {
        Self {
            params: Vec4::new(0.045, 0.0, 0.0, 0.0),
        }
    }

    /// Herbes et cultures : le pied reste planté, le haut ondule.
    pub fn plants() -> Self {
        Self {
            params: Vec4::new(0.0, 0.11, 0.0, 0.0),
        }
    }

    /// Eau : de petites vagues sur le dessus.
    pub fn water() -> Self {
        Self {
            params: Vec4::new(0.0, 0.0, 0.035, 0.0),
        }
    }
}

impl MaterialExtension for Wind {
    fn vertex_shader() -> ShaderRef {
        "embedded://client/shaders/wind.wesl".into()
    }
}
