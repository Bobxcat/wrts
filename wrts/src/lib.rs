mod in_match;
mod input_handling;
mod math_utils;
mod networking;
mod ship;
mod ui;

use std::{collections::HashMap, iter};

use bevy::prelude::*;
use enum_map::{EnumMap, enum_map};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use wrts_messaging::ClientId;

use crate::{
    in_match::InMatchPlugin,
    input_handling::{
        AxisControl, AxisInputs, ButtonControl, ButtonInputs, InputHandlingPlugin,
        InputHandlingSystem,
    },
    networking::{NetworkingPlugin, ThisClient},
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

#[derive(Serialize, Deserialize)]
struct TeamColors {
    pub ship_color: Color,
    pub gun_range_ring_color: Color,
}

#[derive(Serialize, Deserialize)]
struct PlayerControls {
    axis_controls: EnumMap<AxisInputs, AxisControl>,
    button_controls: EnumMap<ButtonInputs, ButtonControl>,
}

impl Default for PlayerControls {
    fn default() -> Self {
        use AxisInputs::*;
        use ButtonInputs::*;
        use KeyCode::*;
        Self {
            axis_controls: enum_map! {
                MoveCameraX => AxisControl::Virtual { hi: KeyD.into(), lo: KeyA.into() },
                MoveCameraY => AxisControl::Virtual { hi: KeyW.into(), lo: KeyS.into() },
            },

            button_controls: enum_map! {
                SetSelectedShip => ButtonControl::new(MouseButton::Left),
                PushSelectedShip => ButtonControl::new_with(MouseButton::Left, [ShiftLeft]),
                ClearSelectedShips => ButtonControl::new(KeyQ),
                SetFireTarg => ButtonControl::new(MouseButton::Right),
                ClearFireTarg => ButtonControl::new_with(KeyQ, [ControlLeft]),
                SetWaypoint => ButtonControl::new(MouseButton::Right),
                PushWaypoint => ButtonControl::new_with(MouseButton::Right, [ShiftLeft]),
                ClearWaypoints => ButtonControl::new_with(KeyQ, [AltLeft]),

                FireTorpVolley => ButtonControl::new_with(MouseButton::Left, [ControlLeft]),

                UseConsumableSmoke => ButtonControl::new(Digit1),
            },
        }
    }
}

#[derive(Resource, Serialize, Deserialize)]
struct PlayerSettings {
    username: String,
    ship_icon_scale: f32,
    bullet_icon_scale: f32,
    team_friend_colors: TeamColors,
    team_enemy_colors: TeamColors,
    controls: PlayerControls,
}

impl Default for PlayerSettings {
    fn default() -> Self {
        Self {
            username: "Username".into(),
            ship_icon_scale: 20.,
            bullet_icon_scale: 5.,
            team_friend_colors: TeamColors {
                ship_color: Color::linear_rgb(0., 0.2, 0.7),
                gun_range_ring_color: Color::linear_rgb(0.2, 0.2, 0.8),
            },
            team_enemy_colors: TeamColors {
                ship_color: Color::linear_rgb(0.7, 0.2, 0.),
                gun_range_ring_color: Color::linear_rgb(0.8, 0.2, 0.2),
            },
            controls: Default::default(),
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

#[derive(Component, Debug, Default, Clone)]
struct Velocity(pub Vec2);

#[derive(Component, Debug, Default, Clone)]
struct MoveOrder {
    pub waypoints: Vec<Vec2>,
}

#[derive(Component, Debug, Default, Clone)]
#[require(Transform)]
struct SmokePuff {
    pub radius: f32,
}

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

fn update_smoke_puff_displays(mut gizmos: Gizmos, smoke_puffs: Query<(&SmokePuff, &Transform)>) {
    for (puff, puff_trans) in smoke_puffs {
        gizmos
            .circle_2d(
                Isometry2d::from_translation(puff_trans.translation.truncate()),
                puff.radius,
                Color::WHITE,
            )
            .resolution(32);
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

fn write_settings_to_file(settings: Res<PlayerSettings>) {
    std::fs::create_dir_all("player_settings").unwrap();
    std::fs::write(
        "player_settings/settings.json",
        serde_json::to_string_pretty(&*settings).unwrap(),
    )
    .unwrap();
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
        .add_plugins(InputHandlingPlugin)
        //
        .init_resource::<PlayerSettings>()
        .init_resource::<CursorWorldPos>()
        .init_resource::<MapZoom>()
        //
        .insert_state(AppState::ConnectingToServer)
        //
        .add_systems(Startup, write_settings_to_file)
        .add_systems(Startup, make_camera)
        // .add_systems(PreUpdate, ())
        .add_systems(
            Update,
            (
                update_selected_ship_orders_display.after(InputHandlingSystem),
                update_ship_ghosts,
                update_ship_ghosts_display.after(update_ship_ghosts),
                draw_background,
                update_bullet_displays,
                update_torpedo_displays,
                update_smoke_puff_displays,
            )
                .run_if(in_state(AppState::InMatch)),
        )
        .run();
}
