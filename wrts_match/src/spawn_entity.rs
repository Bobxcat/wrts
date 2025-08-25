use std::time::Duration;

use bevy::prelude::*;
use itertools::Itertools;
use wrts_match_shared::ship_template::ShipTemplateId;
use wrts_messaging::{Match2Client, Message, WrtsMatchMessage};

use crate::{
    Bullet, Health, Team, Velocity,
    detection::{BaseDetection, CanDetect, DetectionStatus},
    networking::{ClientInfo, MessagesSend, SharedEntityTracking},
    ship::{Ship, TurretState},
};

pub struct DespawnNetworkedEntityCommand {
    pub entity: Entity,
}

impl Command for DespawnNetworkedEntityCommand {
    fn apply(self, world: &mut World) -> () {
        let _ = world.try_despawn(self.entity);

        let Some((shared, _)) = world
            .resource_mut::<SharedEntityTracking>()
            .remove_by_local(self.entity)
        else {
            return;
        };

        let mut clients = world.query::<&ClientInfo>();
        let msgs_tx = world.resource::<MessagesSend>();
        for cl in clients.iter(world) {
            msgs_tx.send(WrtsMatchMessage {
                client: cl.info.id,
                msg: Message::Match2Client(Match2Client::DestroyEntity(shared)),
            });
        }
    }
}

pub struct SpawnShipCommand {
    pub team: Team,
    pub ship_base: ShipTemplateId,
    pub health: Health,
    pub pos: Vec2,
    pub rot: Quat,
}

impl Command for SpawnShipCommand {
    fn apply(self, world: &mut World) -> () {
        let entity = {
            let template = self.ship_base.to_template();
            world
                .spawn((
                    Ship {
                        template,
                        turret_states: template
                            .turret_instances
                            .iter()
                            .map(|t| TurretState {
                                dir: t.default_dir,
                                reload_timer: Timer::from_seconds(
                                    t.turret_template().reload_secs,
                                    TimerMode::Repeating,
                                ),
                            })
                            .collect_vec(),
                        curr_speed: 0.,
                    },
                    BaseDetection(template.detection),
                    DetectionStatus {
                        is_detected: false,
                        detection_increased_by_firing: Timer::new(Duration::ZERO, TimerMode::Once)
                            .tick(Duration::MAX)
                            .clone(),
                    },
                    CanDetect,
                    self.health.clone(),
                    self.team,
                    Transform {
                        translation: self.pos.extend(0.),
                        rotation: self.rot,
                        ..default()
                    },
                ))
                .id()
        };
        let shared_id = world.resource_mut::<SharedEntityTracking>().insert(entity);

        let mut clients = world.query::<&ClientInfo>();
        let msgs_tx = world.get_resource::<MessagesSend>().unwrap();
        for cl in clients.iter(world) {
            let turret_rots = self
                .ship_base
                .to_template()
                .turret_instances
                .iter()
                .map(|instance| instance.default_dir)
                .collect_vec();
            msgs_tx.send(WrtsMatchMessage {
                client: cl.info.id,
                msg: Message::Match2Client(Match2Client::SpawnShip {
                    id: shared_id,
                    team: self.team.0,
                    ship_base: self.ship_base,
                    health: self.health.0,
                    pos: self.pos,
                    rot: self.rot,
                    turret_rots,
                }),
            });
        }
    }
}

pub struct SpawnBulletCommand {
    pub team: Team,
    pub bullet: Bullet,
    pub update_firing_detection_timer: Option<Duration>,
}

impl Command for SpawnBulletCommand {
    fn apply(self, world: &mut World) -> () {
        let rot = Quat::from_rotation_z(self.bullet.inital_vel.truncate().to_angle());

        let entity = {
            world
                .spawn((
                    self.bullet.clone(),
                    self.team,
                    Transform {
                        translation: self.bullet.inital_pos,
                        rotation: rot,
                        ..default()
                    },
                ))
                .id()
        };

        if let Some(t) = self.update_firing_detection_timer {
            let mut det = world
                .get_mut::<DetectionStatus>(self.bullet.owning_ship)
                .unwrap();
            det.detection_increased_by_firing = Timer::new(
                t.max(det.detection_increased_by_firing.remaining()),
                TimerMode::Once,
            );
        }

        let shared_id = world.resource_mut::<SharedEntityTracking>().insert(entity);

        let mut clients = world.query::<&ClientInfo>();
        let msgs_tx = world.get_resource::<MessagesSend>().unwrap();

        let owning_ship = world
            .resource::<SharedEntityTracking>()
            .get_by_local(self.bullet.owning_ship)
            .unwrap();
        for cl in clients.iter(world) {
            msgs_tx.send(WrtsMatchMessage {
                client: cl.info.id,
                msg: Message::Match2Client(Match2Client::SpawnBullet {
                    id: shared_id,
                    team: self.team.0,
                    owning_ship,
                    damage: self.bullet.damage,
                    pos: self.bullet.inital_pos,
                    rot,
                }),
            });
        }
    }
}
