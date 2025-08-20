use bevy::prelude::*;
use wrts_match_shared::ship_template::{ShipTemplate, ShipTemplateId};
use wrts_messaging::{Match2Client, Message, WrtsMatchMessage};

use crate::{
    Health, Team,
    networking::{ClientInfo, MessagesSend, SharedEntityTracking},
    ship::Ship,
    spawn_entity::SpawnShipCommand,
};

pub fn initalize_game(mut commands: Commands, teams: Query<&ClientInfo>) {
    let mut p = vec2(0., 0.);
    for team in teams {
        commands.queue(SpawnShipCommand {
            team: Team(team.info.id),
            ship_base: ShipTemplateId::oland(),
            health: Health(ShipTemplate::from_id(ShipTemplateId::oland()).max_health),
            pos: p,
            rot: Quat::from_rotation_z(0.),
        });
        p += vec2(100., 100.);
    }
}
