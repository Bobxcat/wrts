use bevy::prelude::*;
use wrts_messaging::{Client2Match, Match2Client, Message};

use crate::{AppState, networking::ServerConnection};

pub struct InMatchPlugin;

impl Plugin for InMatchPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (in_match_networking.pipe(in_match_networking_none_handler))
                .run_if(in_state(AppState::InMatch)),
        )
        .add_systems(Update, (send_echo_msg).run_if(in_state(AppState::InMatch)));
    }
}

fn send_echo_msg(
    mut server: ResMut<ServerConnection>,
    mut timer: Local<Option<Timer>>,
    time: Res<Time>,
) {
    let timer = timer.get_or_insert(Timer::from_seconds(2., TimerMode::Repeating));
    timer.tick(time.delta());
    if timer.just_finished() {
        server.send(Message::Client2Match(Client2Match::Echo(
            "Message echoed!!".into(),
        )));
    }
}

fn in_match_networking(mut commands: Commands, mut server: ResMut<ServerConnection>) -> Option<()> {
    for msg in server.recv_all()? {
        match msg {
            Message::Match2Client(Match2Client::PrintMsg(s)) => {
                info!("PrintMsg called: {s}");
            }
            Message::Lobby2Client(_) | Message::Client2Lobby(_) | Message::Client2Match(_) => {
                error!("Unexpected message: {msg:?}");
            }
        }
    }
    Some(())
}

fn in_match_networking_none_handler(
    In(input): In<Option<()>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if let None = input {
        info!("`in_match_networking` disconnected");
        next_state.set(AppState::ConnectingToServer);
    }
}
