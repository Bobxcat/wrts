use std::{
    collections::HashMap,
    ops::{Index, IndexMut},
    time::Duration,
};

use bevy::{prelude::*, window::ExitCondition};
use enum_map::EnumMap;
use itertools::Itertools;
use wrts_messaging::{ClientId, Match2Client, Message, WrtsMatchMessage};

use crate::{
    detection::{DetectionPlugin, DetectionStatus, DetectionSystems},
    initialize_game::initalize_game,
    math_utils::BulletProblemRes,
    networking::{ClientInfo, MessagesSend, NetworkingPlugin, SharedEntityTracking},
    ship::{Ship, apply_dispersion},
    spawn_entity::{DespawnNetworkedEntityCommand, SpawnBulletCommand},
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

fn update_ship_velocity(
    ships: Query<(
        &Ship,
        &mut Transform,
        &mut Velocity,
        Option<&mut MoveOrder>,
        &Team,
        Entity,
    )>,
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
        let new_vel = ship
            .3
            .and_then(|order| order.waypoints.get(0).copied())
            .and_then(|next_waypoint| Dir2::new(next_waypoint - ship.1.translation.truncate()).ok())
            .map(|dir| {
                ship.1.rotation = Quat::from_rotation_z(dir.to_angle());
                dir * ship.0.template.max_speed.mps()
            })
            .unwrap_or(Vec2::ZERO);
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
    inital_pos: Vec3,
    inital_vel: Vec3,
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
        bullet.current_flight_time += time.delta();

        if let Ok(dir) = Dir2::new((new_pos - trans.translation).truncate()) {
            trans.rotation = Quat::from_rotation_z(dir.to_angle());
        }
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

            // Calculate collisions in the local space of the ship hull
            let hull = &ship.template.hull;
            let ship_hull_min = vec3(-0.5 * hull.length, -0.5 * hull.width, -hull.draft);
            let ship_hull_max = vec3(0.5 * hull.length, 0.5 * hull.width, hull.freeboard);
            let bullet_pos =
                ship_trans.rotation * (bullet_trans.translation - ship_trans.translation);
            // FIXME?: we're assuming the bullet impacts when the bullet hits the water
            // Maybe this is fine, because it'll always be approx. correct
            let bullet_vel = ship_trans.rotation * bullet.inital_vel.with_z(-bullet.inital_vel.z);
            if Vec3::cmple(ship_hull_min, bullet_pos).all()
                && Vec3::cmple(bullet_pos, ship_hull_max).all()
            {
                let bullet_alignment = bullet_vel.normalize().dot(Vec3::X).abs();
                let damage = bullet.damage * (1.5 - bullet_alignment as f64);
                info!(
                    "Dealing damage:  ship=`{}`, dmg={damage:.2}, health_before={:.2}, align={bullet_alignment:.2}",
                    ship.template.id.to_name(),
                    ship_health.0,
                );

                ship_health.0 -= damage;

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

fn fire_bullets(
    mut commands: Commands,
    ships: Query<(
        Entity,
        &Team,
        &mut Ship,
        &Transform,
        &Velocity,
        Option<&FireTarget>,
        &DetectionStatus,
    )>,
    time: Res<Time>,
    rules: Res<GameRules>,
    teams: Query<&ClientInfo>,
) {
    let teams: [Team; 2] = teams
        .iter()
        .map(|cl| Team(cl.info.id))
        .collect_array()
        .unwrap();
    let mut ships_by_team: TeamMap<_> = {
        let (team0, team1) = ships
            .into_iter()
            .partition::<Vec<_>, _>(|(_, team, ..)| **team == teams[0]);
        TeamMap::from_iter([(teams[0], team0), (teams[1], team1)])
    };

    for (team, ship_idx, turret_idx) in teams
        .into_iter()
        .flat_map(|team| (0..ships_by_team[team].len()).map(move |idx| (team, idx)))
        .flat_map(|(team, ship_idx)| {
            (0..ships_by_team[team][ship_idx].2.template.turrets.len())
                .map(move |turret_idx| (team, ship_idx, turret_idx))
        })
        .collect_vec()
    {
        let team_opposite = if teams[0] == team { teams[1] } else { teams[0] };

        let (targ, targ_trans, targ_vel) = {
            let targ = ships_by_team[team][ship_idx]
                .5
                .and_then(|targ| {
                    ships_by_team[team_opposite]
                        .iter()
                        .find(|(ship, _, _, _, _, _, _)| *ship == targ.ship)
                })
                .filter(|(_, _, _, _, _, _, targ_detection)| targ_detection.is_detected);

            let Some((targ, _, _, targ_trans, targ_vel, _, _)) = targ else {
                let turret_timer =
                    &mut ships_by_team[team][ship_idx].2.turret_reload_timers[turret_idx];
                if !turret_timer.finished() {
                    turret_timer.tick(time.delta());
                }
                continue;
            };
            (*targ, **targ_trans, **targ_vel)
        };

        let (ship_entity, ship, ship_trans) = {
            let x = &mut ships_by_team[team][ship_idx];
            (x.0, &mut x.2, *x.3)
        };
        let turret = &ship.template.turrets[turret_idx];

        let turret_pos = turret.location_on_ship.to_absolute(
            &ship.template.hull,
            ship_trans.translation.truncate(),
            ship_trans.rotation,
        );
        let targ_pos = targ_trans.translation.truncate();

        let turret_reload_timer = &mut ship.turret_reload_timers[turret_idx];

        let Some(BulletProblemRes {
            intersection_point,
            intersection_time,
            intersection_dist: _,
            projectile_dir: bullet_dir,
            projectile_azimuth: bullet_azimuth,
            projectile_elevation: _,
        }) = math_utils::bullet_problem(
            turret_pos,
            targ_pos,
            targ_vel.0.truncate(),
            turret.muzzle_vel as f64,
            rules.gravity as f64,
        )
        .filter(|bp| bp.intersection_dist < turret.max_range)
        else {
            if !turret_reload_timer.finished() {
                turret_reload_timer.tick(time.delta());
            }
            continue;
        };

        for _ in 0..turret_reload_timer.times_finished_this_tick() {
            for barrel_idx in 0..turret.barrel_count {
                let barrel_lateral_offset =
                    (barrel_idx - (turret.barrel_count - 1) / 2) as f32 * turret.barrel_spacing;

                let bullet_vel =
                    apply_dispersion(&turret.dispersion, bullet_dir) * turret.muzzle_vel as f32;

                let bullet_start = turret_pos
                    + Vec2::from_angle(bullet_azimuth).rotate(vec2(0., barrel_lateral_offset));
                // The bullet should start very slightly above the water,
                // but not by very much since ships have a small draft so
                // it would mean a lot more missing
                let bullet_start = bullet_start.extend(0.1);

                let bullet = Bullet {
                    owning_ship: ship_entity,
                    targ_ship: targ,
                    inital_pos: bullet_start,
                    inital_vel: bullet_vel,
                    inital_aimpoint: intersection_point,
                    current_aimpoint: intersection_point,
                    expected_flight_time_total: Duration::from_secs_f32(intersection_time),
                    current_flight_time: Duration::ZERO,
                    damage: turret.damage,
                };

                commands.queue(SpawnBulletCommand {
                    team,
                    bullet,
                    update_firing_detection_timer: Some(Duration::from_secs(20)),
                });
            }
        }

        // We want the turret to remain reloaded or continue progressing its
        // reload when unable to fire, including when there is no target
        // If we consider the previous checks that the target is shootable,
        // placing the tick here accounts for the above
        turret_reload_timer.tick(time.delta());
    }
}

fn apply_velocity(q: Query<(&mut Transform, &Velocity)>, time: Res<Time>) {
    for (mut trans, vel) in q {
        trans.translation += vel.0 * time.delta_secs();
    }
}

fn main() -> Result<()> {
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
                collide_bullets.after(move_bullets),
                fire_bullets.after(DetectionSystems),
            ),
        )
        .run();

    info!("Bevy exited: `{exit:?}`");

    Ok(())
}
