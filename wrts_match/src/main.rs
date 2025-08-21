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
        &Transform,
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
        let new_vel = match ship.3 {
            Some(order) if order.waypoints.len() > 0 => {
                let dir = (order.waypoints[0] - ship.1.translation.truncate()).normalize();
                dir * ship.0.template.max_speed
            }
            _ => Vec2::ZERO,
        };
        ship.2.0 = new_vel.extend(0.);
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
            commands.queue(DespawnNetworkedEntityCommand { entity });
        }
    }
}

fn collide_bullets(
    mut commands: Commands,
    bullets: Query<(Entity, &Bullet, &Transform, &Team)>,
    mut ships: Query<(Entity, &Ship, &Transform, &Team, &mut Health)>,
    shared_entities: Res<SharedEntityTracking>,
    clients: Query<&ClientInfo>,
    msgs_tx: Res<MessagesSend>,
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

        let (targ_trans, targ_vel) = {
            let targ = ships_by_team[team][ship_idx]
                .5
                .and_then(|targ| {
                    ships_by_team[team_opposite]
                        .iter()
                        .find(|(ship, _, _, _, _, _, _)| *ship == targ.ship)
                })
                .filter(|(_, _, _, _, _, _, targ_detection)| targ_detection.is_detected);

            let Some((_, _, _, targ_trans, targ_vel, _, _)) = targ else {
                let turret_timer =
                    &mut ships_by_team[team][ship_idx].2.turret_reload_timers[turret_idx];
                if !turret_timer.finished() {
                    turret_timer.tick(time.delta());
                }
                continue;
            };
            (targ_trans, targ_vel)
        };
        let targ_trans = **targ_trans;
        let targ_vel = **targ_vel;

        let (ship_entity, _, ship, ship_trans, _, _ship_targ, _) =
            &mut ships_by_team[team][ship_idx];
        let turret_template = &ship.template.turrets[turret_idx];
        let turret_reload_timer = &mut ship.turret_reload_timers[turret_idx];

        let origin_pos = ship_trans.translation.truncate()
            + Vec2::from_angle(ship_trans.rotation.to_euler(EulerRot::ZXY).0)
                .rotate(turret_template.location_on_ship);
        let targ_pos = targ_trans.translation.truncate();

        let Some(BulletProblemRes {
            intersection_point: _,
            intersection_time: _,
            intersection_dist: _,
            projectile_dir: bullet_dir,
            projectile_azimuth: bullet_azimuth,
            projectile_elevation: _,
        }) = math_utils::bullet_problem(
            origin_pos,
            targ_pos,
            targ_vel.0.truncate(),
            turret_template.muzzle_vel as f64,
            rules.gravity as f64,
        )
        .filter(|bp| bp.intersection_dist < turret_template.max_range)
        else {
            if !turret_reload_timer.finished() {
                turret_reload_timer.tick(time.delta());
            }
            continue;
        };

        for _ in 0..turret_reload_timer.times_finished_this_tick() {
            for barrel in &turret_template.barrels {
                let bullet_vel = apply_dispersion(&turret_template.dispersion, bullet_dir)
                    * turret_template.muzzle_vel as f32;

                let bullet_start = origin_pos + Vec2::from_angle(bullet_azimuth).rotate(*barrel);
                let bullet_trans = Transform {
                    translation: bullet_start.extend(5.),
                    rotation: Quat::from_rotation_z(
                        std::f32::consts::FRAC_PI_2 + bullet_vel.truncate().to_angle(),
                    ),
                    ..default()
                };

                commands.queue(SpawnBulletCommand {
                    team,
                    owning_ship: *ship_entity,
                    update_firing_detection_timer: Some(Duration::from_secs(20)),
                    damage: turret_template.damage,
                    pos: bullet_trans.translation,
                    rot: bullet_trans.rotation,
                    vel: bullet_vel,
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
                update_ship_velocity.before(apply_velocity),
                move_bullets,
                apply_velocity,
                collide_bullets.after(move_bullets).after(apply_velocity),
                fire_bullets.after(DetectionSystems),
            ),
        )
        .run();

    info!("Bevy exited: `{exit:?}`");

    Ok(())
}
