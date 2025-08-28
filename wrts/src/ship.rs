use std::{cell::Cell, time::Duration};

use bevy::{prelude::*, window::PrimaryWindow};
use itertools::{Itertools, iproduct};
use ordered_float::OrderedFloat;
use wrts_match_shared::ship_template::ShipTemplate;

use crate::{
    AppState, DetectionStatus, Health, MainCamera, MapZoom, PlayerSettings, Selected, Team,
    networking::ThisClient,
};

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShipDisplaySystem;
#[derive(Debug, Clone, Copy)]
pub struct ShipDisplayPlugin;

impl Plugin for ShipDisplayPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            Update,
            ShipDisplaySystem.run_if(in_state(AppState::InMatch)),
        )
        .add_systems(
            Update,
            (
                destroy_dead_ship_uis,
                // UI element updaters
                (update_torpedo_reload_display)
                    .after(destroy_dead_ship_uis)
                    .before(sort_ship_modifiers_display),
                // ...
                sort_ship_modifiers_display,
                update_ship_ui_position,
                update_ship_sprites,
            )
                .in_set(ShipDisplaySystem),
        );
    }
}

#[derive(Debug)]
pub struct TurretState {
    /// Relative to ship-space
    pub dir: f32,
}

#[derive(Component, Debug)]
#[require(DetectionStatus, Health, Sprite, Transform, Team)]
pub struct Ship {
    pub template: &'static ShipTemplate,
    pub turret_states: Vec<TurretState>,
    pub reloaded_torp_volleys: usize,
    /// Remaining time until each reloading volley is reading,
    /// in ascending order
    pub reloading_torp_volleys_remaining_time: Vec<Duration>,
}
#[derive(Component, Debug)]
#[require(Node)]
pub struct ShipUI {
    pub tracked_ship: Entity,
}

#[derive(Component, Debug)]
#[require(Text)]
pub struct ShipNameTag;

#[derive(Component, Debug, Clone, Copy)]
#[require(Node)]
pub struct ShipModifiersDisplay {
    pub tracked_ship: Entity,
}

/// Has 1 child for each torpedo volley on this ship
#[derive(Component, Debug, Clone, Copy)]
#[require(Node)]
struct TorpedoReloadDisplay;

#[derive(Component, Debug, Clone, Copy)]
#[require(Node, Sprite)]
struct TorpedoReloadDisplayTorpedoStatus;

fn update_torpedo_reload_display(
    mut commands: Commands,
    ships: Query<(Entity, &Ship)>,
    ship_modifiers_displays: Query<(Entity, &ShipModifiersDisplay, Option<&Children>)>,
    mut torpedo_reload_displays: Query<(&TorpedoReloadDisplay, &Children)>,
    mut torpedo_reload_display_torpedo_statuses: Query<(
        &TorpedoReloadDisplayTorpedoStatus,
        &mut ImageNode,
        &Children,
    )>,
    mut torpedo_reload_display_torpedo_statuses_layer1: Query<
        (&mut Node, &mut ImageNode),
        Without<TorpedoReloadDisplayTorpedoStatus>,
    >,
) {
    let total_sprite_size = vec2(6., 20.);

    for (ship_entity, ship) in ships {
        let Some((disp_entity, _, disp_children)) = ship_modifiers_displays
            .iter()
            .find(|(_, disp, _)| disp.tracked_ship == ship_entity)
        else {
            continue;
        };
        let Some(torpedo_reload_display) = disp_children.and_then(|disp_children| {
            disp_children
                .iter()
                .find(|e| torpedo_reload_displays.contains(*e))
        }) else {
            if let Some(torps) = &ship.template.torpedoes {
                let id = commands.spawn(TorpedoReloadDisplay).id();
                let c = (0..torps.volleys)
                    .map(|_| {
                        commands
                            .spawn((
                                Node {
                                    width: Val::Px(total_sprite_size.x),
                                    height: Val::Px(total_sprite_size.y),
                                    margin: UiRect::all(Val::Px(3.)),
                                    ..default()
                                },
                                TorpedoReloadDisplayTorpedoStatus,
                                ImageNode::default(),
                                children![(
                                    Node {
                                        width: Val::Percent(100.),
                                        height: Val::Percent(100.),
                                        ..default()
                                    },
                                    ImageNode::default(),
                                )],
                            ))
                            .id()
                    })
                    .collect_vec();
                commands.entity(disp_entity).add_child(id);
                commands.entity(id).add_children(&c);
            }
            continue;
        };

        let torpedoes = ship.template.torpedoes.as_ref().unwrap();

        let (_torpedo_reload_display, torpedo_reload_display_children) = torpedo_reload_displays
            .get_mut(torpedo_reload_display)
            .expect("unreachable");

        for i in 0..torpedo_reload_display_children.len() {
            let (_, mut torp_status_bottom_layer, torp_status_children) =
                torpedo_reload_display_torpedo_statuses
                    .get_mut(torpedo_reload_display_children[i])
                    .expect("unreachable");

            let (mut torp_status_top_layer_node, mut torp_status_top_layer) =
                torpedo_reload_display_torpedo_statuses_layer1
                    .get_mut(
                        torp_status_children
                            .iter()
                            .find(|&e| torpedo_reload_display_torpedo_statuses_layer1.contains(e))
                            .expect("unreachable"),
                    )
                    .expect("unreachable");

            let is_reloaded = ship.reloaded_torp_volleys > i;
            let bar_loading = Color::linear_rgb(0.6, 0.1, 0.1);
            let bar_grey = Color::linear_rgb(0.1, 0.1, 0.1);
            let bar_loaded = Color::linear_rgb(0.1, 0.4, 0.8);
            match is_reloaded {
                true => {
                    *torp_status_bottom_layer =
                        ImageNode::solid_color(bar_loaded).with_mode(NodeImageMode::Stretch);
                    *torp_status_top_layer = ImageNode::default();
                    torp_status_top_layer.rect = None;
                }
                false => {
                    *torp_status_bottom_layer =
                        ImageNode::solid_color(bar_loading).with_mode(NodeImageMode::Stretch);
                    let cutoff_lerp = ship.reloading_torp_volleys_remaining_time
                        [i - ship.reloaded_torp_volleys]
                        .as_secs_f32()
                        / torpedoes.reload.as_secs_f32();
                    *torp_status_top_layer =
                        ImageNode::solid_color(bar_grey).with_mode(NodeImageMode::Stretch);
                    torp_status_top_layer_node.height =
                        Val::Percent((100. * cutoff_lerp).clamp(0., 100.));
                    // torp_status_top_layer_node.width = Val::Px(total_sprite_size.x);
                }
            }
        }
    }
}

fn update_ship_ui_position(
    camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    window: Query<&Window, With<PrimaryWindow>>,
    ships: Query<&Transform>,
    ship_modifiers_displays: Query<(&ShipUI, &mut Node, &ComputedNode)>,
) {
    let Ok((camera, camera_trans)) = camera.single() else {
        return;
    };
    let Ok(window) = window.single() else {
        return;
    };
    for (disp, mut disp_node, disp_computed_node) in ship_modifiers_displays {
        let Ok(ship_trans) = ships.get(disp.tracked_ship) else {
            continue;
        };
        let Ok(pos) = camera.world_to_viewport(camera_trans, ship_trans.translation) else {
            continue;
        };

        let content_size =
            disp_computed_node.content_size() * camera.target_scaling_factor().unwrap_or(1.);

        disp_node.left = Val::Px(pos.x - content_size.x / 2.);
        disp_node.top = Val::Px(pos.y + 20.);
        // disp_node.bottom = Val::Px(window.height() - pos.y - 30. - content_size.y / 2.);
    }
}

fn destroy_dead_ship_uis(
    mut commands: Commands,
    ship_uis: Query<(Entity, &ShipUI)>,
    ships: Query<(), With<Ship>>,
) {
    for (ship_ui_entity, ship_ui) in ship_uis {
        if !ships.contains(ship_ui.tracked_ship) {
            commands.entity(ship_ui_entity).despawn();
        }
    }
}

/// Sort all existing modifier displays
fn sort_ship_modifiers_display(
    mut commands: Commands,
    ships: Query<(Entity, &Team), With<Ship>>,
    ship_modifiers_displays: Query<(Entity, &ShipModifiersDisplay, &Children)>,
    torpedo_reload_displays: Query<(), With<TorpedoReloadDisplay>>,
    this_client: Res<ThisClient>,
) {
    for (ship_entity, ship_team) in ships {
        let Some((disp_entity, _, disp_children)) = ship_modifiers_displays
            .iter()
            .find(|(_, disp, _)| disp.tracked_ship == ship_entity)
        else {
            continue;
        };
        assert!(ship_team.is_this_client(*this_client));

        let children_ordered = disp_children
            .into_iter()
            .copied()
            .sorted_by_key(|&entity| {
                if torpedo_reload_displays.contains(entity) {
                    0
                } else {
                    u32::MAX
                }
            })
            .collect_vec();
        commands
            .entity(disp_entity)
            .replace_children(&children_ordered);
    }
}

fn update_ship_sprites(
    mut gizmos: Gizmos,
    ships: Query<(
        &Team,
        &Ship,
        &mut Sprite,
        &Transform,
        Option<&Selected>,
        &DetectionStatus,
        &Health,
    )>,
    this_client: Res<ThisClient>,
    settings: Res<PlayerSettings>,
    zoom: Res<MapZoom>,
) {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum DisplayType {
        Accurate,
        Simplified,
    }
    for (team, ship, mut sprite, trans, selected, detection_status, health) in ships {
        let is_visible =
            team.is_this_client(*this_client) || *detection_status == DetectionStatus::Detected;
        let is_selected = selected.is_some();

        let (display_type, sprite_size) = {
            let simplified_size = vec2(1., 1.) * settings.ship_icon_scale * zoom.0;
            let accurate_size = vec2(ship.template.hull.length, ship.template.hull.width);
            if simplified_size.max_element() > accurate_size.max_element() {
                (DisplayType::Simplified, simplified_size)
            } else {
                (DisplayType::Accurate, accurate_size)
            }
        };

        let sprite_bounds = {
            let rot = Vec2::from_angle(trans.rotation.to_euler(EulerRot::ZYX).0);
            let corners = iproduct!([-1., 1.], [-1., 1.])
                .map(|(x, y)| rot.rotate(0.5 * sprite_size * vec2(x, y)))
                .collect_vec();

            let mut max = corners[0];
            let mut min = corners[0];

            for c in corners {
                max = max.max(c);
                min = min.min(c);
            }

            Rect::from_corners(max, min)
        };

        // Turrets
        if is_visible && display_type == DisplayType::Accurate {
            let turrets = ship.template.turret_instances.as_slice();
            for turret_idx in 0..turrets.len() {
                let &TurretState { dir: dir_relative } = &ship.turret_states[turret_idx];
                let dir_absolute = trans.rotation.to_euler(EulerRot::ZXY).0 + dir_relative;
                let pos =
                    turrets[turret_idx].absolute_pos(trans.translation.truncate(), trans.rotation);
                let delta = Vec2::from_angle(dir_absolute) * 30.;
                gizmos.arrow_2d(pos, pos + delta, Color::linear_rgb(0.8, 0.8, 0.8));
            }
        }

        // HP bar
        if team.is_this_client(*this_client) || *detection_status != DetectionStatus::Never {
            let hp_bar_progress = (health.0 / ship.template.max_health) as f32;
            let hp_bar_y = trans.translation.y + 0.5 * sprite_bounds.height() + 3. * zoom.0;
            let hp_bar_dims = vec2(35., 5.) * zoom.0;
            let hp_bar_start = trans.translation.x - hp_bar_dims.x / 2.;
            let hp_bar_end = trans.translation.x + hp_bar_dims.x / 2.;
            let hp_bar_mid = hp_bar_start.lerp(hp_bar_end, hp_bar_progress);
            gizmos.line_2d(
                vec2(hp_bar_start, hp_bar_y),
                vec2(hp_bar_mid, hp_bar_y),
                Color::linear_rgb(0.9, 0.1, 0.1),
            );
            gizmos.line_2d(
                vec2(hp_bar_mid, hp_bar_y),
                vec2(hp_bar_end, hp_bar_y),
                Color::linear_rgb(0.1, 0.1, 0.1),
            );
        }

        if !team.is_this_client(*this_client) && *detection_status != DetectionStatus::Detected {
            *sprite = Sprite::default();
            continue;
        } else {
            let dim = match is_selected {
                true => 0.7,
                false => 1.0,
            };
            *sprite = Sprite::from_color(
                Color::LinearRgba(
                    settings
                        .team_colors(*team, *this_client)
                        .ship_color
                        .to_linear()
                        * dim,
                )
                .with_alpha(1.),
                sprite_size,
            );
        }

        if is_visible {
            // Gun range circle
            if let Some(t) = ship
                .template
                .turret_templates
                .values()
                .max_by_key(|t| OrderedFloat(t.max_range))
            {
                gizmos
                    .circle_2d(
                        Isometry2d::from_translation(trans.translation.truncate()),
                        t.max_range,
                        settings
                            .team_colors(*team, *this_client)
                            .gun_range_ring_color,
                    )
                    .resolution(128);
            }

            // Detection circle
            gizmos
                .circle_2d(
                    Isometry2d::from_translation(trans.translation.truncate()),
                    ship.template.detection,
                    Color::linear_rgb(0.4, 0.4, 0.9),
                )
                .resolution(128);
        }
    }
}
