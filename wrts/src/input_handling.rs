use std::{convert::identity, f32::consts::FRAC_PI_2};

use bevy::{
    input::{InputSystem, mouse::MouseWheel},
    prelude::*,
    window::PrimaryWindow,
};
use enum_map::EnumMap;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;
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
        app
            //
            .configure_sets(OnEnter(AppState::InMatch), InputHandlingSystem)
            .add_systems(
                OnEnter(AppState::InMatch),
                spawn_action_state.in_set(InputHandlingSystem),
            )
            //
            .configure_sets(
                PreUpdate,
                InputHandlingSystem
                    .after(InputSystem)
                    .run_if(in_state(AppState::InMatch)),
            )
            .add_systems(
                PreUpdate,
                (
                    update_action_state,
                    update_cursor_world_pos,
                    update_hovering
                        .after(update_action_state)
                        .after(update_cursor_world_pos),
                    update_map_zoom,
                )
                    .in_set(InputHandlingSystem),
            )
            //
            .configure_sets(
                Update,
                InputHandlingSystem.run_if(in_state(AppState::InMatch)),
            )
            .add_systems(
                Update,
                (
                    use_consumables,
                    update_selection,
                    update_selected_ship_orders.after(update_selection),
                    fire_torpedoes.after(update_selection),
                    update_camera,
                )
                    .in_set(InputHandlingSystem),
            );
    }
}

/// Attached to `Ship`s when the cursor is hovering over them
#[derive(Component, Clone, Copy, PartialEq, Eq, Hash)]
struct Hovering;

#[derive(
    Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Copy, Debug, enum_map::Enum, strum::EnumIter,
)]
pub enum ButtonInputs {
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

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Copy, Debug)]
enum SpecialCondition {
    HoveringOverEnemyShip,
}

impl ButtonInputs {
    fn special_conditions(self) -> Vec<SpecialCondition> {
        match self {
            ButtonInputs::SetFireTarg => vec![SpecialCondition::HoveringOverEnemyShip],
            _ => vec![],
        }
    }

    fn priority(self) -> i32 {
        match self {
            ButtonInputs::SetFireTarg => 1,
            ButtonInputs::ClearFireTarg
            | ButtonInputs::SetWaypoint
            | ButtonInputs::PushWaypoint
            | ButtonInputs::ClearWaypoints
            | ButtonInputs::FireTorpVolley
            | ButtonInputs::UseConsumableSmoke
            | ButtonInputs::SetSelectedShip
            | ButtonInputs::ClearSelectedShips => 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeybindKey {
    Gamepad(GamepadButton),
    Keyboard(KeyCode),
    Mouse(MouseButton),
}

impl KeybindKey {
    fn read_pressed(&self, ctx: ControlReadCtx) -> bool {
        match self {
            KeybindKey::Gamepad(b) => ctx.gamepad.map(|g| g.pressed(*b)).unwrap_or(false),
            KeybindKey::Keyboard(b) => ctx.keyboard.pressed(*b),
            KeybindKey::Mouse(b) => ctx.mouse.pressed(*b),
        }
    }

    fn read_just_pressed(&self, ctx: ControlReadCtx) -> bool {
        match self {
            KeybindKey::Gamepad(b) => ctx.gamepad.map(|g| g.just_pressed(*b)).unwrap_or(false),
            KeybindKey::Keyboard(b) => ctx.keyboard.just_pressed(*b),
            KeybindKey::Mouse(b) => ctx.mouse.just_pressed(*b),
        }
    }
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

#[derive(Debug, Clone, Copy)]
struct ControlReadCtx<'a> {
    gamepad: Option<&'a Gamepad>,
    keyboard: &'a ButtonInput<KeyCode>,
    mouse: &'a ButtonInput<MouseButton>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ButtonControl {
    principle: KeybindKey,
    modifiers: Vec<KeybindKey>,
}

impl ButtonControl {
    /// * `principle` - the key that must be `just_pressed` while *all* `modifiers` are
    /// pressed in order for this control to be considered activated
    pub fn new(principle: impl Into<KeybindKey>) -> Self {
        Self::new_with(principle, std::iter::empty::<KeybindKey>())
    }

    pub fn new_with(
        principle: impl Into<KeybindKey>,
        modifiers: impl IntoIterator<Item = impl Into<KeybindKey>>,
    ) -> Self {
        Self {
            principle: principle.into(),
            modifiers: modifiers.into_iter().map(Into::into).collect_vec(),
        }
    }

    pub fn with(mut self, modifiers: impl IntoIterator<Item = impl Into<KeybindKey>>) -> Self {
        self.modifiers.extend(modifiers.into_iter().map(Into::into));
        self
    }

    /// If `self` is a subset of `other`, and they both have the same principle key
    fn is_subset(&self, other: &Self) -> bool {
        self.principle == other.principle
            && self
                .modifiers
                .iter()
                .copied()
                .all(|k| other.modifiers.contains(&k))
    }

    fn clashes(&self, other: &Self) -> bool {
        self.is_subset(other) || other.is_subset(self)
    }
}

struct ButtonMap {
    controls: EnumMap<ButtonInputs, ButtonControl>,
}

#[derive(
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
    strum::EnumIter,
)]
pub enum AxisInputs {
    MoveCameraY,
    MoveCameraX,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AxisControl {
    Gamepad(GamepadAxis),
    Virtual { hi: KeybindKey, lo: KeybindKey },
}

impl AxisControl {
    fn read(&self, ctx: ControlReadCtx) -> f32 {
        match self {
            AxisControl::Gamepad(axis) => ctx.gamepad.and_then(|g| g.get(*axis)).unwrap_or(0.),
            AxisControl::Virtual { hi, lo } => {
                if hi.read_pressed(ctx) {
                    1.
                } else if lo.read_pressed(ctx) {
                    -1.
                } else {
                    0.
                }
            }
        }
    }
}

struct AxisMap {
    controls: EnumMap<AxisInputs, AxisControl>,
}

struct ButtonState {
    prev_value: bool,
    value: bool,
}

impl ButtonState {
    fn push_value(&mut self, new_value: bool) {
        self.prev_value = self.value;
        self.value = new_value;
    }
}

struct AxisState {
    value: f32,
}

#[derive(Resource)]
struct ActionState {
    button_map: ButtonMap,
    buttons: EnumMap<ButtonInputs, ButtonState>,
    axis_map: AxisMap,
    axes: EnumMap<AxisInputs, AxisState>,
}

impl ActionState {
    pub fn pressed(&self, action: ButtonInputs) -> bool {
        self.buttons[action].value
    }

    pub fn just_pressed(&self, action: ButtonInputs) -> bool {
        self.buttons[action].value && !self.buttons[action].prev_value
    }

    pub fn read_axis(&self, axis: AxisInputs) -> f32 {
        self.axes[axis].value
    }
}

fn spawn_action_state(mut commands: Commands, settings: Res<PlayerSettings>) {
    let button_map = ButtonMap {
        controls: settings.controls.button_controls.clone(),
    };

    let axis_map = AxisMap {
        controls: settings.controls.axis_controls.clone(),
    };

    let action_state = ActionState {
        button_map,
        buttons: EnumMap::from_fn(|_| ButtonState {
            prev_value: false,
            value: false,
        }),
        axis_map,
        axes: EnumMap::from_fn(|_| AxisState { value: 0. }),
    };

    commands.insert_resource(action_state);
}

fn update_action_state(
    mut actions: ResMut<ActionState>,
    gamepads: Query<(&Name, &Gamepad)>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,

    hovering_ships: Query<&Hovering>,
) {
    let (_gamepad_name, gamepad) = gamepads.single().ok().unzip();
    let ctx = ControlReadCtx {
        gamepad,
        keyboard: &*keyboard,
        mouse: &*mouse,
    };

    for axis in AxisInputs::iter() {
        let control = &actions.axis_map.controls[axis];
        let value = control.read(ctx);
        actions.axes[axis].value = value;
    }

    let mut buttons_with_lower_priority: EnumMap<ButtonInputs, Vec<ButtonInputs>> =
        EnumMap::default();
    let mut buttons_with_higher_priority: EnumMap<ButtonInputs, Vec<ButtonInputs>> =
        EnumMap::default();

    for button in ButtonInputs::iter() {
        let control = &actions.button_map.controls[button];
        for other in ButtonInputs::iter() {
            if button == other {
                continue;
            }
            let other_control = &actions.button_map.controls[other];
            if !control.clashes(other_control) {
                continue;
            }

            if button.priority() > other.priority() {
                buttons_with_lower_priority[button].push(other);
                buttons_with_higher_priority[other].push(button);
            } else if button.priority() == other.priority() && other_control.is_subset(control) {
                buttons_with_lower_priority[button].push(other);
                buttons_with_higher_priority[other].push(button);
            }
        }
    }

    let mut has_completed: EnumMap<ButtonInputs, bool> = EnumMap::default();
    loop {
        for button in ButtonInputs::iter() {
            if has_completed[button] {
                continue;
            }
            // We depend on the buttons with a higher priority having been computed
            if buttons_with_higher_priority[button]
                .iter()
                .any(|x| !has_completed[*x])
            {
                continue;
            }
            if buttons_with_higher_priority[button]
                .iter()
                .any(|b| actions.buttons[*b].value)
            {
                actions.buttons[button].push_value(false);
                has_completed[button] = true;
                continue;
            }

            let all_modifiers_pressed = actions.button_map.controls[button]
                .modifiers
                .iter()
                .all(|k| k.read_pressed(ctx));
            let principle_pressed = actions.button_map.controls[button]
                .principle
                .read_pressed(ctx);
            let principle_just_pressed = actions.button_map.controls[button]
                .principle
                .read_just_pressed(ctx);

            let special_conditions_fulfilled =
                button
                    .special_conditions()
                    .into_iter()
                    .all(|condition| match condition {
                        SpecialCondition::HoveringOverEnemyShip => hovering_ships.single().is_ok(),
                    });

            let state = &mut actions.buttons[button];

            let is_now_pressed = match state.value {
                true => special_conditions_fulfilled && all_modifiers_pressed && principle_pressed,
                false => {
                    special_conditions_fulfilled && all_modifiers_pressed && principle_just_pressed
                }
            };

            state.push_value(is_now_pressed);
            has_completed[button] = true;
        }
        if has_completed.values().copied().all(identity) {
            break;
        }
    }
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
    for scroll in mouse_scroll.read() {
        // We want it so that scrolling by 10 once is equivalent to scrolling
        // by 1 ten times. Keep in mind that the change in zoom is based on the
        // current zoom
        // If we were to apply a large scroll all at once,
        // the zoom "speed" would be based entirely on the starting zoom value.
        // So, scrolling by 10 once would scroll much too far
        //
        // One way to approximate this is by doing the following `N` times:
        // zoom = zoom - zoom * scroll_speed * scroll.y * (1 / N)
        //
        // Let's define `a = scroll_speed * scroll.y` and `z(i) = zoom at step i`
        // and rewrite this as the following:
        // z(i+1) = z(i) - z(i) * a / N
        // z(i+1) = (1 - a / N) * z(i)
        // z(i+1) = (1 - a / N)^i * z(0)
        // Our final approximation will be at:
        // z(N) = (1 - a / N)^N * z(0)
        // And we want our appoximation to be perfectly accurate,
        // so let N go to infinity:
        // z = (1 - a / inf)^inf * z(0)
        // z = z(0) / e^a
        //
        // This isn't really necessary
        zoom.0 = zoom.0 * f32::exp(-scroll.y * scroll_speed);
    }
    zoom.0 = zoom.0.clamp(0.5, 50.);
}

fn update_hovering(
    mut commands: Commands,
    ships: Query<(Entity, &Team, &Ship, &Transform, &DetectionStatus)>,
    cursor_pos: Res<CursorWorldPos>,
    zoom: Res<MapZoom>,
    this_client: Res<ThisClient>,
) {
    for (ship, ship_team, _ship, ship_trans, ship_detection) in ships {
        if !ship_team.is_this_client(*this_client) && *ship_detection == DetectionStatus::Never {
            continue;
        }
        if cursor_pos.0.distance(ship_trans.translation.truncate())
            <= crate::SHIP_SELECTION_SIZE * zoom.0
        {
            commands.entity(ship).insert_if_new(Hovering);
        } else {
            commands.entity(ship).try_remove::<Hovering>();
        }
    }
}

fn update_camera(
    mut camera: Query<(&mut Projection, &mut Transform), With<MainCamera>>,
    actions: Res<ActionState>,
    zoom: Res<MapZoom>,
    time: Res<Time>,
) {
    let mut camera = camera.single_mut().unwrap();
    let Projection::Orthographic(proj) = &mut *camera.0 else {
        panic!()
    };

    proj.scale = zoom.0;
    let dir = vec2(
        actions.read_axis(AxisInputs::MoveCameraX),
        actions.read_axis(AxisInputs::MoveCameraY),
    )
    .normalize_or_zero();
    camera.1.translation += (dir * 200. * zoom.0 * time.delta_secs()).extend(0.);
}

fn update_selection(
    mut commands: Commands,
    ships: Query<(Entity, &Transform, Option<&Selected>, &Team), With<Ship>>,
    hovering: Query<(Entity, &Team, &Hovering)>,
    actions: Res<ActionState>,
    this_client: Res<ThisClient>,
) {
    let old_selection = ships
        .iter()
        .filter_map(|(ship, _, selected, _)| selected.map(|_| ship))
        .collect_vec();

    if actions.just_pressed(ButtonInputs::SetSelectedShip) {
        for (ship, ship_team, _hovering) in hovering {
            if ship_team.is_this_client(*this_client) {
                commands.entity(ship).insert_if_new(Selected);
            }
        }
    }

    if actions.just_pressed(ButtonInputs::ClearSelectedShips) {
        for ship in old_selection {
            commands.entity(ship).remove::<Selected>();
        }
    }
}

fn update_selected_ship_orders(
    mut commands: Commands,
    actions: Res<ActionState>,
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

        if actions.just_pressed(ButtonInputs::SetFireTarg) {
            if let Some(new_targ) = all_ships.iter().find(|maybe_targ| {
                !maybe_targ.2.is_this_client(*this_client)
                    && *maybe_targ.3 != DetectionStatus::Never
                    && maybe_targ.1.translation.truncate().distance(mouse_pos.0)
                        <= crate::SHIP_SELECTION_SIZE * zoom.0
            }) {
                new_fire_target = Some(Some(FireTarget { ship: new_targ.0 }));
            }
        }
        if actions.just_pressed(ButtonInputs::ClearFireTarg) {
            new_fire_target = Some(None);
        }

        if actions.just_pressed(ButtonInputs::SetWaypoint) {
            new_move_order = Some(MoveOrder {
                waypoints: vec![mouse_pos.0],
            });
        }
        if actions.just_pressed(ButtonInputs::PushWaypoint) {
            if let Some(mut move_order) = ship.5 {
                move_order.waypoints.push(mouse_pos.0);
                new_move_order = Some(move_order.clone());
            } else {
                new_move_order = Some(MoveOrder {
                    waypoints: vec![mouse_pos.0],
                });
            }
        }
        if actions.just_pressed(ButtonInputs::ClearWaypoints) {
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
    actions: Res<ActionState>,
    mut server: ResMut<ServerConnection>,
    shared_entities: Res<SharedEntityTracking>,
) {
    let Ok((selected_entity, selected_ship)) = selected_ships.single() else {
        return;
    };
    let consumables = &selected_ship.template.consumables;
    // Smoke
    if actions.just_pressed(ButtonInputs::UseConsumableSmoke) {
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
    actions: Res<ActionState>,
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

            if actions.just_pressed(ButtonInputs::FireTorpVolley) {
                let _ = server.send(Message::Client2Match(Client2Match::LaunchTorpedoVolley {
                    ship: shared_entities[selected],
                    dir: fire_dir,
                }));
            }
        }
    };
}
