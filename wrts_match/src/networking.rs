use std::sync::mpsc::TryRecvError;

use bevy::prelude::*;
use wrts_messaging::{Client2Match, Match2Client, Message, WrtsMatchMessage};

use crate::{MessagesRecv, MessagesSend};

pub struct NetworkingPlugin;

impl Plugin for NetworkingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, read_messages);
    }
}

pub fn read_messages(
    rx: NonSend<MessagesRecv>,
    tx: ResMut<MessagesSend>,
    mut exit: EventWriter<AppExit>,
) {
    loop {
        let WrtsMatchMessage { client, msg } = match rx.try_recv() {
            Ok(msg) => msg,
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => {
                error!("Messaging disconnected, exiting");
                exit.write(AppExit::Error(1.try_into().unwrap()));
                return;
            }
        };
        match msg {
            Message::Client2Match(Client2Match::Echo(s)) => {
                if let Err(_) = tx.send(WrtsMatchMessage {
                    client,
                    msg: Message::Match2Client(Match2Client::PrintMsg(s)),
                }) {
                    error!("Messaging disconnected, exiting");
                    exit.write(AppExit::Error(1.try_into().unwrap()));
                }
            }
            Message::Match2Client(_) | Message::Client2Lobby(_) | Message::Lobby2Client(_) => {
                error!("Received unexpected message: {msg:?}");
            }
        };
    }
}
