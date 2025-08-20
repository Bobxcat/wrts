use bevy::prelude::*;
use itertools::Itertools;
use wrts_match_shared::ship_template::ShipTemplateId;
use wrts_messaging::{Match2Client, Message, WrtsMatchMessage};

use crate::{
    Health, Team,
    networking::{ClientInfo, MessagesSend, SharedEntityTracking},
    ship::Ship,
};

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
                        turret_reload_timers: template
                            .turrets
                            .iter()
                            .map(|t| Timer::from_seconds(t.reload_secs, TimerMode::Repeating))
                            .collect_vec(),
                    },
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
            msgs_tx.send(WrtsMatchMessage {
                client: cl.info.id,
                msg: Message::Match2Client(Match2Client::SpawnShip {
                    id: shared_id,
                    team: self.team.0,
                    ship_base: self.ship_base,
                    health: self.health.0,
                    pos: self.pos,
                    rot: self.rot,
                }),
            });
        }
    }
}

pub struct SpawnBulletCommand {
    pub team: Team,
    pub damage: f64,
    pub pos: Vec2,
    pub rot: Quat,
}
