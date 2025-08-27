mod in_match;
mod math_utils;
mod networking;
mod ship;
mod ui;

use std::{
    collections::{HashMap, HashSet},
    convert::identity,
    f32::consts::FRAC_PI_2,
    hash::RandomState,
    iter,
};

use bevy::{
    input::{InputSystem, mouse::MouseWheel},
    prelude::*,
    window::PrimaryWindow,
};
use itertools::Itertools;
use wrts_messaging::{Client2Match, ClientId, Message};

use crate::{
    in_match::{InMatchPlugin, SharedEntityTracking},
    networking::{NetworkingPlugin, ServerConnection, ThisClient},
    ship::{Ship, ShipDisplayPlugin},
    ui::{in_game::InGameUIPlugin, lobby::LobbyUiPlugin},
};

#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[states(scoped_entities)]
pub enum AppState {
    ConnectingToServer,
    LobbyMenu,
    InMatch,
}

const SHIP_SELECTION_SIZE: f32 = 20.;

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
    pub fn team_colors(&self, team: Team, this_client: ThisClient) -> &TeamColors {
        match team.is_this_client(this_client) {
            true => &self.team_friend_colors,
            false => &self.team_enemy_colors,
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

#[derive(Debug, Default, Component, Clone)]
struct MoveOrder {
    pub waypoints: Vec<Vec2>,
}

// struct RudderPos

#[derive(Component, Debug, Clone)]
struct FireTarget {
    ship: Entity,
}

#[derive(Component, Debug, Default, Clone)]
struct Health(pub f64);

#[derive(Component, Debug, Default, Clone, Copy)]
struct Selected;

/// Currently detected
#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum DetectionStatus {
    #[default]
    Never,
    Detected,
    UnDetected,
}

/// The ghost of a once-detected ship
#[derive(Component, Debug, Clone, Copy)]
#[require(Health, Transform, Sprite)]
pub struct ShipGhost {
    pub owner: Entity,
}

/// The number of world units per rendered pixel
///
/// This controls the `zoom` parameter of the camera
#[derive(Resource, Debug, Default, Clone, Copy)]
struct MapZoom(pub f32);

/// Component representing "ownership" by a client
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
struct Team(pub ClientId);

impl Default for Team {
    fn default() -> Self {
        panic!(
            "Called `Default` for Team. Remember to insert `Team` manually when creating objects!!"
        );
    }
}

impl Team {
    pub fn is_this_client(self, this_client: ThisClient) -> bool {
        self.0 == this_client.0
    }
}

#[derive(Component, Debug, Clone)]
#[require(Team, Transform, Sprite, DetectionStatus)]
struct Torpedo {
    owning_ship: Entity,
    damage: f64,
    speed: f32,
}

#[derive(Component, Debug, Clone, Copy)]
struct TorpedoReloadText;

fn fire_torpedoes(
    mut gizmos: Gizmos,
    selected: Query<(Entity, &Ship, &Transform), With<Selected>>,
    cursor_pos: Res<CursorWorldPos>,
    mouse: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    shared_entities: Res<SharedEntityTracking>,
    mut server: ResMut<ServerConnection>,
) {
    let Ok((selected, selected_ship, selected_trans)) = selected.single() else {
        return;
    };

    let Some(torps) = selected_ship.template.torpedoes.as_ref() else {
        return;
    };

    let firing_angles =
        [torps.port_firing_angle, torps.starboard_firing_angle()].map(|angle_range| {
            angle_range.rotated_by(selected_trans.rotation.to_euler(EulerRot::ZXY).0)
        });

    let ship_pos = selected_trans.translation.truncate();
    let angles_color = Color::linear_rgb(0.1, 0.4, 0.8);
    let min_dist = 100.;
    let max_dist = torps.range;

    for angle_range in firing_angles {
        let iso = Isometry2d::new(
            ship_pos,
            Rot2::radians(angle_range.start_dir().to_angle() - FRAC_PI_2),
        );
        let arc_angle = angle_range.start_dir().angle_to(angle_range.end_dir());
        gizmos.arc_2d(iso, arc_angle, min_dist, angles_color);
        gizmos
            .arc_2d(iso, arc_angle, max_dist, angles_color)
            .resolution(64);
        gizmos.line_2d(
            ship_pos + angle_range.start_dir() * min_dist,
            ship_pos + angle_range.start_dir() * max_dist,
            angles_color,
        );
        gizmos.line_2d(
            ship_pos + angle_range.end_dir() * min_dist,
            ship_pos + angle_range.end_dir() * max_dist,
            angles_color,
        );
    }

    let Some(fire_dir) = (cursor_pos.0 - ship_pos).try_normalize() else {
        return;
    };
    let is_valid_angle = firing_angles
        .into_iter()
        .any(|angle_range| angle_range.contains(fire_dir));
    if is_valid_angle {
        gizmos.line_2d(
            ship_pos + fire_dir * min_dist,
            ship_pos + fire_dir * max_dist,
            angles_color,
        );

        if mouse.just_pressed(MouseButton::Right) && only_modifier_keys_pressed(&keyboard, []) {
            let _ = server.send(Message::Client2Match(Client2Match::LaunchTorpedoVolley {
                ship: shared_entities[selected],
                dir: fire_dir,
            }));
        }
    }
}

fn update_torpedo_displays(
    mut gizmos: Gizmos,
    torps: Query<(&Torpedo, &Team, &Transform, &mut Sprite, &DetectionStatus)>,
    this_client: Res<ThisClient>,
    zoom: Res<MapZoom>,
    settings: Res<PlayerSettings>,
) {
    for (torp, torp_team, torp_trans, mut torp_sprite, torp_detection) in torps {
        let is_visible =
            torp_team.is_this_client(*this_client) || *torp_detection == DetectionStatus::Detected;

        match is_visible {
            true => {
                *torp_sprite = Sprite::from_color(
                    settings.team_colors(*torp_team, *this_client).ship_color,
                    vec2(20., 7.) * zoom.0,
                );
                let torp_dir = Vec2::from_angle(torp_trans.rotation.to_euler(EulerRot::ZXY).0);
                let torp_pos = torp_trans.translation.truncate();
                gizmos.line_gradient_2d(
                    torp_pos - torp_dir * 10. * zoom.0,
                    torp_pos - torp.speed / 75. * torp_dir * 10. * zoom.0,
                    Color::WHITE,
                    Color::linear_rgb(0.5, 0.5, 0.5),
                );
            }
            false => {
                *torp_sprite = Sprite::default();
            }
        }
    }
}

fn update_ship_ghosts(
    mut commands: Commands,
    changed_ships: Query<
        (Entity, &Team, &Transform, &DetectionStatus),
        (With<Ship>, Changed<DetectionStatus>),
    >,
    all_ships: Query<(), With<Ship>>,
    this_client: Res<ThisClient>,
    mut current_ghosts: Local<HashMap<Entity, Entity>>,
) {
    for (ship, ship_team, ship_trans, ship_detection) in changed_ships {
        if ship_team.is_this_client(*this_client) {
            continue;
        }
        match ship_detection {
            DetectionStatus::UnDetected if !current_ghosts.contains_key(&ship) => {
                current_ghosts.insert(
                    ship,
                    commands
                        .spawn((
                            StateScoped(AppState::InMatch),
                            ShipGhost { owner: ship },
                            *ship_trans,
                        ))
                        .id(),
                );
            }
            DetectionStatus::Detected if current_ghosts.contains_key(&ship) => {
                commands
                    .entity(current_ghosts.remove(&ship).unwrap())
                    .despawn();
            }
            DetectionStatus::Never => assert!(!current_ghosts.contains_key(&ship)),
            _ => (),
        }
    }

    for (ship_entity, ghost_entity) in current_ghosts.clone() {
        if !all_ships.contains(ship_entity) {
            commands.entity(ghost_entity).despawn();
            current_ghosts.remove(&ship_entity);
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

#[derive(Debug, Component, Clone)]
#[require(Team, Sprite, Transform)]
struct Bullet {
    owning_ship: Entity,
    damage: f64,
}

fn update_bullet_displays(
    bullets: Query<(&Transform, &mut Sprite, &Team), With<Bullet>>,
    settings: Res<PlayerSettings>,
    zoom: Res<MapZoom>,
    this_client: Res<ThisClient>,
) {
    for (trans, mut sprite, &team) in bullets {
        if trans.translation.z <= 0. {
            *sprite = Sprite::from_color(
                Color::linear_rgb(0., 0., 0.),
                sprite.custom_size.unwrap_or_default(),
            );
        } else {
            sprite.color = settings.team_colors(team, *this_client).ship_color;
        }
        let double_height = 1000.;
        let height_scaling = 1. + trans.translation.z.clamp(0., 20_000.) / double_height;
        sprite.custom_size =
            Some(vec2(2., 0.5) * height_scaling * settings.bullet_icon_scale * zoom.0);
    }
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
    let scroll_speed = 0.2;
    let scroll_parts = 4;
    for scroll in mouse_scroll.read() {
        for _ in 0..scroll_parts {
            zoom.0 -= scroll_speed * scroll.y * zoom.0 * (1. / scroll_parts as f32);
        }
    }
    zoom.0 = zoom.0.clamp(0.5, 50.);
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

fn update_selected_ship_orders_display(
    mut gizmos: Gizmos,
    ships_selected: Query<
        (&Ship, &Transform, Option<&FireTarget>, Option<&MoveOrder>),
        With<Selected>,
    >,
    transforms: Query<&Transform>,
    settings: Res<PlayerSettings>,
    zoom: Res<MapZoom>,
) {
    for (_selected_ship, selected_trans, selected_fire_target, selected_move_order) in
        &ships_selected
    {
        let circle_size = zoom.0 * settings.ship_icon_scale * 0.5 * 1.4;
        gizmos
            .circle_2d(
                Isometry2d::from_translation(selected_trans.translation.truncate()),
                circle_size,
                Color::WHITE,
            )
            .resolution(10);
        if let Some(targ) = selected_fire_target.and_then(|targ| transforms.get(targ.ship).ok()) {
            let draw_pos = targ.translation.truncate();
            gizmos
                .circle_2d(
                    Isometry2d::from_translation(draw_pos),
                    circle_size,
                    Color::linear_rgb(0.8, 0.3, 0.3),
                )
                .resolution(10);
        }

        if let Some(move_order) = selected_move_order
            && !move_order.waypoints.is_empty()
        {
            gizmos.linestrip_2d(
                iter::once(selected_trans.translation.truncate())
                    .chain(move_order.waypoints.iter().copied()),
                Color::linear_rgb(1., 0.2, 0.2),
            );
        }
    }
}

fn update_selected_ship_orders(
    mut commands: Commands,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mouse_pos: Res<CursorWorldPos>,
    all_ships: Query<(Entity, &Transform, &Team, &DetectionStatus), With<Ship>>,
    mut ships_selected: Query<(
        Entity,
        &Transform,
        &Selected,
        &Ship,
        Option<&FireTarget>,
        Option<&mut MoveOrder>,
    )>,
    this_client: Res<ThisClient>,
    zoom: Res<MapZoom>,
    shared_entities: Res<SharedEntityTracking>,
    mut server: ResMut<ServerConnection>,
) {
    for ship in &mut ships_selected {
        let mut new_move_order = None;
        let mut new_fire_target = None;

        if mouse.just_pressed(MouseButton::Left)
            && only_modifier_keys_pressed(&keyboard, [KeyCode::ControlLeft])
        {
            if let Some(new_targ) = all_ships.iter().find(|maybe_targ| {
                !maybe_targ.2.is_this_client(*this_client)
                    && *maybe_targ.3 != DetectionStatus::Never
                    && maybe_targ.1.translation.truncate().distance(mouse_pos.0)
                        <= SHIP_SELECTION_SIZE * zoom.0
            }) {
                new_fire_target = Some(Some(FireTarget { ship: new_targ.0 }));
            }
        }
        if keyboard.just_pressed(KeyCode::KeyQ)
            && only_modifier_keys_pressed(&keyboard, [KeyCode::ControlLeft])
        {
            new_fire_target = Some(None);
        }

        if mouse.just_pressed(MouseButton::Left)
            && only_modifier_keys_pressed(&keyboard, [KeyCode::AltLeft])
        {
            new_move_order = Some(MoveOrder {
                waypoints: vec![mouse_pos.0],
            });
        }
        if mouse.just_pressed(MouseButton::Left)
            && only_modifier_keys_pressed(&keyboard, [KeyCode::AltLeft, KeyCode::ShiftLeft])
        {
            if let Some(mut move_order) = ship.5 {
                move_order.waypoints.push(mouse_pos.0);
                new_move_order = Some(move_order.clone());
            } else {
                new_move_order = Some(MoveOrder {
                    waypoints: vec![mouse_pos.0],
                });
            }
        }
        if keyboard.just_pressed(KeyCode::KeyQ)
            && only_modifier_keys_pressed(&keyboard, [KeyCode::AltLeft])
        {
            new_move_order = Some(MoveOrder { waypoints: vec![] });
        }

        if let Some(move_order) = new_move_order {
            let _ = server.send(Message::Client2Match(Client2Match::SetMoveOrder {
                id: shared_entities[ship.0],
                waypoints: move_order.waypoints.clone(),
            }));
            commands.entity(ship.0).insert(move_order);
        }

        if let Some(fire_target) = new_fire_target {
            let _ = server.send(Message::Client2Match(Client2Match::SetFireTarg {
                id: shared_entities[ship.0],
                targ: fire_target.clone().map(|targ| shared_entities[targ.ship]),
            }));
            match fire_target {
                Some(fire_target) => {
                    commands.entity(ship.0).insert(fire_target);
                }
                None => {
                    commands.entity(ship.0).remove::<FireTarget>();
                }
            }
        }
    }
}

fn draw_background(
    mut gizmos: Gizmos,
    camera: Query<&Transform, With<MainCamera>>,
    zoom: Res<MapZoom>,
) {
    let cell_size = { vec2(1000., 1000.) * if zoom.0 < 10. { 2. } else { 4. } };

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
            UVec2::splat(50),
            cell_size,
            Color::WHITE,
        )
        .outer_edges();
    gizmos.rect_2d(
        Isometry2d::IDENTITY,
        wrts_match_shared::map_bounds().1 - wrts_match_shared::map_bounds().0,
        Color::linear_rgb(0.8, 0.2, 0.2),
    );
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
    this_client: Res<ThisClient>,
) {
    let old_selection = ships
        .iter()
        .filter_map(|(ship, _, selected, _)| selected.map(|_| ship))
        .collect_vec();

    if mouse.just_pressed(MouseButton::Left)
        && only_modifier_keys_pressed(&keyboard, [KeyCode::ShiftLeft])
    {
        for (ship, ship_trans, _ship_selected, &ship_team) in &ships {
            if !ship_team.is_this_client(*this_client) {
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

pub fn run() {
    // Note: if system A depends on system B or if system A is run in a later schedule (i.e. `Update` after `PreUpdate`),
    // then the `Commands` buffer will be flushed between system A and B
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(bevy::diagnostic::FrameTimeDiagnosticsPlugin::default())
        //
        .add_plugins(InGameUIPlugin)
        .add_plugins(LobbyUiPlugin)
        .add_plugins(NetworkingPlugin)
        .add_plugins(InMatchPlugin)
        .add_plugins(ShipDisplayPlugin)
        //
        .init_resource::<PlayerSettings>()
        .init_resource::<CursorWorldPos>()
        .init_resource::<MapZoom>()
        //
        .insert_state(AppState::ConnectingToServer)
        //
        .add_systems(Startup, make_camera)
        .add_systems(
            PreUpdate,
            (
                update_cursor_world_pos.after(InputSystem),
                update_map_zoom
                    .after(InputSystem)
                    .run_if(in_state(AppState::InMatch)),
            ),
        )
        .add_systems(
            Update,
            (
                update_selection,
                update_selected_ship_orders.after(update_selection),
                fire_torpedoes.after(update_selection),
                update_selected_ship_orders_display.after(update_selected_ship_orders),
                update_ship_ghosts,
                update_ship_ghosts_display.after(update_ship_ghosts),
                update_camera,
                draw_background,
                update_bullet_displays,
                update_torpedo_displays,
            )
                .run_if(in_state(AppState::InMatch)),
        )
        .run();
}
