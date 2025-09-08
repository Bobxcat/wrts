//! Custom commands that spawn entities and
//! update clients accordingly

use std::time::Duration;

use bevy::prelude::*;
use itertools::Itertools;
use wrts_match_shared::ship_template::ShipTemplateId;
use wrts_messaging::{Match2Client, Message, WrtsMatchMessage};

use crate::{
    Bullet, Health, Team,
    detection::{BaseDetection, CanDetect, DetectionStatus},
    networking::{ClientInfo, MessagesSend, SharedEntityTracking},
    ship::{Ship, SmokeConsumableState, SmokePuff, TurretAimInfo, TurretState, TurretStates},
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
        let template = self.ship_base.to_template();
        let entity = {
            world
                .spawn((
                    Ship {
                        template,
                        curr_speed: 0.,
                        torpedo_reloads: template
                            .torpedoes
                            .iter()
                            .flat_map(|torps| {
                                (0..torps.volleys)
                                    .map(|_idx| Timer::new(torps.reload, TimerMode::Once))
                            })
                            .collect(),
                    },
                    TurretStates {
                        states: template
                            .turret_instances
                            .iter()
                            .map(|t| TurretState {
                                dir: t.default_dir,
                                reload_timer: Timer::from_seconds(
                                    t.turret_template().reload_secs,
                                    TimerMode::Once,
                                ),
                                absolute_pos: Vec2::ZERO,
                                aim_info: TurretAimInfo::NoValidTarget {},
                            })
                            .collect_vec(),
                    },
                    BaseDetection(template.detection),
                    DetectionStatus {
                        is_detected: false,
                        detection_increased_by_firing: Timer::new(Duration::ZERO, TimerMode::Once)
                            .tick(Duration::MAX)
                            .clone(),
                        detection_increased_by_firing_at_range: 0.,
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

        // Consumables
        if let Some(smoke) = template.consumables.smoke() {
            world.entity_mut(entity).insert(SmokeConsumableState {
                cooldown_timer: Timer::new(smoke.cooldown, TimerMode::Once),
                charges_unused: (smoke.charges > 0).then_some(smoke.charges),
            });
        }
        // ...

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
    pub update_firing_detection_range: Option<f32>,
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
            if let Some(range) = self.update_firing_detection_range {
                det.detection_increased_by_firing_at_range =
                    det.detection_increased_by_firing_at_range.max(range);
            }
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

pub struct SpawnSmokePuffCommand {
    pub pos: Vec2,
    pub radius: f32,
    pub dissapation: Duration,
}

impl Command for SpawnSmokePuffCommand {
    fn apply(self, world: &mut World) -> () {
        let entity = {
            world
                .spawn((
                    SmokePuff {
                        radius: self.radius,
                        dissapation: Timer::new(self.dissapation, TimerMode::Once),
                    },
                    Transform {
                        translation: self.pos.extend(0.),
                        ..default()
                    },
                ))
                .id()
        };

        let shared_id = world.resource_mut::<SharedEntityTracking>().insert(entity);

        let mut clients = world.query::<&ClientInfo>();
        let msgs_tx = world.get_resource::<MessagesSend>().unwrap();

        for cl in clients.iter(world) {
            msgs_tx.send(WrtsMatchMessage {
                client: cl.info.id,
                msg: Message::Match2Client(Match2Client::SpawnSmokePuff {
                    id: shared_id,
                    pos: self.pos,
                    radius: self.radius,
                }),
            });
        }
    }
}
