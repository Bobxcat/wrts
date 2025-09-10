use std::{
    f32::consts::PI,
    ops::{Index, IndexMut},
    time::Duration,
};

use bevy::{prelude::*, window::ExitCondition};
use itertools::Itertools;
use ordered_float::OrderedFloat;
use wrts_match_shared::{
    formulas::{ProjectileHitCalc, ProjectileHitRes},
    ship_template::{AngleRange, BulletType, Caliber, Speed, TargetingMode},
};
use wrts_messaging::{ClientId, Match2Client, Message, WrtsMatchMessage};

use crate::{
    detection::{DetectionPlugin, DetectionStatus, DetectionSystems},
    initialize_game::initalize_game,
    math_utils::BulletProblemRes,
    networking::{ClientInfo, MessagesSend, NetworkingPlugin, SharedEntityTracking},
    ship::{
        Ship, SmokeConsumableState, SmokeDeploying, SmokePuff, TurretAimInfo, TurretStates,
        apply_dispersion,
    },
    spawn_entity::{DespawnNetworkedEntityCommand, SpawnBulletCommand, SpawnSmokePuffCommand},
};

mod detection;
mod initialize_game;
mod math_utils;
mod networking;
mod ship;
mod spawn_entity;

#[derive(Resource)]
struct GameRules {
    gravity: f32,
}

impl Default for GameRules {
    fn default() -> Self {
        Self { gravity: 10. }
    }
}

#[derive(Debug, Default, Component, Clone, Copy)]
#[require(Transform)]
struct Velocity(pub Vec3);

#[derive(Debug, Component, Default, Clone)]
struct Health(pub f64);

#[derive(Debug, Component, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Team(pub ClientId);

impl Default for Team {
    fn default() -> Self {
        Self(ClientId(u32::MAX))
    }
}

pub struct TeamMap<V> {
    entries: [(Team, V); 2],
}

impl<V> TeamMap<V> {
    fn index_of_team(&self, team: Team) -> usize {
        if self.entries[0].0 == team { 0 } else { 1 }
    }

    pub fn get_opposite_pair(&self, team: Team) -> (Team, &V) {
        let i = self.index_of_team(team) ^ 1;
        (self.entries[i].0, &self.entries[i].1)
    }

    pub fn get_opposite(&self, team: Team) -> &V {
        self.get_opposite_pair(team).1
    }
}

impl<V> FromIterator<(Team, V)> for TeamMap<V> {
    fn from_iter<T: IntoIterator<Item = (Team, V)>>(iter: T) -> Self {
        let entries = iter.into_iter().collect_array().unwrap();
        Self { entries }
    }
}

impl<V> Index<Team> for TeamMap<V> {
    type Output = V;

    fn index(&self, team: Team) -> &Self::Output {
        &self.entries[self.index_of_team(team)].1
    }
}

impl<V> IndexMut<Team> for TeamMap<V> {
    fn index_mut(&mut self, team: Team) -> &mut Self::Output {
        &mut self.entries[self.index_of_team(team)].1
    }
}

#[derive(Debug, Default, Component, Clone)]
struct MoveOrder {
    pub waypoints: Vec<Vec2>,
}

#[derive(Debug, Component, Clone)]
struct FireTarget {
    ship: Entity,
}

#[derive(Debug, Component, Clone)]
#[require(Team, Velocity)]
struct Torpedo {
    owning_ship: Entity,
    damage: f64,
    inital_pos: Vec2,
    max_range: f32,
}

fn torpedo_reloading(ships: Query<&mut Ship>, time: Res<Time>) {
    for mut ship in ships {
        for timer in &mut ship.torpedo_reloads {
            timer.tick(time.delta());
        }
    }
}

fn despawn_old_torpedoes(mut commands: Commands, torps: Query<(Entity, &Torpedo, &Transform)>) {
    for (torp_entity, torp, torp_trans) in torps {
        if torp_trans.translation.truncate().distance(torp.inital_pos) > torp.max_range {
            commands.queue(DespawnNetworkedEntityCommand {
                entity: torp_entity,
            });
        }
    }
}

fn collide_torpedoes(
    mut commands: Commands,
    mut ships: Query<(Entity, &Ship, &Team, &Transform, &mut Health)>,
    torpedoes: Query<(Entity, &Torpedo, &Team, &Transform)>,
) {
    for (torp_entity, torp, torp_team, torp_trans) in torpedoes {
        for (ship_entity, ship, ship_team, ship_trans, mut ship_health) in &mut ships {
            if *torp_team == *ship_team {
                continue;
            }
            if ship_health.0 <= 0. {
                continue;
            }
            // Calculate collisions in the local space of the ship hull
            let ship_rot_inv = Vec2::from_angle(-ship_trans.rotation.to_euler(EulerRot::ZXY).0);
            let (ship_hull_min, ship_hull_max) = ship.template.hull.to_bounds();
            let torp_pos = ship_rot_inv
                .rotate(torp_trans.translation.truncate() - ship_trans.translation.truncate());
            if Vec2::cmple(ship_hull_min.truncate(), torp_pos).all()
                && Vec2::cmple(torp_pos, ship_hull_max.truncate()).all()
            {
                let damage = torp.damage;
                ship_health.0 -= damage;
                commands.queue(DespawnNetworkedEntityCommand {
                    entity: torp_entity,
                });
                if ship_health.0 <= 0. {
                    commands.queue(DespawnNetworkedEntityCommand {
                        entity: ship_entity,
                    });
                }
            }
        }
    }
}

fn update_ship_velocity(
    ships: Query<(
        &mut Ship,
        &mut Transform,
        &mut Velocity,
        Option<&mut MoveOrder>,
        &Team,
        Entity,
    )>,
    time: Res<Time>,
    shared_entities: Res<SharedEntityTracking>,
    msgs_tx: Res<MessagesSend>,
) {
    for mut ship in ships {
        if let Some(move_order) = &mut ship.3 {
            if move_order
                .waypoints
                .get(0)
                .is_some_and(|next| next.distance(ship.1.translation.truncate()) <= 5.)
            {
                move_order.waypoints.remove(0);
                if let Some(shared) = shared_entities.get_by_local(ship.5) {
                    msgs_tx.send(WrtsMatchMessage {
                        client: ship.4.0,
                        msg: Message::Match2Client(Match2Client::SetMoveOrder {
                            id: shared,
                            waypoints: move_order.waypoints.clone(),
                        }),
                    });
                }
            }
        }

        let curr_dir = ship.1.rotation.to_euler(EulerRot::ZXY).0;

        let (targ_speed, targ_dir) = match ship
            .3
            .and_then(|order| order.waypoints.get(0).copied())
            .and_then(|next_waypoint| {
                Some((
                    next_waypoint,
                    Dir2::new(next_waypoint - ship.1.translation.truncate()).ok()?,
                ))
            }) {
            Some((next_waypoint, to_next_waypoint)) => {
                let dist = ship.1.translation.truncate().distance(next_waypoint);
                let targ_speed = ship.0.template.max_speed.mps().clamp(0., dist);
                let targ_dir = to_next_waypoint.to_angle();
                (targ_speed, targ_dir)
            }
            None => (0., curr_dir),
        };

        let (new_vel, new_dir) = {
            let turn_rate_limiter =
                f32::clamp(ship.0.curr_speed / Speed::from_kts(20.).mps(), 0., 1.);
            let new_dir = Vec2::from_angle(curr_dir).rotate_towards(
                Vec2::from_angle(targ_dir),
                turn_rate_limiter * ship.0.template.turning_rate.radps() * time.delta_secs(),
            );

            let speed_delta = targ_speed - ship.0.curr_speed;
            ship.0.curr_speed += f32::clamp(
                speed_delta.signum()
                    * ship.0.template.engine_acceleration.mps()
                    * time.delta_secs(),
                -speed_delta.abs(),
                speed_delta.abs(),
            );
            ship.0.curr_speed = ship.0.curr_speed.clamp(0., ship.0.template.max_speed.mps());

            (new_dir * ship.0.curr_speed, new_dir)
        };

        ship.1.rotation = Quat::from_rotation_z(new_dir.to_angle());
        ship.2.0 = new_vel.extend(0.);
    }
}

fn force_ship_in_map(ships: Query<&mut Transform, With<Ship>>) {
    let (lower, upper) = wrts_match_shared::map_bounds();

    for mut trans in ships {
        trans.translation = trans
            .translation
            .truncate()
            .clamp(lower, upper)
            .extend(trans.translation.z);
    }
}

#[derive(Debug, Component, Clone)]
#[require(Team, Sprite, Transform)]
struct Bullet {
    owning_ship: Entity,
    targ_ship: Entity,
    caliber: Caliber,
    ty: BulletType,
    inital_pos: Vec3,
    inital_vel: Vec3,
    curr_vel: Vec3,
    inital_aimpoint: Vec2,
    current_aimpoint: Vec2,
    expected_flight_time_total: Duration,
    current_flight_time: Duration,
    damage: f64,
}

fn move_bullets(
    mut commands: Commands,
    q: Query<(Entity, &mut Bullet, &mut Transform)>,
    targets: Query<(&Transform, &Velocity), Without<Bullet>>,
    rules: Res<GameRules>,
    time: Res<Time>,
) {
    for (entity, mut bullet, mut trans) in q {
        if let Ok((targ_trans, targ_vel)) = targets.get(bullet.targ_ship) {
            let rem_time = bullet
                .expected_flight_time_total
                .saturating_sub(bullet.current_flight_time)
                .as_secs_f32();
            bullet.current_aimpoint =
                targ_trans.translation.truncate() + targ_vel.0.truncate() * rem_time;
        };
        let aimpoint_adjustment = bullet.current_aimpoint - bullet.inital_aimpoint;
        // Use an explicit solution for position as a function of time,
        // with P(0) = (bullet inital position) + (aimpoint adjustment)
        // This has the nice benefit of being perfectly accurate
        let new_pos = vec3(
            0.,
            0.,
            -0.5 * rules.gravity * bullet.current_flight_time.as_secs_f32().powi(2),
        ) + bullet.inital_vel * bullet.current_flight_time.as_secs_f32()
            + bullet.inital_pos
            + aimpoint_adjustment.extend(0.);
        let new_vel = vec3(
            0.,
            0.,
            -rules.gravity * bullet.current_flight_time.as_secs_f32(),
        ) + bullet.inital_vel;
        bullet.current_flight_time += time.delta();

        if let Ok(dir) = Dir2::new((new_pos - trans.translation).truncate()) {
            trans.rotation = Quat::from_rotation_z(dir.to_angle());
        }
        bullet.curr_vel = new_vel;
        trans.translation = new_pos;

        if trans.translation.z <= -100. {
            commands.queue(DespawnNetworkedEntityCommand { entity });
        }
    }
}

fn collide_bullets(
    mut commands: Commands,
    bullets: Query<(Entity, &Bullet, &Transform, &Team)>,
    mut ships: Query<(Entity, &Ship, &Transform, &Team, &mut Health)>,
) {
    for (bullet_entity, bullet, bullet_trans, bullet_team) in bullets {
        for (ship_entity, ship, ship_trans, ship_team, mut ship_health) in &mut ships {
            if bullet_team == ship_team {
                continue;
            }
            if ship_health.0 <= 0. {
                continue;
            }

            let hit = ProjectileHitCalc {
                ship: ship.template.id,
                ship_pos: ship_trans.translation.truncate(),
                ship_rot: ship_trans.rotation,
                projectile_base_damage: bullet.damage,
                projectile_caliber: bullet.caliber,
                projectile_vel: bullet.curr_vel,
                projectile_pos: bullet_trans.translation,
            };

            if let ProjectileHitRes::Hit { damage_dealt } = hit.run() {
                ship_health.0 -= damage_dealt;

                commands.queue(DespawnNetworkedEntityCommand {
                    entity: bullet_entity,
                });
                if ship_health.0 <= 0. {
                    commands.queue(DespawnNetworkedEntityCommand {
                        entity: ship_entity,
                    });
                }
            }
        }
    }
}

fn turret_reloading(states: Query<&mut TurretStates>, time: Res<Time>) {
    for mut turrets in states {
        for turret in &mut turrets.states {
            turret.reload_timer.tick(time.delta());
        }
    }
}

fn update_turret_absolute_pos(ships: Query<(&Ship, &mut TurretStates, &Transform)>) {
    for (ship, mut turrets, ship_trans) in ships {
        for (turret, turret_state) in
            itertools::zip_eq(&ship.template.turret_instances, &mut turrets.states)
        {
            turret_state.absolute_pos =
                turret.absolute_pos(ship_trans.translation.truncate(), ship_trans.rotation);
        }
    }
}

fn aim_turrets(
    ships: Query<(
        Entity,
        &Team,
        &Ship,
        &Transform,
        &Velocity,
        &DetectionStatus,
        Option<&FireTarget>,
    )>,
    mut turret_states: Query<&mut TurretStates>,
    time: Res<Time>,
    rules: Res<GameRules>,
    teams: Query<&ClientInfo>,
) {
    let rules = &*rules;

    struct ShipQueryItem<'a> {
        entity: Entity,
        team: Team,
        ship: &'a Ship,
        trans: Transform,
        vel: Velocity,
        detection: &'a DetectionStatus,
        fire_targ: Option<FireTarget>,
    }

    let teams: [Team; 2] = teams
        .iter()
        .map(|cl| Team(cl.info.id))
        .collect_array()
        .unwrap();
    let ships_by_team: TeamMap<Vec<ShipQueryItem>> = {
        let (team0, team1) = ships
            .into_iter()
            .map(
                |(entity, team, ship, trans, vel, detection, fire_targ)| ShipQueryItem {
                    entity,
                    team: *team,
                    ship,
                    trans: *trans,
                    vel: *vel,
                    detection,
                    fire_targ: fire_targ.cloned(),
                },
            )
            .partition::<Vec<_>, _>(|item| item.team == teams[0]);
        TeamMap::from_iter([(teams[0], team0), (teams[1], team1)])
    };

    let turrets_iter = teams
        .into_iter()
        .flat_map(|team| (0..ships_by_team[team].len()).map(move |ship_idx| (team, ship_idx)))
        .flat_map(|(team, ship_idx)| {
            (0..ships_by_team[team][ship_idx]
                .ship
                .template
                .turret_instances
                .len())
                .map(move |turret_idx| (team, ship_idx, turret_idx))
        });

    for (team, ship_idx, turret_idx) in turrets_iter.collect_vec() {
        let team_opposite = if teams[0] == team { teams[1] } else { teams[0] };
        let ship_info = &ships_by_team[team][ship_idx];
        let turret_state = &mut turret_states.get_mut(ship_info.entity).unwrap().states[turret_idx];
        let turret_pos = turret_state.absolute_pos;
        let turret_instance = &ship_info.ship.template.turret_instances[turret_idx];
        let turret_template = turret_instance.turret_template();

        let (targ_info, bp) = {
            let do_bp_against_targ = move |fire_targ: &ShipQueryItem| -> Option<BulletProblemRes> {
                if !fire_targ.detection.is_detected {
                    return None;
                }
                math_utils::bullet_problem(
                    turret_pos,
                    fire_targ.trans.translation.truncate(),
                    fire_targ.vel.0.truncate(),
                    turret_template.muzzle_vel as f64,
                    rules.gravity as f64,
                )
                .filter(|bp| bp.intersection_dist < turret_template.max_range)
            };

            let bp_is_within_firing_angle = |bp: &BulletProblemRes| -> bool {
                turret_instance
                    .firing_angle
                    .or(turret_instance.movement_angle)
                    .is_some_and(|valid_angle| {
                        let targ_dir = Vec2::from_angle(
                            bp.projectile_azimuth
                                - ship_info.trans.rotation.to_euler(EulerRot::ZYX).0,
                        );
                        valid_angle.contains(targ_dir)
                    })
            };

            let fire_targ = ships_by_team[team][ship_idx]
                .fire_targ
                .clone()
                .and_then(|targ| {
                    ships_by_team[team_opposite]
                        .iter()
                        .find(|item| item.entity == targ.ship)
                })
                .filter(|targ_info| targ_info.detection.is_detected);

            let primary_targ = fire_targ
                .and_then(|fire_targ| do_bp_against_targ(fire_targ).map(|bp| (fire_targ, bp)));

            match (turret_template.targeting_mode, primary_targ) {
                // FireTarget is within range
                (TargetingMode::Primary, Some(primary_targ)) => primary_targ,
                (TargetingMode::Primary, None) => {
                    turret_state.aim_info = TurretAimInfo::NoValidTarget {};
                    continue;
                }

                (TargetingMode::Secondary, primary_targ) => {
                    let fallback_targs = ships_by_team[team_opposite]
                        .iter()
                        .sorted_by_key(|targ| {
                            OrderedFloat(
                                targ.trans
                                    .translation
                                    .distance_squared(ship_info.trans.translation),
                            )
                        })
                        .filter_map(|potential_targ| {
                            do_bp_against_targ(potential_targ).map(|bp| (potential_targ, bp))
                        });
                    if let Some(new_targ_found) = primary_targ
                        .into_iter()
                        .chain(fallback_targs)
                        .filter(|(_, bp)| bp_is_within_firing_angle(bp))
                        .next()
                    {
                        new_targ_found
                    } else {
                        turret_state.aim_info = TurretAimInfo::NoValidTarget {};
                        continue;
                    }
                }
            }
        };

        // Turn turret and make sure the turret's turned before firing

        // Directions here are all relative to ship-space
        let targ_dir = Vec2::from_angle(
            bp.projectile_azimuth - ship_info.trans.rotation.to_euler(EulerRot::ZYX).0,
        );
        let curr_dir = Vec2::from_angle(turret_state.dir);

        let rotate_dir = match turret_instance.movement_angle {
            Some(movement_angle) => {
                // Nudge the curr_dir so the turret doesn't get stuck at the edges of the movement angle
                let curr_dir_nudged_ccw = Vec2::from_angle(0.001).rotate(curr_dir);
                let curr_dir_nudged_cw = Vec2::from_angle(-0.001).rotate(curr_dir);
                if !AngleRange::from_vectors(curr_dir_nudged_ccw, targ_dir)
                    .overlaps(movement_angle.inverse())
                {
                    // If I can sweep from curr_dir to targ_dir without overlapping
                    // the place I'm not allowed to move, sweep counter clockwise
                    1.
                } else if !AngleRange::from_vectors(targ_dir, curr_dir_nudged_cw)
                    .overlaps(movement_angle.inverse())
                {
                    // If I can sweep from curr_dir to targ_dir *clockwise*
                    // without overlapping the place I'm not allowed to move,
                    // turn clockwise
                    -1.
                } else {
                    // The only way that this statement can be reached is
                    // if the target is outside our movement angle
                    let targ_dir_clamped = movement_angle.clamp_angle(targ_dir);
                    if targ_dir_clamped.distance_squared(movement_angle.end_dir()) <= 0.001 {
                        // Snapped to the end angle of the `movement_angle`
                        1.
                    } else {
                        // Snapped to the start angle of the `movement_angle`
                        -1.
                    }
                }
            }
            None => curr_dir.angle_to(targ_dir).signum(),
        };

        let new_dir = {
            let mut dir = curr_dir.rotate(Vec2::from_angle(
                rotate_dir * turret_template.turn_rate.radps() * time.delta_secs(),
            ));
            if let Some(movement_angle) = turret_instance.movement_angle {
                dir = movement_angle.clamp_angle(dir);
            }
            dir
        };
        turret_state.dir = new_dir.to_angle();

        let turret_not_aimed = new_dir.angle_to(targ_dir).abs() > PI / 180.;
        let turret_outside_firing_angle = if let Some(firing_angle) = turret_instance.firing_angle {
            !firing_angle.contains(new_dir)
        } else {
            false
        };
        let turret_cant_fire_this_frame = turret_not_aimed || turret_outside_firing_angle;

        turret_state.aim_info = match turret_cant_fire_this_frame {
            true => TurretAimInfo::AimingToTarget {
                target: targ_info.entity,
                bp,
            },
            false => TurretAimInfo::AimedAtTarget {
                target: targ_info.entity,
                bp,
            },
        };
    }
}

fn fire_bullets(
    mut commands: Commands,
    ships: Query<(Entity, &Team, &mut Ship, &mut TurretStates)>,
) {
    let mut ships = ships.into_iter().collect_vec();
    for (ship_idx, turret_idx) in (0..ships.len())
        .flat_map(|ship_idx| {
            (0..ships[ship_idx].2.template.turret_instances.len())
                .map(move |turret_idx| (ship_idx, turret_idx))
        })
        .collect_vec()
    {
        let (ship_entity, team, ship, turret_states) = &mut ships[ship_idx];
        let (ship_entity, team) = (*ship_entity, *team);

        let turret_instance = &ship.template.turret_instances[turret_idx];
        let turret_template = turret_instance.turret_template();

        let turret_state = &mut turret_states.states[turret_idx];

        let TurretAimInfo::AimedAtTarget { target, bp } = &turret_state.aim_info else {
            continue;
        };

        if !turret_state.reload_timer.finished() {
            continue;
        }

        for barrel_idx in 0..turret_template.barrel_count {
            let barrel_lateral_offset = (barrel_idx - (turret_template.barrel_count - 1) / 2)
                as f32
                * turret_template.barrel_spacing;

            let bullet_vel = apply_dispersion(&turret_template.dispersion, bp.projectile_dir)
                * turret_template.muzzle_vel as f32;

            let bullet_start = turret_state.absolute_pos
                + Vec2::from_angle(bp.projectile_azimuth).rotate(vec2(0., barrel_lateral_offset));
            // The bullet should start very slightly above the water,
            // but not by very much since ships have a small draft so
            // it would mean a lot more missing
            let bullet_start = bullet_start.extend(0.01);

            let bullet = Bullet {
                owning_ship: ship_entity,
                targ_ship: *target,
                caliber: Caliber::from_mm(300.),
                ty: BulletType::AP,
                inital_pos: bullet_start,
                inital_vel: bullet_vel,
                curr_vel: bullet_vel,
                inital_aimpoint: bp.intersection_point,
                current_aimpoint: bp.intersection_point,
                expected_flight_time_total: Duration::from_secs_f32(bp.intersection_time),
                current_flight_time: Duration::ZERO,
                damage: turret_template.damage,
            };

            commands.queue(SpawnBulletCommand {
                team: *team,
                bullet,
                update_firing_detection_timer: Some(Duration::from_secs(20)),
                update_firing_detection_range: Some(turret_template.max_range),
            });
        }

        turret_state.reload_timer.reset();
    }
}

fn advance_smoke_cooldown(
    smokers: Query<&mut SmokeConsumableState, Without<SmokeDeploying>>,
    time: Res<Time>,
) {
    for mut smoker in smokers {
        smoker.cooldown_timer.tick(time.delta());
    }
}

fn deploy_smoke(
    mut commands: Commands,
    smokers: Query<(Entity, &Ship, &mut SmokeDeploying, &Transform)>,
    time: Res<Time>,
) {
    for (smoker_entity, ship, mut smoker, smoker_trans) in smokers {
        smoker.action_timer.tick(time.delta());
        smoker.puff_timer.tick(time.delta());
        if smoker.puff_timer.finished() || smoker.action_timer.finished() {
            let smoke = ship.template.consumables.smoke().unwrap();
            commands.queue(SpawnSmokePuffCommand {
                pos: smoker_trans.translation.truncate(),
                radius: smoke.radius,
                dissapation: smoke.dissapation,
            });
        }

        if smoker.action_timer.finished() {
            commands.entity(smoker_entity).remove::<SmokeDeploying>();
        }
    }
}

fn dissapate_smoke_puffs(
    mut commands: Commands,
    puffs: Query<(Entity, &mut SmokePuff)>,
    time: Res<Time>,
) {
    for (puff_entity, mut puff) in puffs {
        puff.dissapation.tick(time.delta());
        if puff.dissapation.finished() {
            commands.queue(DespawnNetworkedEntityCommand {
                entity: puff_entity,
            });
        }
    }
}

fn apply_velocity(q: Query<(&mut Transform, &Velocity)>, time: Res<Time>) {
    for (mut trans, vel) in q {
        trans.translation += vel.0 * time.delta_secs();
    }
}

pub fn start_match() -> Result<()> {
    let exit = App::new()
        .init_resource::<GameRules>()
        .add_plugins(
            DefaultPlugins
                .set(ImagePlugin::default_nearest())
                .set(WindowPlugin {
                    primary_window: None,
                    exit_condition: ExitCondition::DontExit,
                    ..default()
                }),
        )
        .add_plugins(NetworkingPlugin)
        .add_plugins(DetectionPlugin)
        .add_systems(Startup, initalize_game)
        .configure_sets(
            Update,
            DetectionSystems
                .after(apply_velocity)
                .after(collide_bullets),
        )
        .add_systems(
            Update,
            (
                update_ship_velocity,
                apply_velocity.after(update_ship_velocity),
                force_ship_in_map.after(apply_velocity),
                move_bullets,
                torpedo_reloading,
                despawn_old_torpedoes.after(apply_velocity),
                collide_torpedoes.after(apply_velocity),
                collide_bullets.after(move_bullets),
                turret_reloading,
                update_turret_absolute_pos,
                aim_turrets.after(update_turret_absolute_pos),
                fire_bullets
                    .after(turret_reloading)
                    .after(aim_turrets)
                    .after(DetectionSystems),
                advance_smoke_cooldown,
                deploy_smoke,
                dissapate_smoke_puffs,
            ),
        )
        .run();

    info!("Bevy exited: `{exit:?}`");

    Ok(())
}
