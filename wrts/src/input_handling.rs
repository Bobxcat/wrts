use std::f32::consts::FRAC_PI_2;

use bevy::{
    ecs::system::lifetimeless::{SQuery, SRes},
    input::{InputSystem, mouse::MouseWheel},
    prelude::*,
    window::PrimaryWindow,
};
use itertools::Itertools;
use leafwing_input_manager::{
    Actionlike, InputControlKind,
    buttonlike::ButtonValue,
    clashing_inputs::BasicInputs,
    plugin::InputManagerSystem,
    prelude::{
        updating::{CentralInputStore, InputRegistration, UpdatableInput},
        *,
    },
};
use serde::{Deserialize, Serialize};
use wrts_messaging::{Client2Match, Message};

use crate::{
    AppState, CursorWorldPos, DetectionStatus, FireTarget, MainCamera, MapZoom, MoveOrder,
    PlayerSettings, Selected, Team, Velocity,
    in_match::SharedEntityTracking,
    math_utils,
    networking::{ServerConnection, ThisClient},
    ship::Ship,
};

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InputHandlingSystem;

#[derive(Debug, Clone, Copy)]
pub struct InputHandlingPlugin;

impl Plugin for InputHandlingPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(InputManagerPlugin::<InputAction>::default())
            //
            .configure_sets(OnEnter(AppState::InMatch), InputHandlingSystem)
            .add_systems(
                OnEnter(AppState::InMatch),
                spawn_input_map.in_set(InputHandlingSystem),
            )
            //
            .configure_sets(
                PreUpdate,
                InputHandlingSystem.run_if(in_state(AppState::InMatch)),
            )
            .add_systems(
                PreUpdate,
                (
                    update_cursor_world_pos.after(InputSystem),
                    update_hovering
                        .before(InputManagerSystem::Filter)
                        .before(InputManagerSystem::Accumulate)
                        .before(InputManagerSystem::Update)
                        .after(update_cursor_world_pos),
                    update_map_zoom
                        .after(InputSystem)
                        .run_if(in_state(AppState::InMatch)),
                ),
            )
            //
            .configure_sets(
                Update,
                InputHandlingSystem.run_if(in_state(AppState::InMatch)),
            )
            .add_systems(
                Update,
                (
                    read_inputs,
                    use_consumables,
                    update_selection,
                    update_selected_ship_orders.after(update_selection),
                    fire_torpedoes.after(update_selection),
                    update_camera,
                )
                    .in_set(InputHandlingSystem),
            )
            .register_input_kind::<HoveringOverEnemyShip>(InputControlKind::Button);
    }
}

/// Attached to `Ship`s when the cursor is hovering over them
#[derive(Component, Clone, Copy, PartialEq, Eq, Hash)]
struct Hovering;

/// https://docs.rs/leafwing-input-manager/latest/leafwing_input_manager/user_input/updating/trait.UpdatableInput.html
///
/// This is both an `UpdatableInput` *and* a `UserInput`, which
/// reflects whether or not a single enemy ship is currently being hovered over
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize)]
struct HoveringOverEnemyShip;

impl<'w, 's> UpdatableInput for HoveringOverEnemyShip {
    type SourceData = (
        SQuery<(Entity, &'static Team), With<Hovering>>,
        Option<SRes<ThisClient>>,
    );

    fn compute(
        mut central_input_store: ResMut<CentralInputStore>,
        source_data: bevy::ecs::system::StaticSystemParam<Self::SourceData>,
    ) {
        let (hovered, this_client) = source_data.into_inner();
        let Some(this_client) = this_client else {
            return;
        };

        let hovering_over_enemy = hovered
            .single()
            .is_ok_and(|(_, team)| !team.is_this_client(*this_client));
        central_input_store.update_buttonlike(Self, ButtonValue::from_pressed(hovering_over_enemy));
    }
}

impl UserInput for HoveringOverEnemyShip {
    fn kind(&self) -> InputControlKind {
        InputControlKind::Button
    }

    fn decompose(&self) -> BasicInputs {
        BasicInputs::Simple(Box::new(Self))
    }
}

impl Buttonlike for HoveringOverEnemyShip {
    fn pressed(&self, input_store: &updating::CentralInputStore, _gamepad: Entity) -> bool {
        input_store.pressed(&Self)
    }
}

#[derive(
    Actionlike,
    Serialize,
    Deserialize,
    PartialEq,
    Eq,
    Hash,
    Clone,
    Copy,
    Debug,
    Reflect,
    enum_map::Enum,
)]
pub enum InputAction {
    #[actionlike(DualAxis)]
    MoveCamera,
    SetSelectedShip,
    ClearSelectedShips,

    SetFireTarg,
    ClearFireTarg,
    SetWaypoint,
    PushWaypoint,
    ClearWaypoints,

    FireTorpVolley,

    UseConsumableSmoke,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeybindKey {
    Gamepad(GamepadButton),
    Keyboard(KeyCode),
    Mouse(MouseButton),
}

impl From<GamepadButton> for KeybindKey {
    fn from(value: GamepadButton) -> Self {
        Self::Gamepad(value)
    }
}

impl From<KeyCode> for KeybindKey {
    fn from(value: KeyCode) -> Self {
        Self::Keyboard(value)
    }
}

impl From<MouseButton> for KeybindKey {
    fn from(value: MouseButton) -> Self {
        Self::Mouse(value)
    }
}

enum KeybindClassify {
    Empty,
    Single(KeybindKey),
    Chord(ButtonlikeChord),
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Keybind {
    keys: Vec<KeybindKey>,
    #[serde(skip)]
    has_hovering_over_enemy_ship: bool,
}

impl Keybind {
    pub fn new(buttons: impl IntoIterator<Item = impl Into<KeybindKey>>) -> Self {
        Self {
            keys: buttons.into_iter().map(Into::into).collect_vec(),
            has_hovering_over_enemy_ship: false,
        }
    }

    pub fn with(mut self, button: impl Into<KeybindKey>) -> Self {
        self.keys.push(button.into());
        self
    }

    fn full_len(&self) -> usize {
        self.keys.len() + self.has_hovering_over_enemy_ship as usize
    }

    fn classify(&self) -> KeybindClassify {
        match self.full_len() {
            0 => KeybindClassify::Empty,
            // FIXME: Single crashes if it's not a keybind, and is instead something like "HoveringOverEnemyShip"
            1 => KeybindClassify::Single(
                self.keys
                    .get(0)
                    .cloned()
                    .expect("Can't have a `KeybindClassify::Single` which isn't a keybind"),
            ),
            _ => {
                let mut gamepads = vec![];
                let mut keyboards = vec![];
                let mut mouses = vec![];
                for key in self.keys.clone() {
                    match key {
                        KeybindKey::Gamepad(b) => gamepads.push(b),
                        KeybindKey::Keyboard(b) => keyboards.push(b),
                        KeybindKey::Mouse(b) => mouses.push(b),
                    }
                }

                let hovering = self
                    .has_hovering_over_enemy_ship
                    .then_some(HoveringOverEnemyShip);

                KeybindClassify::Chord(
                    ButtonlikeChord::new(gamepads)
                        .with_multiple(keyboards)
                        .with_multiple(mouses)
                        .with_multiple(hovering),
                )
            }
        }
    }
}

fn read_inputs(
    input_store: Res<CentralInputStore>,
    input_map: Res<InputMap<InputAction>>,
    actions: Res<ActionState<InputAction>>,
    mut prev: Local<Option<ActionState<InputAction>>>,
) {
    let prev = prev.get_or_insert_default();
    if &*actions == prev {
        return;
    }

    info!(
        "DEC SetFireTarg={:?}, SetWaypoint={:?}, clashes={}",
        input_map.decomposed(&InputAction::SetFireTarg),
        input_map.decomposed(&InputAction::SetWaypoint),
        input_map.decomposed(&InputAction::SetFireTarg)[0]
            .clashes_with(&input_map.decomposed(&InputAction::SetWaypoint)[0]),
    );

    info!(
        "hovering={}, SetFireTarg={:?}, SetWaypoint={:?}",
        input_store.button_value(&HoveringOverEnemyShip),
        actions.pressed(&InputAction::SetFireTarg),
        actions.pressed(&InputAction::SetWaypoint)
    );
    *prev = actions.clone();
}

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

fn update_hovering(
    mut commands: Commands,
    ships: Query<(Entity, &Ship, &Transform, Option<&Hovering>)>,
    cursor_pos: Res<CursorWorldPos>,
    zoom: Res<MapZoom>,
) {
    for (ship, _ship, ship_trans, _) in ships {
        if cursor_pos.0.distance(ship_trans.translation.truncate())
            <= crate::SHIP_SELECTION_SIZE * zoom.0
        {
            commands.entity(ship).insert_if_new(Hovering);
        } else {
            commands.entity(ship).try_remove::<Hovering>();
        }
    }
}

fn spawn_input_map(mut commands: Commands, settings: Res<PlayerSettings>) {
    let mut input_map = InputMap::default();

    input_map.insert_dual_axis(InputAction::MoveCamera, GamepadStick::LEFT);
    input_map.insert_dual_axis_boxed(
        InputAction::MoveCamera,
        settings.controls.move_camera.clone(),
    );

    for (action, inputs) in &settings.controls.button_controls {
        if Actionlike::input_control_kind(&action) != InputControlKind::Button {
            continue;
        }
        let mut inputs = inputs.clone();
        if action == InputAction::SetFireTarg {
            inputs.has_hovering_over_enemy_ship = true;
        }
        info!(
            "{:?} -> {}",
            action,
            serde_json::to_string(&inputs).unwrap()
        );

        match inputs.classify() {
            KeybindClassify::Empty => input_map.insert(action, ButtonlikeChord::default()),
            KeybindClassify::Single(KeybindKey::Gamepad(b)) => input_map.insert(action, b),
            KeybindClassify::Single(KeybindKey::Keyboard(b)) => input_map.insert(action, b),
            KeybindClassify::Single(KeybindKey::Mouse(b)) => input_map.insert(action, b),
            KeybindClassify::Chord(b) => input_map.insert(action, b),
        };
    }

    commands.insert_resource(input_map);
    commands.init_resource::<ActionState<InputAction>>();
}

fn update_camera(
    mut camera: Query<(&mut Projection, &mut Transform), With<MainCamera>>,
    actions: Res<ActionState<InputAction>>,
    zoom: Res<MapZoom>,
    time: Res<Time>,
) {
    let mut camera = camera.single_mut().unwrap();
    let Projection::Orthographic(proj) = &mut *camera.0 else {
        panic!()
    };

    proj.scale = zoom.0;
    let dir = actions.clamped_axis_pair(&InputAction::MoveCamera);
    camera.1.translation += (dir * 200. * zoom.0 * time.delta_secs()).extend(0.);
}

fn update_selection(
    mut commands: Commands,
    ships: Query<(Entity, &Transform, Option<&Selected>, &Team), With<Ship>>,
    hovering: Query<(Entity, &Team, &Hovering)>,
    actions: Res<ActionState<InputAction>>,
    this_client: Res<ThisClient>,
) {
    let old_selection = ships
        .iter()
        .filter_map(|(ship, _, selected, _)| selected.map(|_| ship))
        .collect_vec();

    if actions.just_pressed(&InputAction::SetSelectedShip) {
        for (ship, ship_team, _hovering) in hovering {
            if ship_team.is_this_client(*this_client) {
                commands.entity(ship).insert_if_new(Selected);
            }
        }
    }

    if actions.just_pressed(&InputAction::ClearSelectedShips) {
        for ship in old_selection {
            commands.entity(ship).remove::<Selected>();
        }
    }
}

fn update_selected_ship_orders(
    mut commands: Commands,
    actions: Res<ActionState<InputAction>>,
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

        if actions.just_pressed(&InputAction::SetFireTarg) {
            if let Some(new_targ) = all_ships.iter().find(|maybe_targ| {
                !maybe_targ.2.is_this_client(*this_client)
                    && *maybe_targ.3 != DetectionStatus::Never
                    && maybe_targ.1.translation.truncate().distance(mouse_pos.0)
                        <= crate::SHIP_SELECTION_SIZE * zoom.0
            }) {
                new_fire_target = Some(Some(FireTarget { ship: new_targ.0 }));
            }
        }
        if actions.just_pressed(&InputAction::ClearFireTarg) {
            new_fire_target = Some(None);
        }

        if actions.just_pressed(&InputAction::SetWaypoint) {
            new_move_order = Some(MoveOrder {
                waypoints: vec![mouse_pos.0],
            });
        }
        if actions.just_pressed(&InputAction::PushWaypoint) {
            if let Some(mut move_order) = ship.5 {
                move_order.waypoints.push(mouse_pos.0);
                new_move_order = Some(move_order.clone());
            } else {
                new_move_order = Some(MoveOrder {
                    waypoints: vec![mouse_pos.0],
                });
            }
        }
        if actions.just_pressed(&InputAction::ClearWaypoints) {
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

fn use_consumables(
    selected_ships: Query<(Entity, &Ship), With<Selected>>,
    actions: Res<ActionState<InputAction>>,
    mut server: ResMut<ServerConnection>,
    shared_entities: Res<SharedEntityTracking>,
) {
    let Ok((selected_entity, selected_ship)) = selected_ships.single() else {
        return;
    };
    let consumables = &selected_ship.template.consumables;
    // Smoke
    if actions.just_pressed(&InputAction::UseConsumableSmoke) {
        if consumables.smoke().is_some() {
            let _ = server.send(Message::Client2Match(Client2Match::UseConsumableSmoke {
                ship: shared_entities[selected_entity],
            }));
        }
    }
}

fn fire_torpedoes(
    mut gizmos: Gizmos,
    selected: Query<(Entity, &Ship, &Transform), With<Selected>>,
    ships: Query<(&Team, &Transform, &Velocity, &DetectionStatus), With<Ship>>,
    actions: Res<ActionState<InputAction>>,
    cursor_pos: Res<CursorWorldPos>,
    shared_entities: Res<SharedEntityTracking>,
    mut server: ResMut<ServerConnection>,
    this_client: Res<ThisClient>,
    zoom: Res<MapZoom>,
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

    for (ship_team, ship_trans, ship_vel, ship_detection) in ships {
        if ship_team.is_this_client(*this_client) {
            continue;
        }

        if *ship_detection != DetectionStatus::Detected {
            continue;
        }

        let Some(res) = math_utils::torpedo_problem(
            selected_trans.translation.truncate(),
            ship_trans.translation.truncate(),
            ship_vel.0,
            torps.speed.mps() as f64,
        ) else {
            continue;
        };

        gizmos.cross_2d(
            Isometry2d::from_translation(res.intersection_point),
            10. * zoom.0,
            Color::linear_rgb(0.4, 0.5, 0.5),
        );
    }

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

    if let Some(fire_dir) = (cursor_pos.0 - ship_pos).try_normalize() {
        let is_valid_angle = firing_angles
            .into_iter()
            .any(|angle_range| angle_range.contains(fire_dir));
        if is_valid_angle {
            gizmos.line_2d(
                ship_pos + fire_dir * min_dist,
                ship_pos + fire_dir * max_dist,
                angles_color,
            );

            if actions.just_pressed(&InputAction::FireTorpVolley) {
                let _ = server.send(Message::Client2Match(Client2Match::LaunchTorpedoVolley {
                    ship: shared_entities[selected],
                    dir: fire_dir,
                }));
            }
        }
    };
}
