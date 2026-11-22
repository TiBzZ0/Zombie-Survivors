//! Bases protégées. Chaque joueur peut planter une bannière : en JcJ, les autres joueurs ne
//! peuvent ni casser, ni poser, ni ouvrir quoi que ce soit dans un rayon de 12 m autour, et
//! le propriétaire n'y prend aucun coup des autres joueurs. Les zombies, eux, s'en moquent :
//! une bannière abattue libère la base.

use std::collections::HashMap;

use bevy::prelude::*;
use bevy_renet::RenetServer;
use shared::block::{Block, CLAIM_RADIUS, in_claim};
use shared::protocol::ServerMessage;

use crate::net::broadcast;
use crate::world::GameWorld;

pub struct ClaimsPlugin;

impl Plugin for ClaimsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Claims>().add_systems(FixedUpdate, prune_claims);
    }
}

/// Bannières plantées et pseudo de leur propriétaire.
#[derive(Resource, Default, Debug, Clone)]
pub struct Claims(pub HashMap<IVec3, String>);

/// Pourquoi une bannière ne peut pas être plantée.
#[derive(Debug, PartialEq, Eq)]
pub enum ClaimRefusal {
    AlreadyOwns(IVec3),
    TooClose(String),
}

impl Claims {
    /// Propriétaire d'une base qui contient `pos` et n'est pas à `name`, s'il y en a une.
    pub fn foreign_owner(&self, pos: IVec3, name: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(banner, owner)| owner.as_str() != name && in_claim(**banner, pos))
            .map(|(_, owner)| owner.as_str())
    }

    /// Le joueur `name` est-il chez lui en `pos` ?
    pub fn at_home(&self, pos: IVec3, name: &str) -> bool {
        self.0
            .iter()
            .any(|(banner, owner)| owner == name && in_claim(*banner, pos))
    }

    /// Une bannière peut-elle être plantée ici par `name` ? Une par joueur, et deux bases
    /// ne se chevauchent pas.
    pub fn can_plant(&self, pos: IVec3, name: &str) -> Result<(), ClaimRefusal> {
        if let Some((banner, _)) = self.0.iter().find(|(_, owner)| owner.as_str() == name) {
            return Err(ClaimRefusal::AlreadyOwns(*banner));
        }
        let min = 2.0 * CLAIM_RADIUS;
        if let Some((_, owner)) = self.0.iter().find(|(banner, _)| {
            let d = (pos - **banner).as_vec3();
            d.x * d.x + d.z * d.z < min * min
        }) {
            return Err(ClaimRefusal::TooClose(owner.clone()));
        }
        Ok(())
    }

    pub fn message(&self) -> ServerMessage {
        let mut list: Vec<_> = self.0.iter().map(|(p, n)| (*p, n.clone())).collect();
        list.sort_by_key(|(p, _)| (p.x, p.y, p.z));
        ServerMessage::Claims(list)
    }
}

/// Une bannière cassée, explosée ou effondrée libère sa base.
fn prune_claims(world: Res<GameWorld>, mut claims: ResMut<Claims>, mut server: Option<ResMut<RenetServer>>) {
    let before = claims.0.len();
    claims.0.retain(|pos, _| world.block(*pos) == Block::Banner);
    if claims.0.len() != before
        && let Some(server) = server.as_deref_mut()
    {
        broadcast(server, &claims.message());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_banner_protects_its_surroundings_from_others_only() {
        let mut claims = Claims::default();
        claims.0.insert(IVec3::new(100, 40, 100), "Alice".into());
        let wall = IVec3::new(108, 70, 95);
        assert_eq!(claims.foreign_owner(wall, "Bob"), Some("Alice"));
        assert_eq!(claims.foreign_owner(wall, "Alice"), None);
        assert!(claims.at_home(wall, "Alice"));
        assert!(!claims.at_home(wall, "Bob"));
        assert_eq!(claims.foreign_owner(IVec3::new(113, 40, 100), "Bob"), None);
    }

    #[test]
    fn one_banner_per_player_and_no_overlap() {
        let mut claims = Claims::default();
        claims.0.insert(IVec3::new(100, 40, 100), "Alice".into());
        assert_eq!(
            claims.can_plant(IVec3::new(300, 40, 100), "Alice"),
            Err(ClaimRefusal::AlreadyOwns(IVec3::new(100, 40, 100)))
        );
        assert_eq!(
            claims.can_plant(IVec3::new(120, 40, 100), "Bob"),
            Err(ClaimRefusal::TooClose("Alice".into()))
        );
        assert_eq!(claims.can_plant(IVec3::new(125, 40, 100), "Bob"), Ok(()));
    }
}
