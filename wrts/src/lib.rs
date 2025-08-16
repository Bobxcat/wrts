mod generated_bullet_problem_solution;
mod math_utils;
mod networking;
mod ship;
mod ui;

use std::{
    collections::{HashMap, HashSet},
    convert::identity,
    hash::RandomState,
};

use bevy::{
    input::{InputSystem, mouse::MouseWheel},
    prelude::*,
    window::PrimaryWindow,
};
use enum_map::EnumMap;
use itertools::Itertools;
use ordered_float::OrderedFloat;

use crate::{
    math_utils::BulletProblemRes,
    networking::NetworkingPlugin,
    ship::Ship,
    ui::{
        in_game::{InGameUIPlugin, InGameUIState},
        lobby::LobbyUiPlugin,
        main_menu::MainMenuUIPlugin,
    },
};

#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[states(scoped_entities)]
pub enum AppState {
    ConnectingToServer,
    LobbyMenu,
    MainMenu,
    InGame { paused: bool },
    PostGame,
}

/// Represents any `AppState::InGame`
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct InGameState;

impl ComputedStates for InGameState {
    type SourceStates = Option<AppState>;

    fn compute(sources: Option<AppState>) -> Option<Self> {
        match sources {
            Some(AppState::InGame { .. }) => Some(InGameState),
            _ => None,
        }
    }
}

const SHIP_SELECTION_SIZE: f32 = 20.;

#[derive(Resource)]
struct GameRules {
    gravity: f32,
}

impl Default for GameRules {
    fn default() -> Self {
        Self { gravity: 10. }
    }
}

struct TeamColors {
    pub ship_color: Color,
    pub gun_range_ring_color: Color,
}

#[derive(Resource)]
struct PlayerSettings {
    username: String,
    ship_icon_scale: f32,
    bullet_icon_scale: f32,
    team_friend_colors: TeamColors,
    team_enemy_colors: TeamColors,
}

impl Default for PlayerSettings {
    fn default() -> Self {
        Self {
            username: "Username".into(),
            ship_icon_scale: 30.,
            bullet_icon_scale: 5.,
            team_friend_colors: TeamColors {
                ship_color: Color::linear_rgb(0., 0.2, 0.7),
                gun_range_ring_color: Color::linear_rgb(0.2, 0.2, 0.8),
            },
            team_enemy_colors: TeamColors {
                ship_color: Color::linear_rgb(0.7, 0.2, 0.),
                gun_range_ring_color: Color::linear_rgb(0.8, 0.2, 0.2),
            },
        }
    }
}

impl PlayerSettings {
    pub fn team_colors(&self, team: Team) -> &TeamColors {
        match team {
            Team::Friend => &self.team_friend_colors,
            Team::Enemy => &self.team_enemy_colors,
        }
    }
}

#[derive(Resource, Default)]
struct CursorWorldPos(Vec2);

#[derive(Component)]
struct MainCamera;

fn update_cursor_world_pos(
    mut cursor_pos: ResMut<CursorWorldPos>,
    q_window: Query<&Window, With<PrimaryWindow>>,
    q_camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
) {
    let (camera, camera_transform) = q_camera.single().unwrap();
    let window = q_window.single().unwrap();
    if let Some(world_position) = window
        .cursor_position()
        .and_then(|cursor| camera.viewport_to_world(camera_transform, cursor).ok())
        .map(|ray| ray.origin.truncate())
    {
        cursor_pos.0 = world_position;
    }
}

#[derive(Debug, Default, Component, Clone, Copy)]
#[require(Transform)]
struct Velocity(pub Vec3);

#[derive(Debug, Default, Component, Clone)]
struct MoveOrder {
    pub waypoints: Vec<Vec2>,
}

// struct RudderPos

#[derive(Debug, Component, Clone)]
struct FireTarget {
    ship: Entity,
}

#[derive(Debug, Component, Default, Clone)]
struct Health(pub f64);

#[derive(Debug, Default, Component, Clone, Copy)]
struct Selected;

/// NOT A COMPONENT
#[derive(Debug, Clone, Copy)]
pub enum DetectionStatus {
    Detected,
    NoLonger(NoLongerDetected),
    Never,
}

impl DetectionStatus {
    pub fn from_options(detected: Option<&Detected>, no_longer: Option<&NoLongerDetected>) -> Self {
        match (detected, no_longer) {
            (Some(_), None) => Self::Detected,
            (_, Some(n)) => Self::NoLonger(*n),
            (None, None) => Self::Never,
        }
    }

    pub fn is_detected(&self) -> bool {
        match self {
            Self::Detected => true,
            _ => false,
        }
    }

    pub fn is_no_longer(&self) -> bool {
        match self {
            Self::NoLonger(_) => true,
            _ => false,
        }
    }

    pub fn is_never(&self) -> bool {
        match self {
            Self::Never => true,
            _ => false,
        }
    }
}

/// Currently detected
#[derive(Debug, Default, Component, Clone, Copy)]
pub struct Detected;

/// Has been detected before, but isn't currently
#[derive(Debug, Default, Component, Clone, Copy)]
pub struct NoLongerDetected {
    pub last_known: Transform,
}

/// The ghost of a once-detected ship
#[derive(Debug, Component, Clone, Copy)]
#[require(Health, Transform, Sprite)]
pub struct ShipGhost {
    pub owner: Entity,
}

#[derive(Debug, Default, Resource, Clone, Copy)]
struct MapZoom(pub f32);

#[derive(Debug, Default, Component, Clone, Copy, PartialEq, Eq, enum_map::Enum)]
enum Team {
    #[default]
    Friend,
    Enemy,
}

impl Team {
    pub fn opposite(self) -> Self {
        match self {
            Team::Friend => Team::Enemy,
            Team::Enemy => Team::Friend,
        }
    }

    pub fn is_friend(self) -> bool {
        self == Self::Friend
    }

    pub fn is_enemy(self) -> bool {
        self == Self::Enemy
    }
}

fn apply_velocity(q: Query<(&mut Transform, &Velocity)>, time: Res<Time>) {
    for (mut trans, vel) in q {
        trans.translation += vel.0 * time.delta_secs();
    }
}

#[derive(Debug, Component, Clone)]
#[require(Team, Sprite, Transform, Velocity)]
struct Bullet {
    owning_ship: Entity,
    damage: f64,
}

fn move_bullets(
    mut commands: Commands,
    q: Query<(Entity, &Bullet, &Transform, &mut Velocity)>,
    rules: Res<GameRules>,
    time: Res<Time>,
) {
    for (entity, _bullet, trans, mut bullet_vel) in q {
        bullet_vel.0.z -= rules.gravity * time.delta_secs();
        if trans.translation.z <= -100. {
            commands.entity(entity).despawn();
        }
    }
}

fn collide_bullets(
    mut commands: Commands,
    bullets: Query<(Entity, &Bullet, &Transform, &Team)>,
    mut ships: Query<(Entity, &Ship, &Transform, &Team, &mut Health)>,
) {
    for (bullet_entity, bullet, bullet_trans, bullet_team) in bullets {
        for (ship_entity, _ship, ship_trans, ship_team, mut ship_health) in &mut ships {
            if bullet_team == ship_team {
                continue;
            }
            if ship_trans.translation.distance(bullet_trans.translation) <= 10. {
                if ship_health.0 <= 0. {
                    continue;
                }
                ship_health.0 -= bullet.damage;
                commands.entity(bullet_entity).despawn();
                if ship_health.0 <= 0. {
                    commands.entity(ship_entity).despawn();
                    break;
                }
            }
        }
    }
}

fn update_detected_ships(
    mut commands: Commands,
    ships: Query<(
        Entity,
        &Ship,
        &Team,
        &Transform,
        Option<&Detected>,
        Option<&NoLongerDetected>,
    )>,
) {
    for ship in &ships {
        let mut entity = commands.entity(ship.0);
        let detection_last_frame = DetectionStatus::from_options(ship.4, ship.5);
        let is_detected = ships.iter().any(|other_ship| {
            other_ship.2.opposite() == *ship.2
                && other_ship
                    .3
                    .translation
                    .truncate()
                    .distance(ship.3.translation.truncate())
                    <= ship.1.detection
        });
        if is_detected {
            entity.insert(Detected);
            if detection_last_frame.is_no_longer() {
                entity.remove::<NoLongerDetected>();
            }
        }
        if !is_detected && detection_last_frame.is_detected() {
            entity.insert(NoLongerDetected {
                last_known: *ship.3,
            });
            entity.remove::<Detected>();
        }
    }
}

fn update_ship_ghosts(
    mut commands: Commands,
    ships: Query<(Entity, Option<&Detected>, Option<&NoLongerDetected>, &Team), With<Ship>>,

    mut current_ghosts: Local<HashMap<Entity, Entity>>,
) {
    for ship in ships {
        if ship.3.is_friend() {
            continue;
        }
        let detection_status = DetectionStatus::from_options(ship.1, ship.2);
        match detection_status {
            DetectionStatus::NoLonger(no_longer_detected)
                if !current_ghosts.contains_key(&ship.0) =>
            {
                current_ghosts.insert(
                    ship.0,
                    commands
                        .spawn((
                            StateScoped(InGameState),
                            ShipGhost { owner: ship.0 },
                            no_longer_detected.last_known,
                        ))
                        .id(),
                );
            }
            DetectionStatus::Detected if current_ghosts.contains_key(&ship.0) => {
                commands
                    .entity(current_ghosts.remove(&ship.0).unwrap())
                    .despawn();
            }
            DetectionStatus::Never => assert!(!current_ghosts.contains_key(&ship.0)),
            _ => (),
        }
    }

    for (ship_entity, ghost_entity) in &current_ghosts {
        if !ships.contains(*ship_entity) {
            commands.entity(*ghost_entity).despawn();
        }
    }
}

fn update_ship_ghosts_display(
    mut commands: Commands,
    ghosts: Query<Entity, With<ShipGhost>>,
    settings: Res<PlayerSettings>,
    zoom: Res<MapZoom>,
) {
    for ghost in ghosts {
        let sprite_size = vec2(1., 1.) * settings.ship_icon_scale * zoom.0;
        commands.entity(ghost).insert(Sprite::from_color(
            Color::linear_rgb(0.8, 0.8, 0.7),
            sprite_size,
        ));
    }
}

fn update_ai_ship_targets(
    mut commands: Commands,

    ships: Query<(
        Entity,
        &Team,
        &mut Ship,
        &Transform,
        &Velocity,
        Option<&FireTarget>,
    )>,
) {
    let (friend_ships, enemy_ships) = ships
        .into_iter()
        .partition::<Vec<_>, _>(|&(_, &team, _, _, _, _)| team == Team::Friend);

    for ai_ship in enemy_ships {
        let targ = friend_ships.iter().min_by_key(|player_ship| {
            OrderedFloat(
                ai_ship
                    .3
                    .translation
                    .distance_squared(player_ship.3.translation),
            )
        });
        let Some(targ) = targ else {
            continue;
        };
        commands
            .entity(ai_ship.0)
            .insert(FireTarget { ship: targ.0 });
    }
}

fn update_ai_moves(mut commands: Commands, ships: Query<(Entity, &Transform, &Team), With<Ship>>) {
    for ship in ships {
        if ship.2.is_enemy() {
            commands.entity(ship.0).insert(MoveOrder {
                waypoints: vec![vec2(0., 0.)],
            });
        }
    }
}

fn fire_bullets(
    mut commands: Commands,
    ships: Query<(
        Entity,
        &Team,
        &mut Ship,
        &Transform,
        &Velocity,
        Option<&FireTarget>,
    )>,
    time: Res<Time>,
    rules: Res<GameRules>,
    settings: Res<PlayerSettings>,
) {
    let mut ships_by_team = EnumMap::from_iter({
        let (friend_ships, enemy_ships) = ships
            .into_iter()
            .partition::<Vec<_>, _>(|&(_, &team, _, _, _, _)| team == Team::Friend);
        [(Team::Friend, friend_ships), (Team::Enemy, enemy_ships)]
    });

    for (team, ship_idx, turret_idx) in [Team::Friend, Team::Enemy]
        .into_iter()
        .flat_map(|team| (0..ships_by_team[team].len()).map(move |idx| (team, idx)))
        .flat_map(|(team, ship_idx)| {
            (0..ships_by_team[team][ship_idx].2.turrets.len())
                .map(move |turret_idx| (team, ship_idx, turret_idx))
        })
        .collect_vec()
    {
        let (targ_trans, targ_vel) = {
            let targ = ships_by_team[team][ship_idx].5.and_then(|targ| {
                ships_by_team[team.opposite()]
                    .iter()
                    .find(|(ship, _, _, _, _, _)| *ship == targ.ship)
            });

            let Some((_, _, _, targ_trans, targ_vel, _)) = targ else {
                let turret = &mut ships_by_team[team][ship_idx].2.turrets[turret_idx];
                if !turret.reload_timer.finished() {
                    turret.reload_timer.tick(time.delta());
                }
                continue;
            };
            (targ_trans, targ_vel)
        };
        let targ_trans = **targ_trans;
        let targ_vel = **targ_vel;

        let (ship_entity, _ship_team, ship, ship_trans, _ship_vel, _ship_targ) =
            &mut ships_by_team[team][ship_idx];
        let turret = &mut ship.turrets[turret_idx];

        let origin_pos = ship_trans.translation.truncate()
            + Vec2::from_angle(ship_trans.rotation.to_euler(EulerRot::ZXY).0)
                .rotate(turret.location_on_ship);
        let targ_pos = targ_trans.translation.truncate();

        let Some(BulletProblemRes {
            intersection_point: _,
            intersection_time: _,
            intersection_dist,
            projectile_dir: bullet_dir,
            projectile_azimuth: bullet_azimuth,
            projectile_elevation: _,
        }) = math_utils::bullet_problem(
            origin_pos,
            targ_pos,
            targ_vel.0.truncate(),
            turret.muzzle_vel as f64,
            rules.gravity as f64,
        )
        else {
            if !turret.reload_timer.finished() {
                turret.reload_timer.tick(time.delta());
            }
            continue;
        };
        if intersection_dist >= turret.max_range {
            if !turret.reload_timer.finished() {
                turret.reload_timer.tick(time.delta());
            }
            continue;
        }

        for _ in 0..turret.reload_timer.times_finished_this_tick() {
            for barrel in &turret.barrels {
                let bullet_vel =
                    turret.dispersion.apply_dispersion(bullet_dir) * turret.muzzle_vel as f32;

                let bullet_start = origin_pos + Vec2::from_angle(bullet_azimuth).rotate(*barrel);
                let bullet_trans = Transform {
                    translation: bullet_start.extend(5.),
                    rotation: Quat::from_rotation_z(
                        std::f32::consts::FRAC_PI_2 + bullet_vel.truncate().to_angle(),
                    ),
                    ..default()
                };

                make_bullet(
                    commands.reborrow(),
                    *ship_entity,
                    bullet_trans,
                    bullet_vel,
                    turret.damage,
                    team,
                    &settings,
                );
            }
        }

        // We want the turret to remain reloaded or continue progressing its
        // reload when unable to fire, including when there is no target
        // If we consider the previous checks that the target is shootable,
        // placing the tick here accounts for the above
        turret.reload_timer.tick(time.delta());
    }
}

fn make_bullet(
    mut commands: Commands,
    owning_ship: Entity,
    trans: Transform,
    vel: Vec3,
    damage: f64,
    team: Team,
    settings: &PlayerSettings,
) {
    commands.spawn((
        StateScoped(InGameState),
        Bullet {
            owning_ship,
            damage,
        },
        trans,
        Velocity(vel),
        team,
        Sprite::from_color(settings.team_colors(team).ship_color, Vec2::ZERO),
    ));
}

fn update_bullet_displays(
    bullets: Query<(&Transform, &mut Sprite, &Team), With<Bullet>>,
    settings: Res<PlayerSettings>,
    zoom: Res<MapZoom>,
) {
    for (trans, mut sprite, &_team) in bullets {
        if trans.translation.z <= 0. {
            *sprite = Sprite::from_color(
                Color::linear_rgb(0., 0., 0.),
                sprite.custom_size.unwrap_or_default(),
            );
        }

        let double_height = 1000.;
        let height_scaling = 1. + trans.translation.z / double_height;
        sprite.custom_size =
            Some(vec2(0.5, 2.) * height_scaling * settings.bullet_icon_scale * zoom.0);
    }
}

fn make_ships(mut commands: Commands) {
    commands.spawn((
        StateScoped(InGameState),
        Ship::bismarck(),
        Health(10_000.),
        Team::Friend,
        Transform {
            translation: vec2(-300., 60.).extend(0.),
            ..Default::default()
        },
    ));

    commands.spawn((
        StateScoped(InGameState),
        Ship::oland(),
        Health(1000.),
        Team::Enemy,
        Transform {
            translation: vec2(8000., 120.).extend(0.),
            ..Default::default()
        },
    ));
}

fn make_camera(mut commands: Commands) {
    let mut proj = OrthographicProjection::default_2d();
    proj.scale = 10.;
    commands.spawn((
        Camera2d::default(),
        Projection::Orthographic(proj),
        MainCamera,
    ));
}

fn update_map_zoom(mut mouse_scroll: EventReader<MouseWheel>, mut zoom: ResMut<MapZoom>) {
    for scroll in mouse_scroll.read() {
        zoom.0 -= scroll.y;
    }
    zoom.0 = zoom.0.clamp(0.5, 1000.);
}

fn update_camera(
    mut camera: Query<(&mut Projection, &mut Transform), With<MainCamera>>,
    keys: Res<ButtonInput<KeyCode>>,
    zoom: Res<MapZoom>,
    time: Res<Time>,
) {
    let mut camera = camera.single_mut().unwrap();
    let Projection::Orthographic(proj) = &mut *camera.0 else {
        panic!()
    };

    proj.scale = zoom.0;
    let dir: Vec2 = [
        keys.pressed(KeyCode::KeyW).then_some(vec2(0., 1.)),
        keys.pressed(KeyCode::KeyA).then_some(vec2(-1., 0.)),
        keys.pressed(KeyCode::KeyS).then_some(vec2(0., -1.)),
        keys.pressed(KeyCode::KeyD).then_some(vec2(1., 0.)),
    ]
    .into_iter()
    .filter_map(identity)
    .sum();
    camera.1.translation += (dir * 200. * zoom.0 * time.delta_secs()).extend(0.);
}

fn update_ship_displays(
    mut gizmos: Gizmos,
    ships: Query<(
        &Ship,
        &mut Sprite,
        &Transform,
        &Team,
        Option<&Selected>,
        Option<&Detected>,
        Option<&NoLongerDetected>,
    )>,
    settings: Res<PlayerSettings>,
    zoom: Res<MapZoom>,
) {
    for (ship, mut sprite, trans, team, selected, detected, no_longer_detected) in ships {
        let is_selected = selected.is_some();
        let detection_status = DetectionStatus::from_options(detected, no_longer_detected);
        let sprite_size = vec2(1., 1.) * settings.ship_icon_scale * zoom.0;

        if team.is_enemy() && !detection_status.is_detected() {
            *sprite = Sprite::default();
            continue;
        } else {
            let dim = match is_selected {
                true => 0.7,
                false => 1.0,
            };
            *sprite = Sprite::from_color(
                Color::LinearRgba(settings.team_colors(*team).ship_color.to_linear() * dim)
                    .with_alpha(1.),
                sprite_size,
            );
        }

        if *team == Team::Friend || detection_status.is_detected() {
            // Gun range circle
            if let Some(t) = ship
                .turrets
                .iter()
                .max_by_key(|t| OrderedFloat(t.max_range))
            {
                gizmos
                    .circle_2d(
                        Isometry2d::from_translation(trans.translation.truncate()),
                        t.max_range,
                        settings.team_colors(*team).gun_range_ring_color,
                    )
                    .resolution(128);
            }

            gizmos
                .circle_2d(
                    Isometry2d::from_translation(trans.translation.truncate()),
                    ship.detection,
                    Color::linear_rgb(0.4, 0.4, 0.9),
                )
                .resolution(128);
        }
    }
}

fn update_selected_ship_orders(
    mut commands: Commands,
    mut gizmos: Gizmos,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mouse_pos: Res<CursorWorldPos>,
    all_ships: Query<(Entity, &Transform, &Team, Option<&NoLongerDetected>), With<Ship>>,
    mut ships_selected: Query<(
        Entity,
        &Transform,
        &Selected,
        &Ship,
        Option<&FireTarget>,
        Option<&mut MoveOrder>,
    )>,
    settings: Res<PlayerSettings>,
    zoom: Res<MapZoom>,
) {
    // Display
    for selected in &ships_selected {
        let circle_size = zoom.0 * settings.ship_icon_scale * 0.5 * 1.4;
        gizmos
            .circle_2d(
                Isometry2d::from_translation(selected.1.translation.truncate()),
                circle_size,
                Color::WHITE,
            )
            .resolution(10);
        if let Some(targ) = selected.4.and_then(|targ| all_ships.get(targ.ship).ok()) {
            let draw_pos = targ
                .3
                .map_or(*targ.1, |no_longer| no_longer.last_known)
                .translation
                .truncate();
            gizmos
                .circle_2d(
                    Isometry2d::from_translation(draw_pos),
                    circle_size,
                    Color::linear_rgb(0.8, 0.3, 0.3),
                )
                .resolution(10);
        }

        if let Some(move_order) = selected.5 {
            if !move_order.waypoints.is_empty() {
                gizmos.linestrip_2d(
                    std::iter::once(selected.1.translation.truncate())
                        .chain(move_order.waypoints.iter().copied()),
                    Color::WHITE,
                );
            }
        }
    }

    // Orders
    for ship in &mut ships_selected {
        if mouse.just_pressed(MouseButton::Left)
            && only_modifier_keys_pressed(&keyboard, [KeyCode::ControlLeft])
        {
            if let Some(new_targ) = all_ships.iter().find(|maybe_targ| {
                *maybe_targ.2 == Team::Enemy
                    && maybe_targ.1.translation.truncate().distance(mouse_pos.0)
                        <= SHIP_SELECTION_SIZE * zoom.0
            }) {
                commands
                    .entity(ship.0)
                    .insert(FireTarget { ship: new_targ.0 });
            }
        }
        if keyboard.just_pressed(KeyCode::KeyQ)
            && only_modifier_keys_pressed(&keyboard, [KeyCode::ControlLeft])
        {
            commands.entity(ship.0).remove::<FireTarget>();
        }

        if mouse.just_pressed(MouseButton::Left)
            && only_modifier_keys_pressed(&keyboard, [KeyCode::AltLeft])
        {
            commands.entity(ship.0).insert(MoveOrder {
                waypoints: vec![mouse_pos.0],
            });
        }
        if mouse.just_pressed(MouseButton::Left)
            && only_modifier_keys_pressed(&keyboard, [KeyCode::AltLeft, KeyCode::ShiftLeft])
        {
            if let Some(mut move_order) = ship.5 {
                move_order.waypoints.push(mouse_pos.0);
            } else {
                commands.entity(ship.0).insert(MoveOrder {
                    waypoints: vec![mouse_pos.0],
                });
            }
        }
        if keyboard.just_pressed(KeyCode::KeyQ)
            && only_modifier_keys_pressed(&keyboard, [KeyCode::AltLeft])
        {
            commands.entity(ship.0).remove::<MoveOrder>();
        }
    }
}

fn draw_background(mut gizmos: Gizmos, camera: Query<&Transform, With<MainCamera>>) {
    let cell_size = vec2(2000., 2000.);
    let offset = camera
        .single()
        .unwrap()
        .translation
        .truncate()
        .div_euclid(cell_size)
        * cell_size;
    gizmos
        .grid_2d(
            Isometry2d::from_translation(offset),
            UVec2::splat(100),
            cell_size,
            Color::WHITE,
        )
        .outer_edges();
}

fn only_modifier_keys_pressed(
    keyboard: impl AsRef<ButtonInput<KeyCode>>,
    modifier_keys: impl IntoIterator<Item = KeyCode>,
) -> bool {
    use KeyCode::*;
    let keyboard = keyboard.as_ref();
    let modifier_keys_needed: HashSet<KeyCode, RandomState> = modifier_keys.into_iter().collect();
    let all_modifier_keys = vec![
        AltLeft,
        AltRight,
        ShiftLeft,
        ShiftRight,
        ControlLeft,
        ControlRight,
    ];
    let (keys_yes_press, keys_no_press) = all_modifier_keys
        .into_iter()
        .partition::<Vec<_>, _>(|key| modifier_keys_needed.contains(key));

    keyboard.all_pressed(keys_yes_press) && !keyboard.any_pressed(keys_no_press)
}

fn update_selection(
    mut commands: Commands,
    ships: Query<(Entity, &Transform, Option<&Selected>, &Team), With<Ship>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mouse_pos: Res<CursorWorldPos>,
    zoom: Res<MapZoom>,
) {
    let old_selection = ships
        .iter()
        .filter_map(|(ship, _, selected, _)| selected.map(|_| ship))
        .collect_vec();

    if mouse.just_pressed(MouseButton::Left)
        && only_modifier_keys_pressed(&keyboard, [KeyCode::ShiftLeft])
    {
        for (ship, ship_trans, _ship_selected, &ship_team) in &ships {
            if ship_team != Team::Friend {
                continue;
            }
            if mouse_pos.0.distance(ship_trans.translation.truncate())
                <= SHIP_SELECTION_SIZE * zoom.0
            {
                commands.entity(ship).insert_if_new(Selected);
            }
        }
    }

    if keyboard.just_pressed(KeyCode::KeyQ) && only_modifier_keys_pressed(&keyboard, []) {
        for ship in old_selection {
            commands.entity(ship).remove::<Selected>();
        }
    }
}

fn update_ship_velocity(ships: Query<(&Ship, &Transform, &mut Velocity, Option<&mut MoveOrder>)>) {
    for mut ship in ships {
        if let Some(move_order) = &mut ship.3 {
            if move_order
                .waypoints
                .get(0)
                .is_some_and(|next| next.distance(ship.1.translation.truncate()) <= 0.5)
            {
                move_order.waypoints.remove(0);
            }
        }
        let new_vel = match ship.3 {
            Some(order) if order.waypoints.len() > 0 => {
                let dir = (order.waypoints[0] - ship.1.translation.truncate()).normalize();
                dir * ship.0.speed
            }
            _ => Vec2::ZERO,
        };
        ship.2.0 = new_vel.extend(0.);
    }
}

fn toggle_paused(
    keyboard: Res<ButtonInput<KeyCode>>,
    curr_app_state: Res<State<AppState>>,
    mut next_app_state: ResMut<NextState<AppState>>,
) {
    if keyboard.just_pressed(KeyCode::KeyP) && only_modifier_keys_pressed(keyboard, []) {
        let s = curr_app_state.clone();

        next_app_state.set(match s {
            AppState::InGame { paused } => AppState::InGame { paused: !paused },
            _ => unreachable!(),
        })
    }
}

fn detect_game_end(
    ships: Query<(Entity, &Health, &Team, &Ship)>,
    mut next_app_state: ResMut<NextState<AppState>>,
) {
    let num_friendly = ships.iter().filter(|x| x.2.is_friend()).count();
    let num_enemy = ships.iter().filter(|x| x.2.is_enemy()).count();
    if num_enemy == 0 || num_friendly == 0 {
        println!("GAME FINISHED! friends={num_friendly};enemies={num_enemy}");
        next_app_state.set(AppState::PostGame);
    }
}

pub fn run() {
    // Note: if system A depends on system B or if system A is run in a later schedule (i.e. `Update` after `PreUpdate`),
    // then the `Commands` buffer will be flushed between system A and B
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(bevy::diagnostic::FrameTimeDiagnosticsPlugin::default())
        //
        .add_plugins(MainMenuUIPlugin)
        .add_plugins(InGameUIPlugin)
        .add_plugins(NetworkingPlugin)
        .add_plugins(LobbyUiPlugin)
        //
        .init_resource::<PlayerSettings>()
        .init_resource::<CursorWorldPos>()
        .init_resource::<GameRules>()
        .init_resource::<MapZoom>()
        //
        .insert_state(AppState::ConnectingToServer)
        .add_computed_state::<InGameState>()
        .enable_state_scoped_entities::<InGameState>()
        //
        .add_systems(Startup, (make_camera))
        .add_systems(
            PreUpdate,
            (
                update_cursor_world_pos.after(InputSystem),
                update_map_zoom
                    .after(InputSystem)
                    .run_if(in_state(AppState::InGame { paused: false })),
            ),
        )
        .add_systems(OnEnter(InGameState), (make_ships))
        .add_systems(Update, (toggle_paused).run_if(in_state(InGameState)))
        .add_systems(
            Update,
            (
                update_selection,
                update_selected_ship_orders
                    .after(update_selection)
                    .before(update_ship_velocity),
                update_ai_moves.before(update_ship_velocity),
                update_ship_velocity.before(apply_velocity),
                move_bullets,
                apply_velocity,
                collide_bullets.after(move_bullets).after(apply_velocity),
                update_detected_ships
                    .after(apply_velocity)
                    .after(collide_bullets),
                update_ship_ghosts.after(update_detected_ships),
                update_ship_ghosts_display.after(update_ship_ghosts),
                update_camera,
                draw_background.after(update_camera),
                update_bullet_displays.after(collide_bullets),
                update_ship_displays.after(update_detected_ships),
                update_ai_ship_targets
                    .after(update_detected_ships)
                    .before(fire_bullets),
                fire_bullets.after(update_detected_ships),
                detect_game_end.after(collide_bullets),
            )
                .run_if(in_state(AppState::InGame { paused: false })),
        )
        .run();
}
