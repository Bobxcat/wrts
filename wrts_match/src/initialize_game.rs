use bevy::prelude::*;
use itertools::Itertools;
use wrts_match_shared::ship_template::{ShipTemplate, ShipTemplateId};

use crate::{Health, Team, networking::ClientInfo, spawn_entity::SpawnShipCommand};

pub fn initalize_game(mut commands: Commands, teams: Query<&ClientInfo>) {
    let teams: [&ClientInfo; 2] = teams
        .into_iter()
        .collect_array()
        .expect("There aren't two clients!!!");
    for team_idx in 0..2 {
        let pos_base = match team_idx {
            0 => vec2(8000., 0.),
            _ => vec2(-8000., 0.),
        };
        let rot = Quat::from_rotation_z(match team_idx {
            0 => std::f32::consts::PI,
            _ => 0.,
        });
        let ships = [
            ShipTemplateId::oland(),
            ShipTemplateId::bismarck(),
            ShipTemplateId::kiev(),
            ShipTemplateId::nagato(),
        ];
        for ship_idx in 0..ships.len() {
            let offset_side = if ship_idx % 2 == 0 { -1. } else { 1. };
            let offset_ct = (ship_idx + 1).div_euclid(2) as f32;
            let pos = pos_base + vec2(0., 400.) * offset_ct * offset_side;
            commands.queue(SpawnShipCommand {
                team: Team(teams[team_idx].info.id),
                ship_base: ships[ship_idx],
                health: Health(ShipTemplate::from_id(ships[ship_idx]).max_health),
                pos,
                rot,
            });
        }
    }

    commands.queue(crate::spawn_entity::SpawnSmokePuffCommand {
        pos: vec2(0., 0.),
        radius: 2_000.,
        dissapation: std::time::Duration::from_secs(20),
    });
}
