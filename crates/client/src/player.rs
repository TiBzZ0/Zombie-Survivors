//! Le joueur local : vue à la première personne, déplacements avec collisions,
//! minage, pose de blocs, inventaire et barre d'objets.

use std::f32::consts::FRAC_PI_2;

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions};
use bevy_renet::RenetClient;
use shared::block::Block;
use shared::item::{Inventory, Item, break_time};
use shared::physics::{GRAVITY, PLAYER_EYE_HEIGHT, PLAYER_SIZE, overlaps_solid, step_body};
use shared::protocol::{ClientMessage, PlayerState, REACH};
use shared::raycast::{ray_aabb, raycast};
use shared::survivor::TALK_RANGE;

use crate::AppState;
use crate::animals::{AnimalView, aimed_animal};
use crate::net::{ClientWorld, NetEvent, Session, send_reliable, send_state};
use crate::remote::RemotePlayer;
use crate::settings::Settings;
use crate::survivors::{SurvivorView, aimed_survivor};
use crate::zombies::ZombieView;

const WALK_SPEED: f32 = 4.5;
const SPRINT_SPEED: f32 = 7.0;
const JUMP_SPEED: f32 = 8.5;
/// Brasse tranquille.
const SWIM_SPEED: f32 = 2.8;
/// Crawl (Maj) : plus rapide.
const CRAWL_SPEED: f32 = 4.6;
/// Vitesse verticale sous l'eau (Espace pour monter, Ctrl pour descendre).
const DIVE_SPEED: f32 = 3.2;
/// Vitesse de remontée pour se hisser sur la rive.
const SWIM_UP_SPEED: f32 = 5.5;
/// Dans l'eau jusqu'aux genoux : on marche un peu moins vite.
const WADE_FACTOR: f32 = 0.8;
const MOUSE_SENSITIVITY: f32 = 0.0025;
/// Délai entre deux poses de blocs quand le bouton reste enfoncé.
const ACTION_COOLDOWN: f32 = 0.22;
/// Délai entre deux lancers d'explosif (le serveur applique le même).
const THROW_COOLDOWN: f32 = 0.8;
/// Délai après avoir mangé.
const EAT_COOLDOWN: f32 = 0.6;

/// Nombre de cases de la barre d'objets (touches 1 à 9).
pub const HOTBAR_SLOTS: usize = 9;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LocalPlayer>()
            .add_systems(OnEnter(AppState::Playing), spawn_player)
            .add_systems(
                Update,
                (
                    grab_cursor,
                    look,
                    apply_teleports,
                    select_slot,
                    move_player,
                    interact,
                    update_camera,
                    send_position,
                )
                    .chain()
                    .run_if(in_state(AppState::Playing))
                    .run_if(resource_exists::<ClientWorld>),
            );
    }
}

/// État du joueur local.
#[derive(Resource, Debug, Default)]
pub struct LocalPlayer {
    pub health: f32,
    pub hunger: f32,
    pub thirst: f32,
    /// Température du corps, en °C.
    pub body_temp: f32,
    /// Air ressenti, en °C.
    pub felt: f32,
    /// Avancée de l'infection (0 = sain).
    pub infection: f32,
    pub inventory: Inventory,
    /// Objets rangés dans la barre, dans l'ordre des touches.
    pub hotbar: [Option<Item>; HOTBAR_SLOTS],
    /// Bloc en cours de minage et progression (0 à 1).
    pub mining: Option<(IVec3, f32)>,
    /// Vrai si le bloc visé demande une pioche.
    pub needs_pickaxe: bool,
    /// Récoltes restantes des filons entamés, d'après le serveur.
    pub deposits: std::collections::HashMap<IVec3, u8>,
    /// Le panneau de fabrication est ouvert (le curseur est libre).
    pub crafting_open: bool,
    /// Coffre ouvert et son contenu (le curseur est libre).
    pub chest: Option<(IVec3, Inventory)>,
    /// Le joueur écrit dans la discussion (le clavier ne déplace plus le joueur).
    pub typing: bool,
    held_sent: Option<Option<Item>>,
    /// Cause de la mort tant que le joueur n'a pas réapparu.
    pub dead: Option<String>,
    /// Intensité de l'effet « touché » à l'écran (0 à 1).
    pub hurt_flash: f32,
    pub position: Vec3,
    pub velocity: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
    pub selected: usize,
    /// Bloc visé, s'il y en a un à portée.
    pub target: Option<shared::raycast::RayHit>,
    /// Eau visée, à portée pour boire.
    pub aimed_water: Option<IVec3>,
    /// Le joueur nage (en surface ou sous l'eau).
    pub swimming: bool,
    /// Position du corps par rapport à l'eau.
    pub immersion: shared::swim::Immersion,
    /// Souffle restant sous l'eau, en secondes (le serveur fait foi).
    pub breath: f32,
    /// Survivant visé, assez près pour lui parler.
    pub aimed_survivor: Option<u32>,
    /// Indication à afficher sur le survivant visé.
    pub survivor_hint: Option<String>,
    /// Garde levée (arme de mêlée en main, clic droit maintenu).
    pub guarding: bool,
    /// Arme fixe pilotée (mitrailleuse, canon, catapulte).
    pub mounted: Option<IVec3>,
    /// Intensité de l'effet « paré ! » (0 à 1).
    pub parry_flash: f32,
    /// Effets temporaires actifs (café, bière...).
    pub buffs: shared::buff::Buffs,
    /// Expérience et compétences, envoyées par le serveur.
    pub progress: shared::skills::Progress,
    /// Arbre de compétences ouvert (touche K).
    pub skills_open: bool,
    /// Objets déjà vus dans l'inventaire : seuls les nouveaux venus prennent une case libre
    /// de la barre (une case vidée à la main le reste).
    known: std::collections::BTreeSet<Item>,
    cooldown: f32,
    last_sent: f64,
}

impl LocalPlayer {
    pub fn eye(&self) -> Vec3 {
        self.position + Vec3::Y * PLAYER_EYE_HEIGHT
    }

    pub fn look_direction(&self) -> Vec3 {
        Quat::from_euler(EulerRot::YXZ, self.yaw, self.pitch, 0.0) * Vec3::NEG_Z
    }

    /// Un panneau (fabrication, coffre) libère la souris.
    pub fn panel_open(&self) -> bool {
        self.crafting_open || self.chest.is_some() || self.skills_open
    }

    /// Effets des compétences apprises.
    pub fn skills(&self) -> shared::skills::SkillEffects {
        self.progress.effects()
    }

    /// Santé maximale, compétences comprises.
    pub fn max_health(&self) -> f32 {
        self.skills().max_health
    }

    /// Objet de la case sélectionnée, s'il en reste.
    pub fn held(&self) -> Option<Item> {
        self.hotbar[self.selected].filter(|item| self.inventory.has(*item, 1))
    }

    /// Range les nouveaux objets dans les cases libres et vide celles des objets épuisés.
    fn sync_hotbar(&mut self) {
        for slot in self.hotbar.iter_mut() {
            if slot.is_some_and(|item| !self.inventory.has(item, 1)) {
                *slot = None;
            }
        }
        let missing: Vec<Item> = self
            .inventory
            .iter()
            .map(|(item, _)| item)
            .filter(|item| !self.hotbar.contains(&Some(*item)) && !self.known.contains(item))
            .collect();
        for item in missing {
            if let Some(slot) = self.hotbar.iter_mut().find(|s| s.is_none()) {
                *slot = Some(item);
            }
        }
        self.known = self.inventory.iter().map(|(item, _)| item).collect();
    }

    /// Range un objet dans une case de la barre (il quitte sa case précédente).
    pub fn assign_hotbar(&mut self, slot: usize, item: Item) {
        for s in self.hotbar.iter_mut() {
            if *s == Some(item) {
                *s = None;
            }
        }
        if let Some(s) = self.hotbar.get_mut(slot) {
            *s = Some(item);
        }
    }
}

fn spawn_player(mut player: ResMut<LocalPlayer>, session: Res<Session>) {
    *player = LocalPlayer {
        health: 100.0,
        hunger: 100.0,
        thirst: 100.0,
        breath: shared::swim::BREATH_SECS,
        body_temp: shared::climate::BODY_NORMAL,
        felt: 18.0,
        position: session.spawn,
        yaw: -std::f32::consts::FRAC_PI_4,
        ..default()
    };
}

fn cursor_locked(cursor: &CursorOptions) -> bool {
    cursor.grab_mode != CursorGrabMode::None
}

fn grab_cursor(
    mut cursor: Single<&mut CursorOptions>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut player: ResMut<LocalPlayer>,
    mut client: ResMut<RenetClient>,
) {
    if player.typing {
        return;
    }
    let was_open = player.panel_open();
    if keys.just_pressed(KeyCode::KeyC) || keys.just_pressed(KeyCode::Tab) {
        player.skills_open = false;
        if player.chest.is_some() {
            close_chest(&mut player, &mut client);
        } else {
            player.crafting_open = !player.crafting_open;
        }
    }
    if keys.just_pressed(KeyCode::KeyK) {
        close_chest(&mut player, &mut client);
        player.crafting_open = false;
        player.skills_open = !player.skills_open;
    }
    if keys.just_pressed(KeyCode::Escape) {
        let any_panel = player.panel_open();
        player.crafting_open = false;
        player.skills_open = false;
        close_chest(&mut player, &mut client);
        if !any_panel {
            cursor.visible = true;
            cursor.grab_mode = CursorGrabMode::None;
            return;
        }
    }
    let open = player.panel_open();
    if open {
        cursor.visible = true;
        cursor.grab_mode = CursorGrabMode::None;
    } else if was_open || (mouse.just_pressed(MouseButton::Left) && !cursor_locked(&cursor)) {
        cursor.visible = false;
        cursor.grab_mode = CursorGrabMode::Locked;
    }
}

fn close_chest(player: &mut LocalPlayer, client: &mut RenetClient) {
    if player.chest.take().is_some() {
        send_reliable(client, &ClientMessage::CloseChest);
    }
}

fn look(
    motion: Res<AccumulatedMouseMotion>,
    cursor: Single<&CursorOptions>,
    settings: Res<Settings>,
    mut player: ResMut<LocalPlayer>,
) {
    if !cursor_locked(&cursor) {
        return;
    }
    let k = MOUSE_SENSITIVITY * settings.sensitivity;
    player.yaw -= motion.delta.x * k;
    player.pitch = (player.pitch - motion.delta.y * k).clamp(-FRAC_PI_2 + 0.01, FRAC_PI_2 - 0.01);
}

fn select_slot(
    keys: Res<ButtonInput<KeyCode>>,
    scroll: Res<AccumulatedMouseScroll>,
    mut player: ResMut<LocalPlayer>,
    mut client: ResMut<RenetClient>,
) {
    const KEYS: [KeyCode; HOTBAR_SLOTS] = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
    ];
    for (i, key) in KEYS.iter().enumerate() {
        if keys.just_pressed(*key) && !player.typing {
            player.selected = i;
        }
    }
    if !player.panel_open() && scroll.delta.y != 0.0 {
        let step = if scroll.delta.y > 0.0 { HOTBAR_SLOTS - 1 } else { 1 };
        player.selected = (player.selected + step) % HOTBAR_SLOTS;
    }
    // Le serveur connaît l'objet en main (dégâts, vitesse de minage).
    let held = player.held();
    if player.held_sent != Some(held) {
        player.held_sent = Some(held);
        player.mining = None;
        send_reliable(&mut client, &ClientMessage::Hold { item: held });
    }
}

fn apply_teleports(
    mut events: MessageReader<NetEvent>,
    mut player: ResMut<LocalPlayer>,
    keys: Res<ButtonInput<KeyCode>>,
    mut client: ResMut<RenetClient>,
    time: Res<Time>,
) {
    player.hurt_flash = (player.hurt_flash - time.delta_secs() * 2.0).max(0.0);
    player.parry_flash = (player.parry_flash - time.delta_secs() * 3.0).max(0.0);
    for event in events.read() {
        match event {
            NetEvent::Parried => player.parry_flash = 1.0,
            NetEvent::Teleport(position) => {
                player.position = *position;
                player.velocity = Vec3::ZERO;
            }
            NetEvent::Buffs(buffs) => {
                player.buffs = buffs.clone();
            }
            NetEvent::Progress(progress) => {
                player.progress = progress.clone();
            }
            NetEvent::Vitals {
                health,
                hunger,
                thirst,
                body_temp,
                felt,
                infection,
                breath,
            } => {
                player.breath = *breath;
                player.body_temp = *body_temp;
                player.felt = *felt;
                player.infection = *infection;
                if *health < player.health - 0.5 {
                    player.hurt_flash = 1.0;
                }
                player.health = *health;
                player.hunger = *hunger;
                player.thirst = *thirst;
                if *health > 0.0 {
                    player.dead = None;
                }
            }
            NetEvent::Chest { pos, contents } => {
                player.chest = Some((*pos, contents.clone()));
            }
            NetEvent::ChestClosed => {
                player.chest = None;
            }
            NetEvent::Deposit { pos, remaining } => {
                player.deposits.insert(*pos, *remaining);
            }
            NetEvent::Inventory(inventory) => {
                player.inventory = inventory.clone();
                player.sync_hotbar();
            }
            NetEvent::Died(cause) => {
                player.dead = Some(cause.clone());
                player.health = 0.0;
            }
            _ => {}
        }
    }
    if player.dead.is_some() && keys.just_pressed(KeyCode::Enter) {
        send_reliable(&mut client, &ClientMessage::Respawn);
    }
}

fn move_player(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    world: Res<ClientWorld>,
    mut player: ResMut<LocalPlayer>,
) {
    if player.dead.is_some() {
        return;
    }
    // En train d'écrire : le joueur s'arrête (la gravité continue).
    let keys_active = !player.typing;
    let pressed = |k: KeyCode| keys_active && keys.pressed(k);
    // Les touches sont physiques : KeyW est la touche Z sur un clavier AZERTY.
    let mut wish = Vec3::ZERO;
    if pressed(KeyCode::KeyW) {
        wish.z -= 1.0;
    }
    if pressed(KeyCode::KeyS) {
        wish.z += 1.0;
    }
    if pressed(KeyCode::KeyA) {
        wish.x -= 1.0;
    }
    if pressed(KeyCode::KeyD) {
        wish.x += 1.0;
    }
    let speed = if pressed(KeyCode::ShiftLeft) {
        SPRINT_SPEED
    } else {
        WALK_SPEED
    };
    // Garde levée : on avance à petits pas ; aux commandes d'une arme fixe, on ne bouge plus.
    let speed = if player.mounted.is_some() {
        0.0
    } else if player.guarding {
        WALK_SPEED * 0.5
    } else {
        speed
    };
    // Dans l'eau : on marche ralenti dans les hauts-fonds, on nage en surface (la tête dehors),
    // on plonge avec Ctrl et on nage alors dans la direction du regard.
    use shared::swim::{FLOAT_HEIGHT, Immersion, immersion, surface_above};
    let water = immersion(player.position, |p| world.block(p));
    player.immersion = water;
    player.swimming = water.swimming();
    let skills = player.skills();
    let health_part = player.health / skills.max_health;
    let boost = player.buffs.effects().speed * skills.move_speed(health_part);
    let dt = time.delta_secs().min(0.05);
    let mut velocity = player.velocity;
    if player.swimming && player.mounted.is_none() {
        let crawl = pressed(KeyCode::ShiftLeft);
        let speed = if crawl { CRAWL_SPEED } else { SWIM_SPEED } * skills.swim_speed * boost;
        let up = pressed(KeyCode::Space);
        let down = pressed(KeyCode::ControlLeft) || pressed(KeyCode::ControlRight);
        // Sous l'eau, on avance là où l'on regarde ; en surface, à plat.
        let pitch = if water == Immersion::Under { player.pitch } else { 0.0 };
        let forward = Quat::from_euler(EulerRot::YXZ, player.yaw, pitch, 0.0) * Vec3::NEG_Z;
        let right = Quat::from_rotation_y(player.yaw) * Vec3::X;
        let mut target = (forward * -wish.z + right * wish.x).normalize_or_zero() * speed;
        let surface = surface_above(player.position, |p| world.block(p));
        target.y += if up {
            DIVE_SPEED
        } else if down {
            -DIVE_SPEED
        } else if let Some(top) = surface.filter(|_| water == Immersion::Surface) {
            // Flotte : le menton reste à fleur d'eau, avec un léger roulis.
            let bob = (time.elapsed_secs() * 2.2).sin() * 0.06;
            ((top - FLOAT_HEIGHT + bob - player.position.y) * 4.0).clamp(-2.0, 2.0)
        } else {
            // Les poumons pleins font doucement remonter.
            0.5
        };
        // Se hisser sur la rive : en surface, en poussant vers un bord d'un bloc.
        if water != Immersion::Under && wish.z < 0.0 {
            let ahead = player.position + Quat::from_rotation_y(player.yaw) * Vec3::NEG_Z * 0.6;
            let bank = (ahead + Vec3::Y * 1.0).floor().as_ivec3();
            if world.is_solid(bank) && !world.is_solid(bank + IVec3::Y) && !world.is_solid(bank + IVec3::Y * 2) {
                target.y = SWIM_UP_SPEED;
            }
        }
        // L'eau freine : la vitesse rejoint la cible en douceur (et compense la gravité).
        let k = (6.0 * dt).min(1.0);
        velocity = velocity.lerp(target, k);
        velocity.y += GRAVITY * dt;
    } else {
        let speed = if water == Immersion::Wading {
            speed * WADE_FACTOR
        } else {
            speed
        } * boost;
        let wish = Quat::from_rotation_y(player.yaw) * wish.normalize_or_zero() * speed;
        let control = if player.on_ground { 1.0 } else { 0.25 };
        let horizontal = Vec3::new(velocity.x, 0.0, velocity.z);
        let target = Vec3::new(wish.x, 0.0, wish.z);
        let new_horizontal = horizontal.lerp(target, (control * 15.0 * dt).min(1.0));
        velocity = Vec3::new(new_horizontal.x, velocity.y, new_horizontal.z);
        if player.on_ground && pressed(KeyCode::Space) {
            velocity.y = JUMP_SPEED * skills.jump.sqrt();
        }
    }

    // Tant que le terrain sous le joueur n'est pas connu, il ne tombe pas dans le vide.
    let solid = |p: IVec3| world.is_solid(p) || !world.in_bounds(p) && p.y < 0;
    let result = step_body(player.position, velocity, PLAYER_SIZE, dt, &solid);
    player.position = result.position;
    player.velocity = result.velocity;
    player.on_ground = result.on_ground;

    // Tombé sous la carte : retour en surface.
    if player.position.y < -20.0 {
        let p = player.position.as_ivec3();
        let ground = world.generator().surface_height(p.x, p.z);
        player.position.y = ground as f32 + 2.0;
        player.velocity = Vec3::ZERO;
    }
}

fn interact(
    mouse: Res<ButtonInput<MouseButton>>,
    cursor: Single<&CursorOptions>,
    time: Res<Time>,
    mut world: ResMut<ClientWorld>,
    mut player: ResMut<LocalPlayer>,
    mut client: ResMut<RenetClient>,
    mut changes: MessageWriter<NetEvent>,
    zombies: Query<(&ZombieView, &Transform)>,
    survivors: Query<(&SurvivorView, &Transform)>,
    animals: Query<(&AnimalView, &Transform)>,
    keys: Res<ButtonInput<KeyCode>>,
    session: Option<Res<Session>>,
    remotes: Query<(&RemotePlayer, &Transform)>,
) {
    let pvp = session.is_some_and(|s| s.pvp);
    let dt = time.delta_secs();
    player.target = raycast(player.eye(), player.look_direction(), REACH, |p| {
        world.block(p).is_targetable()
    });
    // L'eau ne se vise pas comme un bloc (on pose et on casse à travers), mais on y boit.
    player.aimed_water = raycast(player.eye(), player.look_direction(), REACH, |p| {
        let b = world.block(p);
        b.is_targetable() || b.is_water()
    })
    .map(|h| h.block)
    .filter(|p| world.block(*p).is_water());
    let block_dist = player
        .target
        .map(|h| (h.block.as_vec3() + Vec3::splat(0.5)).distance(player.eye()))
        .unwrap_or(f32::MAX);
    player.aimed_survivor = aimed_survivor(player.eye(), player.look_direction(), TALK_RANGE, &survivors)
        .filter(|(_, d)| *d < block_dist)
        .map(|(id, _)| id);
    player.cooldown -= dt;
    player.needs_pickaxe = false;
    // Arme fixe pilotée : le clic gauche tire, le clic droit fait descendre.
    if let Some(pos) = player.mounted {
        let gun = world.block(pos).emplacement();
        let close = emplacement_close(&player, pos);
        let leave = cursor_locked(&cursor) && mouse.just_pressed(MouseButton::Right);
        match gun {
            Some(gun) if close && player.dead.is_none() && !leave => {
                player.mining = None;
                let fire = cursor_locked(&cursor) && mouse.pressed(MouseButton::Left) && player.cooldown <= 0.0;
                if fire && player.inventory.remove(gun.ammo, 1) {
                    let direction = player.look_direction();
                    send_reliable(&mut client, &ClientMessage::FireEmplacement { pos, direction });
                    player.cooldown = gun.cooldown;
                }
                return;
            }
            _ => {
                player.mounted = None;
                if leave {
                    player.cooldown = 0.35;
                    return;
                }
            }
        }
    }
    // Garde : clic droit maintenu avec une arme de mêlée, sauf pour parler, ouvrir une porte ou un coffre.
    let interactable = player.aimed_survivor.is_some()
        || player.target.is_some_and(|h| {
            let b = world.block(h.block);
            b.is_door() || b.is_storage() || b.emplacement().is_some()
        });
    let guard = cursor_locked(&cursor)
        && player.dead.is_none()
        && mouse.pressed(MouseButton::Right)
        && player.held().is_some_and(Item::is_melee_weapon)
        && (player.guarding || !interactable);
    if guard != player.guarding {
        player.guarding = guard;
        send_reliable(&mut client, &ClientMessage::Guard { active: guard });
    }
    if guard {
        player.mining = None;
        return;
    }
    if !cursor_locked(&cursor) || player.dead.is_some() || !mouse.pressed(MouseButton::Left) {
        player.mining = None;
    }
    if !cursor_locked(&cursor) || player.dead.is_some() {
        return;
    }

    if mouse.pressed(MouseButton::Left)
        && let Some(item) = player.held().filter(|i| i.throwable().is_some())
    {
        // Explosif en main : le clic gauche le lance.
        player.mining = None;
        if player.cooldown <= 0.0 && mouse.just_pressed(MouseButton::Left) && player.inventory.remove(item, 1) {
            let direction = player.look_direction();
            send_reliable(&mut client, &ClientMessage::Throw { direction });
            player.cooldown = THROW_COOLDOWN;
        }
        return;
    }

    if mouse.pressed(MouseButton::Left)
        && let Some(weapon) = player.held().and_then(Item::ranged)
    {
        // Arme à distance en main : le clic gauche tire.
        player.mining = None;
        if player.cooldown <= 0.0 && player.inventory.remove(weapon.ammo, 1) {
            let direction = player.look_direction();
            send_reliable(&mut client, &ClientMessage::Shoot { direction });
            player.cooldown = weapon.cooldown;
        }
        return;
    }

    if mouse.pressed(MouseButton::Left) {
        // Un zombie visé passe avant les blocs.
        let block_dist = player
            .target
            .map(|h| (h.block.as_vec3() + Vec3::splat(0.5)).distance(player.eye()))
            .unwrap_or(f32::MAX);
        let melee = Item::melee(player.held());
        if let Some((id, dist)) = aimed_zombie(player.eye(), player.look_direction(), melee.reach, &zombies)
            && dist < block_dist
        {
            player.mining = None;
            if player.cooldown <= 0.0 {
                send_reliable(&mut client, &ClientMessage::Attack { zombie: id });
                player.cooldown = melee.cooldown;
            }
            return;
        }
        // En JcJ, les autres joueurs aussi.
        if pvp
            && let Some((id, dist)) = aimed_remote(player.eye(), player.look_direction(), melee.reach, &remotes)
            && dist < block_dist
        {
            player.mining = None;
            if player.cooldown <= 0.0 {
                send_reliable(&mut client, &ClientMessage::AttackPlayer { id });
                player.cooldown = melee.cooldown;
            }
            return;
        }
        // Le gibier aussi.
        if let Some((id, dist)) = aimed_animal(player.eye(), player.look_direction(), melee.reach, &animals)
            && dist < block_dist
        {
            player.mining = None;
            if player.cooldown <= 0.0 {
                send_reliable(&mut client, &ClientMessage::AttackAnimal { id });
                player.cooldown = melee.cooldown;
            }
            return;
        }
        mine(&mut player, &mut world, &mut client, &mut changes, dt);
        return;
    }

    if !mouse.pressed(MouseButton::Right) || player.cooldown > 0.0 {
        return;
    }
    // Survivants, portes et coffres passent avant l'objet en main.
    if let Some(id) = player.aimed_survivor
        && mouse.just_pressed(MouseButton::Right)
    {
        // Maj : renvoyer ; Ctrl : confier l'arme tenue.
        let dismiss = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
        let give = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
        send_reliable(&mut client, &ClientMessage::TalkToSurvivor { id, dismiss, give });
        player.cooldown = 0.35;
        return;
    }
    if let Some(hit) = player.target {
        let block = world.block(hit.block);
        if block.is_door() {
            send_reliable(&mut client, &ClientMessage::ToggleDoor { pos: hit.block });
            player.cooldown = 0.35;
            return;
        }
        if block.emplacement().is_some() && mouse.just_pressed(MouseButton::Right) {
            if emplacement_close(&player, hit.block) {
                player.mounted = Some(hit.block);
                player.cooldown = 0.35;
            }
            return;
        }
        if block == Block::RipeFruitLeaves && mouse.just_pressed(MouseButton::Right) {
            send_reliable(&mut client, &ClientMessage::Harvest { pos: hit.block });
            player.cooldown = 0.35;
            return;
        }
        if block.is_bed() && mouse.just_pressed(MouseButton::Right) {
            send_reliable(&mut client, &ClientMessage::Rest { pos: hit.block });
            player.cooldown = 0.35;
            return;
        }
        if block.is_storage() && mouse.just_pressed(MouseButton::Right) {
            send_reliable(&mut client, &ClientMessage::OpenChest { pos: hit.block });
            player.crafting_open = false;
            player.cooldown = 0.35;
            return;
        }
    }
    match player.held() {
        Some(item) if item.is_consumable() => {
            send_reliable(&mut client, &ClientMessage::Eat { item });
            player.cooldown = EAT_COOLDOWN;
        }
        Some(item @ (Item::Seeds | Item::CarrotSeeds)) => {
            // Semer : sur le dessus d'un bloc de terre ou d'herbe.
            let Some(hit) = player.target else { return };
            // Viser une touffe d'herbes hautes revient à viser le sol dessous.
            let (ground, normal) = if world.block(hit.block) == Block::TallGrass {
                (hit.block - IVec3::Y, IVec3::Y)
            } else {
                (hit.block, hit.normal)
            };
            let pos = ground + IVec3::Y;
            let soil = matches!(world.block(ground), Block::Grass | Block::Dirt);
            let crop = if item == Item::CarrotSeeds {
                Block::CarrotSprout
            } else {
                Block::Sprout
            };
            if normal == IVec3::Y && soil && matches!(world.block(pos), Block::Air | Block::TallGrass) {
                world.set_block(pos, crop);
                player.inventory.remove(item, 1);
                changes.write(NetEvent::BlockChanged(pos));
                send_reliable(&mut client, &ClientMessage::PlaceBlock { pos, block: crop });
                player.cooldown = ACTION_COOLDOWN;
            }
        }
        Some(Item::WaterBucket) => {
            let Some(hit) = player.target else { return };
            let pos = placement(&world, hit);
            if matches!(world.block(pos), Block::Air | Block::FlowingWater | Block::TallGrass) {
                send_reliable(&mut client, &ClientMessage::PourWater { pos });
                player.cooldown = ACTION_COOLDOWN;
            }
        }
        Some(Item::Block(block)) => {
            let Some(hit) = player.target else { return };
            let pos = placement(&world, hit);
            let blocks_me = block.is_solid() && overlaps_solid(player.position, PLAYER_SIZE, &|b| b == pos);
            if !blocks_me && world.in_bounds(pos) && world.block(pos).is_replaceable() {
                // Appliqué tout de suite pour la réactivité ; le serveur corrige s'il refuse.
                world.set_block(pos, block);
                player.inventory.remove(Item::Block(block), 1);
                changes.write(NetEvent::BlockChanged(pos));
                send_reliable(&mut client, &ClientMessage::PlaceBlock { pos, block });
                player.cooldown = ACTION_COOLDOWN;
            }
        }
        _ => {
            // Main libre ou gourde vide : boire à la source visée (ou remplir la gourde).
            if let Some(pos) = player.aimed_water
                && mouse.just_pressed(MouseButton::Right)
            {
                send_reliable(&mut client, &ClientMessage::Drink { pos });
                player.cooldown = EAT_COOLDOWN;
            }
        }
    }
}

/// Mine le bloc visé : la progression dépend du bloc et de l'outil en main.
fn mine(
    player: &mut LocalPlayer,
    world: &mut ClientWorld,
    client: &mut RenetClient,
    changes: &mut MessageWriter<NetEvent>,
    dt: f32,
) {
    let Some(hit) = player.target.filter(|h| h.block.y > 0) else {
        player.mining = None;
        return;
    };
    let block = world.block(hit.block);
    let Some(duration) = break_time(block, player.held()) else {
        player.needs_pickaxe = true;
        player.mining = None;
        return;
    };
    let progress = match player.mining {
        Some((pos, progress)) if pos == hit.block => progress,
        _ => {
            send_reliable(client, &ClientMessage::StartMining { pos: hit.block });
            0.0
        }
    };
    let speed = player.buffs.effects().mining * player.skills().mining(block);
    let progress = progress + dt * speed / duration.max(0.05);
    if progress < 1.0 {
        player.mining = Some((hit.block, progress));
        return;
    }
    send_reliable(client, &ClientMessage::BreakBlock { pos: hit.block });
    changes.write(NetEvent::BlockBroken { pos: hit.block, block });
    if block.is_deposit() {
        // Un filon reste en place : la récolte suivante commence aussitôt.
        player.mining = Some((hit.block, 0.0));
        return;
    }
    // Appliqué tout de suite pour la réactivité ; le serveur corrige s'il refuse.
    world.set_block(hit.block, Block::Air);
    changes.write(NetEvent::BlockChanged(hit.block));
    player.mining = None;
}

/// Case où poser un bloc visé : à côté du bloc, ou à sa place s'il se remplace (herbes hautes).
fn placement(world: &ClientWorld, hit: shared::raycast::RayHit) -> IVec3 {
    if world.block(hit.block) == Block::TallGrass {
        hit.block
    } else {
        hit.block + hit.normal
    }
}

fn update_camera(player: Res<LocalPlayer>, mut camera: Single<&mut Transform, With<Camera3d>>) {
    camera.translation = player.eye();
    camera.rotation = Quat::from_euler(EulerRot::YXZ, player.yaw, player.pitch, 0.0);
}

fn send_position(mut player: ResMut<LocalPlayer>, mut client: ResMut<RenetClient>, time: Res<Time>) {
    let state = PlayerState {
        position: player.position,
        yaw: player.yaw,
        pitch: player.pitch,
        swinging: player.mining.is_some() || player.cooldown > 0.0,
    };
    let mut last = player.last_sent;
    send_state(&mut client, state, &time, &mut last);
    player.last_sent = last;
}

/// Le joueur est assez près de l'arme fixe pour la piloter.
pub fn emplacement_close(player: &LocalPlayer, pos: IVec3) -> bool {
    (pos.as_vec3() + Vec3::splat(0.5)).distance(player.position + Vec3::Y) <= shared::block::EMPLACEMENT_REACH
}

/// Zombie visé (intersection rayon / boîte) à portée de mêlée, avec sa distance.
/// Autre joueur visé à portée, et sa distance.
fn aimed_remote(
    eye: Vec3,
    dir: Vec3,
    reach: f32,
    remotes: &Query<(&RemotePlayer, &Transform)>,
) -> Option<(shared::protocol::PlayerId, f32)> {
    let half = Vec3::new(PLAYER_SIZE.half_width + 0.1, 0.0, PLAYER_SIZE.half_width + 0.1);
    remotes
        .iter()
        .filter_map(|(r, t)| {
            ray_aabb(
                eye,
                dir,
                t.translation - half,
                t.translation + half + Vec3::Y * PLAYER_SIZE.height,
            )
            .filter(|d| *d <= reach)
            .map(|d| (r.id, d))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
}

fn aimed_zombie(eye: Vec3, dir: Vec3, reach: f32, zombies: &Query<(&ZombieView, &Transform)>) -> Option<(u32, f32)> {
    let mut best: Option<(u32, f32)> = None;
    for (view, transform) in zombies.iter() {
        let size = view.kind.stats().size;
        let min = transform.translation - Vec3::new(size.half_width + 0.1, 0.0, size.half_width + 0.1);
        let max = transform.translation + Vec3::new(size.half_width + 0.1, size.height, size.half_width + 0.1);
        if let Some(t) = ray_aabb(eye, dir, min, max)
            && t <= reach
            && best.is_none_or(|(_, d)| t < d)
        {
            best = Some((view.id, t));
        }
    }
    best
}
