use std::{collections::HashSet, convert::identity, f32::consts::FRAC_PI_2};

use bevy::prelude::*;
use itertools::Itertools;
use leafwing_input_manager::{
    Actionlike,
    prelude::{ButtonlikeChord, GamepadStick, InputMap, VirtualDPad},
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
        app.add_plugins(leafwing_input_manager::plugin::InputManagerPlugin::<
            InputAction,
        >::default())
            //
            .configure_sets(OnEnter(AppState::InMatch), InputHandlingSystem)
            .add_systems(
                OnEnter(AppState::InMatch),
                spawn_input_map.in_set(InputHandlingSystem),
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
    SetFireTarg,
    SetWaypoint,
    PushWaypoint,

    UseConsumableSmoke,
}

impl InputAction {
    /// Actions with a higher priority are performed when it overlaps with another
    fn priority(self) -> i32 {
        use InputAction::*;
        match self {
            PushWaypoint => 1,
            SetWaypoint | SetFireTarg | UseConsumableSmoke => 0,
            MoveCamera => -1,
        }
    }
}

fn spawn_input_map(mut commands: Commands, settings: Res<PlayerSettings>) {
    let mut input_map = InputMap::default();

    input_map.insert_dual_axis(InputAction::MoveCamera, GamepadStick::LEFT);
    input_map.insert_dual_axis(InputAction::MoveCamera, VirtualDPad::wasd());

    for (action, inputs) in settings
        .controls
        .button_controls
        .iter()
        .sorted_by_key(|(action, _)| action.priority())
    {
        input_map.insert(action, ButtonlikeChord::new(inputs.iter().cloned()));
    }

    commands.spawn(input_map);
}

fn only_modifier_keys_pressed(
    keyboard: impl AsRef<ButtonInput<KeyCode>>,
    modifier_keys: impl IntoIterator<Item = KeyCode>,
) -> bool {
    use KeyCode::*;
    let keyboard = keyboard.as_ref();
    let modifier_keys_needed: HashSet<KeyCode> = modifier_keys.into_iter().collect();
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
                <= crate::SHIP_SELECTION_SIZE * zoom.0
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
                        <= crate::SHIP_SELECTION_SIZE * zoom.0
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

fn use_consumables(
    mut commands: Commands,
    selected_ships: Query<(Entity, &Ship), With<Selected>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut server: ResMut<ServerConnection>,
    shared_entities: Res<SharedEntityTracking>,
) {
    let Ok((selected_entity, selected_ship)) = selected_ships.single() else {
        return;
    };
    let consumables = &selected_ship.template.consumables;
    // Smoke
    if keyboard.just_pressed(KeyCode::Digit1) && only_modifier_keys_pressed(&keyboard, []) {
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
    cursor_pos: Res<CursorWorldPos>,
    mouse: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
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

            if mouse.just_pressed(MouseButton::Right) && only_modifier_keys_pressed(&keyboard, []) {
                let _ = server.send(Message::Client2Match(Client2Match::LaunchTorpedoVolley {
                    ship: shared_entities[selected],
                    dir: fire_dir,
                }));
            }
        }
    };
}
