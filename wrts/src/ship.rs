use std::{cell::Cell, time::Duration};

use bevy::{prelude::*, window::PrimaryWindow};
use itertools::{Itertools, iproduct};
use ordered_float::OrderedFloat;
use wrts_match_shared::ship_template::ShipTemplate;
use wrts_messaging::ClientId;

use crate::{
    AppState, DetectionStatus, Health, MainCamera, MapZoom, PlayerSettings, Selected, Team,
    networking::ThisClient,
};

const CONSUMABLE_CHARGING_COLOR: Color = Color::linear_rgb(0.6, 0.1, 0.1);
const CONSUMABLE_READY_COLOR: Color = Color::linear_rgb(0.1, 0.4, 0.8);

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
                (
                    update_torpedo_reload_display,
                    update_smoke_consumable_display,
                )
                    .after(destroy_dead_ship_uis)
                    .before(sort_ship_modifiers_display),
                // ...
                sort_ship_modifiers_display,
                update_ship_ui_position,
                update_ship_sprites,
                update_detection_indicator_display,
                update_shaded_progress_bars.after(sort_ship_modifiers_display),
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

/// Attached to `ShipUI` and its children
#[derive(Component, Debug, Clone, Copy)]
pub struct ShipUITrackedShip(pub Entity);

#[derive(Component, Debug)]
#[require(Node)]
pub struct ShipUI;

#[derive(Component, Debug)]
pub struct ShipUIFirstRow;

#[derive(Component, Debug, Clone, Copy)]
#[require(Node)]
pub struct ShipModifiersDisplay;

/// Has 1 child for each torpedo volley on this ship
#[derive(Component, Debug, Clone, Copy)]
#[require(Node)]
struct TorpedoReloadDisplay;

#[derive(Component, Debug, Clone, Copy)]
#[require(Node, Sprite)]
struct TorpedoReloadDisplayTorpedoStatus;

#[derive(Component, Debug, Clone, Copy)]
pub struct SmokeConsumableState {
    pub charges_unused: Option<u16>,
    pub action_state: SmokeConsumableActionState,
}

#[derive(Debug, Clone, Copy)]
pub enum SmokeConsumableActionState {
    Deploying { time_remaining: Duration },
    Recharging { time_remaining: Duration },
    Recharged,
}

#[derive(Component, Debug, Clone, Copy)]
#[require(Node)]
struct SmokeConsumableDisplay;

#[derive(Component, Debug, Clone, Copy)]
#[require(Node, ImageNode)]
pub struct DetectionIndicatorDisplay;

fn make_shaded_progress_bar(
    mut commands: Commands,
    parent: Option<Entity>,
    node: Node,
    loaded_image: ImageNode,
    top_image: ImageNode,
    base_image: ImageNode,
) -> Entity {
    let bar = commands
        .spawn((
            node,
            ShadedProgressBar {
                progress: 0.,
                loaded_image,
                top_image,
                base_image,
            },
            ImageNode::default(),
            children![(
                Node {
                    width: Val::Percent(100.),
                    height: Val::Percent(100.),
                    ..default()
                },
                ImageNode::default()
            )],
        ))
        .id();
    if let Some(parent) = parent {
        commands.entity(parent).add_child(bar);
    }
    bar
}

#[derive(Component, Debug)]
#[require(Node, ImageNode)]
struct ShadedProgressBar {
    /// From 0 to 1
    progress: f32,
    /// Displayed only at progress >= 1
    loaded_image: ImageNode,
    /// More of this image is shown with a higher progress,
    /// up to 100% near a progress of 1
    top_image: ImageNode,
    /// More of this image is shown with a lower progress,
    /// up to 100% at a progress of 0
    base_image: ImageNode,
}

fn update_shaded_progress_bars(
    bars: Query<(Entity, &ShadedProgressBar, &Children)>,
    mut nodes: Query<(&mut Node, &mut ImageNode)>,
) {
    for (bar_entity, bar, bar_children) in bars {
        let [(mut _bot_node, mut bot_img), (mut top_node, mut top_img)] =
            nodes.get_many_mut([bar_entity, bar_children[0]]).unwrap();

        match bar.progress >= 1. {
            true => {
                top_node.height = Val::Percent(100.);
                *top_img = bar.loaded_image.clone();
                *bot_img = ImageNode::default();
            }
            false => {
                top_node.height = Val::Percent((100. * bar.progress).clamp(0., 100.));
                *top_img = bar.top_image.clone();
                *bot_img = bar.base_image.clone();
            }
        }
    }
}

fn update_torpedo_reload_display(
    mut commands: Commands,
    ships: Query<(Entity, &Ship)>,
    ship_modifiers_displays: Query<(
        Entity,
        &ShipUITrackedShip,
        &ShipModifiersDisplay,
        Option<&Children>,
    )>,
    mut torpedo_reload_displays: Query<(&TorpedoReloadDisplay, &Children)>,
    mut torpedo_reload_display_torpedo_statuses: Query<
        &Children,
        With<TorpedoReloadDisplayTorpedoStatus>,
    >,
    mut progress_bars: Query<&mut ShadedProgressBar>,
) {
    let total_sprite_size = vec2(6., 20.);

    let bar_grey_color = Color::linear_rgb(0.1, 0.1, 0.1);
    for (ship_entity, ship) in ships {
        let Some((disp_entity, _, _, disp_children)) = ship_modifiers_displays
            .iter()
            .find(|(_, disp_tracked_ship, _, _)| disp_tracked_ship.0 == ship_entity)
        else {
            continue;
        };
        let Some(torpedo_reload_display) = disp_children.and_then(|disp_children| {
            disp_children
                .iter()
                .find(|e| torpedo_reload_displays.contains(*e))
        }) else {
            if let Some(torps) = &ship.template.torpedoes {
                let id = commands
                    .spawn((ShipUITrackedShip(ship_entity), TorpedoReloadDisplay))
                    .id();
                let c = (0..torps.volleys)
                    .map(|_| {
                        let torp_status_disp = commands
                            .spawn((
                                ShipUITrackedShip(ship_entity),
                                Node {
                                    width: Val::Px(total_sprite_size.x),
                                    height: Val::Px(total_sprite_size.y),
                                    margin: UiRect::all(Val::Px(3.)),
                                    ..default()
                                },
                                TorpedoReloadDisplayTorpedoStatus,
                            ))
                            .id();
                        make_shaded_progress_bar(
                            commands.reborrow(),
                            Some(torp_status_disp),
                            Node {
                                width: Val::Percent(100.),
                                height: Val::Percent(100.),
                                ..default()
                            },
                            ImageNode::solid_color(CONSUMABLE_READY_COLOR),
                            ImageNode::solid_color(bar_grey_color),
                            ImageNode::solid_color(CONSUMABLE_CHARGING_COLOR),
                        );

                        torp_status_disp
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
            let torp_status_children = torpedo_reload_display_torpedo_statuses
                .get_mut(torpedo_reload_display_children[i])
                .expect("unreachable");

            let mut progress_bar = progress_bars
                .get_mut(
                    torp_status_children
                        .iter()
                        .find(|&e| progress_bars.contains(e))
                        .expect("unreachable"),
                )
                .expect("unreachable");

            let is_reloaded = ship.reloaded_torp_volleys > i;
            match is_reloaded {
                true => {
                    progress_bar.progress = 2.;
                }
                false => {
                    let cutoff_lerp = ship.reloading_torp_volleys_remaining_time
                        [i - ship.reloaded_torp_volleys]
                        .as_secs_f32()
                        / torpedoes.reload.as_secs_f32();
                    progress_bar.progress = cutoff_lerp;
                }
            }
        }
    }
}

fn update_smoke_consumable_display(
    mut commands: Commands,
    ships: Query<(Entity, &Ship, &SmokeConsumableState)>,
    ship_modifiers_displays: Query<(
        Entity,
        &ShipUITrackedShip,
        &ShipModifiersDisplay,
        Option<&Children>,
    )>,
    mut smoke_consumable_displays: Query<(&SmokeConsumableDisplay, &Children)>,
    mut text_query: Query<&mut Text>,
    mut progress_bars: Query<&mut ShadedProgressBar>,
) {
    let total_sprite_size = vec2(15., 20.);

    for (ship_entity, ship, smoke_state) in ships {
        let Some((disp_entity, _, _, disp_children)) = ship_modifiers_displays
            .iter()
            .find(|(_, disp_tracked_ship, _, _)| disp_tracked_ship.0 == ship_entity)
        else {
            continue;
        };
        let Some(smoke) = ship.template.consumables.smoke() else {
            continue;
        };
        let Some(smoke_consumable_display) = disp_children.and_then(|disp_children| {
            disp_children
                .iter()
                .find(|e| smoke_consumable_displays.contains(*e))
        }) else {
            let smoke_icon_id = make_shaded_progress_bar(
                commands.reborrow(),
                None,
                Node {
                    width: Val::Px(total_sprite_size.x),
                    height: Val::Px(total_sprite_size.y),
                    margin: UiRect::all(Val::Px(3.)),
                    ..default()
                },
                ImageNode::default(),
                ImageNode::default(),
                ImageNode::default(),
            );

            let id = commands
                .spawn((
                    ShipUITrackedShip(ship_entity),
                    SmokeConsumableDisplay,
                    Node { ..default() },
                    children![
                        // Charge count
                        (
                            ShipUITrackedShip(ship_entity),
                            Node {
                                width: Val::Auto,
                                height: Val::Px(total_sprite_size.y),
                                margin: UiRect::all(Val::Px(3.)),
                                ..default()
                            },
                            Text("".into())
                        ),
                        // Smoke icon (added outside of this scope)
                        // ...
                    ],
                ))
                .id();
            commands.entity(disp_entity).add_child(id);
            commands.entity(id).add_child(smoke_icon_id);
            continue;
        };

        let (_smoke_consumable_display, smoke_consumable_display_children) =
            smoke_consumable_displays
                .get_mut(smoke_consumable_display)
                .unwrap();

        let mut charge_count_text = text_query
            .get_mut(smoke_consumable_display_children[0])
            .unwrap();

        let mut smoke_icon = progress_bars
            .get_mut(smoke_consumable_display_children[1])
            .unwrap();

        charge_count_text.0 = smoke_state
            .charges_unused
            .map_or("".into(), |n| format!("{}", n));

        // v The bar starts fully in colored by this color:
        let charging_top_img = ImageNode::solid_color(Color::linear_rgb(0., 0., 0.));
        let charging_base_img = ImageNode::solid_color(CONSUMABLE_CHARGING_COLOR);
        let charged_img = ImageNode::solid_color(CONSUMABLE_READY_COLOR);
        let deploying_top_img = ImageNode::solid_color(Color::linear_rgb(0.3, 0.7, 0.7));
        let deploying_base_img = ImageNode::solid_color(Color::linear_rgb(0.3, 0.3, 0.3));
        // ^ And ends up fully colored by this color, before
        // instantly returning to the top

        match smoke_state.action_state {
            SmokeConsumableActionState::Deploying { time_remaining } => {
                smoke_icon.progress =
                    time_remaining.as_secs_f32() / smoke.action_time.as_secs_f32();
                smoke_icon.top_image = deploying_top_img;
                smoke_icon.loaded_image = smoke_icon.top_image.clone();
                smoke_icon.base_image = deploying_base_img;
            }
            SmokeConsumableActionState::Recharging { time_remaining } => {
                smoke_icon.progress = time_remaining.as_secs_f32() / smoke.cooldown.as_secs_f32();
                smoke_icon.top_image = charging_top_img;
                smoke_icon.loaded_image = smoke_icon.top_image.clone();
                smoke_icon.base_image = charging_base_img;
            }
            SmokeConsumableActionState::Recharged => {
                smoke_icon.progress = 2.;
                smoke_icon.loaded_image = charged_img;
            }
        }
    }
}

fn update_detection_indicator_display(
    ships: Query<(&Ship, &Team, &DetectionStatus)>,
    detection_indicator_displays: Query<(
        &DetectionIndicatorDisplay,
        &ShipUITrackedShip,
        &mut Node,
        &mut ImageNode,
    )>,
    this_client: Res<ThisClient>,
) {
    let total_sprite_size = vec2(6., 20.);
    for (_disp, tracked_ship, mut node, mut image) in detection_indicator_displays {
        let Ok((_ship, ship_team, ship_detection)) = ships.get(tracked_ship.0) else {
            continue;
        };
        if *ship_detection == DetectionStatus::Never || !ship_team.is_this_client(*this_client) {
            node.width = Val::Px(0.);
            node.height = Val::Px(0.);
            *image = ImageNode::default();
            continue;
        }

        match ship_detection {
            DetectionStatus::Never => unreachable!(),
            DetectionStatus::Detected => {
                node.width = Val::Px(total_sprite_size.x);
                node.height = Val::Px(total_sprite_size.y);
                *image = ImageNode::solid_color(Color::srgb_u8(240, 208, 41));
            }
            DetectionStatus::UnDetected => {
                node.width = Val::Px(total_sprite_size.x);
                node.height = Val::Px(total_sprite_size.y);
                *image = ImageNode::solid_color(Color::srgb_u8(28, 26, 12));
            }
        }
    }
}

fn update_ship_ui_position(
    camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    ships: Query<&Transform>,
    ship_uis: Query<(&ShipUI, &ShipUITrackedShip, &mut Node, &ComputedNode)>,
) {
    let Ok((camera, camera_trans)) = camera.single() else {
        return;
    };
    for (_disp, disp_tracked, mut disp_node, disp_computed_node) in ship_uis {
        let Ok(ship_trans) = ships.get(disp_tracked.0) else {
            continue;
        };
        let Ok(pos) = camera.world_to_viewport(camera_trans, ship_trans.translation) else {
            continue;
        };

        let content_size =
            disp_computed_node.content_size() * camera.target_scaling_factor().unwrap_or(1.);

        disp_node.left = Val::Px(pos.x - content_size.x / 2.);
        disp_node.top = Val::Px(pos.y + 20.);
    }
}

fn destroy_dead_ship_uis(
    mut commands: Commands,
    ship_uis: Query<(Entity, &ShipUI, &ShipUITrackedShip)>,
    ships: Query<(), With<Ship>>,
) {
    for (ship_ui_entity, _ship_ui, ship_ui_tracked) in ship_uis {
        if !ships.contains(ship_ui_tracked.0) {
            commands.entity(ship_ui_entity).despawn();
        }
    }
}

/// Sort all existing modifier displays
fn sort_ship_modifiers_display(
    mut commands: Commands,
    ships: Query<(Entity, &Team), With<Ship>>,
    ship_modifiers_displays: Query<(Entity, &ShipUITrackedShip, &ShipModifiersDisplay, &Children)>,
    torpedo_reload_displays: Query<(), With<TorpedoReloadDisplay>>,
    smoke_consumable_displays: Query<(), With<SmokeConsumableDisplay>>,
    this_client: Res<ThisClient>,
) {
    for (ship_entity, ship_team) in ships {
        let Some((disp_entity, _, _, disp_children)) = ship_modifiers_displays
            .iter()
            .find(|(_, disp_tracked, _, _)| disp_tracked.0 == ship_entity)
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
                } else if smoke_consumable_displays.contains(entity) {
                    1
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
