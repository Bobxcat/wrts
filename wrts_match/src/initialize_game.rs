use bevy::prelude::*;
use wrts_messaging::{Match2Client, Message, WrtsMatchMessage};

use crate::{Health, Team, networking::MessagesSend, ship::Ship};

pub fn initalize_game(mut commands: Commands) {
    //
}

fn make_ships(mut commands: Commands, msgs_tx: Res<MessagesSend>) {
    commands.spawn((
        Ship::bismarck(),
        Health(10_000.),
        Team::Friend,
        Transform {
            translation: vec2(-300., 60.).extend(0.),
            ..Default::default()
        },
    ));

    commands.spawn((
        Ship::oland(),
        Health(1000.),
        Team::Enemy,
        Transform {
            translation: vec2(8000., 120.).extend(0.),
            ..Default::default()
        },
    ));
}

pub struct SpawnShipCommand {
    pub ship: Ship,
    pub health: Health,
    pub team: Team,
    pub transform: Transform,
}

impl EntityCommand for SpawnShipCommand {
    fn apply(self, mut entity: EntityWorldMut) -> () {
        let msgs_tx = entity.world().get_resource::<MessagesSend>().unwrap();
        // let _ = msgs_tx.send(WrtsMatchMessage {
        //     client: self.team.0,
        //     msg: Message::Match2Client(Match2Client::),
        // });
        entity.insert((self.ship, self.health, self.team, self.transform));
    }
}
