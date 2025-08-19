use bevy::prelude::*;
use wrts_messaging::{Client2Match, ClientSharedInfo, Match2Client, Message};

use crate::{
    AppState, PlayerSettings,
    networking::{ClientInfo, ServerConnection, ThisClient},
};

pub struct InMatchPlugin;

impl Plugin for InMatchPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            OnEnter(AppState::InMatch),
            (in_match_startup_networking.pipe(in_match_startup_networking_none_handler)),
        )
        .add_systems(
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
    let timer = timer.get_or_insert(Timer::from_seconds(4., TimerMode::Repeating));
    timer.tick(time.delta());
    if timer.just_finished() {
        let _ = server.send(Message::Client2Match(Client2Match::Echo(format!(
            "Echo from {:?}",
            time.elapsed()
        ))));
    }
}

fn in_match_startup_networking(
    mut commands: Commands,
    mut server: ResMut<ServerConnection>,
    settings: Res<PlayerSettings>,
) -> Option<()> {
    let Message::Match2Client(Match2Client::InitA { your_client }) = server.recv_blocking()? else {
        return None;
    };

    commands.insert_resource(ThisClient(your_client));

    server.send(Message::Client2Match(Client2Match::InitB {
        info: ClientSharedInfo {
            id: your_client,
            user: settings.username.clone(),
        },
    }))?;

    let Message::Match2Client(Match2Client::InitC { all_clients }) = server.recv_blocking()? else {
        return None;
    };

    assert!(
        all_clients.len() == 2,
        "Currently, there should always be two clients per game"
    );
    for info in all_clients {
        commands.spawn((
            StateScoped(AppState::InMatch),
            ClientInfo {
                id: info.id,
                user: info.user,
            },
        ));
    }

    Some(())
}

fn in_match_startup_networking_none_handler(
    In(input): In<Option<()>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if let None = input {
        info!("`in_match_startup_networking` disconnected");
        next_state.set(AppState::ConnectingToServer);
    }
}

fn in_match_networking(mut commands: Commands, mut server: ResMut<ServerConnection>) -> Option<()> {
    for msg in server.recv_all()? {
        match msg {
            Message::Match2Client(Match2Client::PrintMsg(s)) => {
                info!("PrintMsg called: {s}");
            }
            Message::Match2Client(Match2Client::InitA { .. })
            | Message::Match2Client(Match2Client::InitC { .. })
            | Message::Lobby2Client(_)
            | Message::Client2Lobby(_)
            | Message::Client2Match(_) => {
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
