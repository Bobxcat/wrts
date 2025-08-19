use std::{
    io::{Write, stdin},
    ops::{Deref, DerefMut},
    sync::mpsc::{self, Receiver, SyncSender},
};

use bevy::{
    log::LogPlugin, prelude::*, render::RenderPlugin, window::ExitCondition, winit::WinitPlugin,
};
use wrts_messaging::{
    ClientId, RecvFromStream, WrtsMatchInitMessage, WrtsMatchMessage, read_from_stream_sync,
    write_to_stream_sync,
};

use crate::networking::NetworkingPlugin;

mod networking;

fn stdin_handler(tx: SyncSender<WrtsMatchMessage>) {
    let mut stdin = std::io::stdin().lock();
    loop {
        match WrtsMatchMessage::recv_sync(&mut stdin) {
            Ok(msg) => {
                info!("Receiving: {msg:?}");
                if let Err(_) = tx.send(msg) {
                    error!("lost connection to bevy, exiting");
                    return;
                }
            }
            Err(e) => {
                error!("Error receiving WrtsMatchMessage: `{e}`");
                error!("Exiting stdin_handler");
                return;
            }
        }
    }
}

fn stdout_handler(rx: Receiver<WrtsMatchMessage>) {
    let mut stdout = std::io::stdout().lock();
    loop {
        match rx.recv() {
            Ok(msg) => {
                info!("Sending: {msg:?}");
                if let Err(e) = write_to_stream_sync(&msg, &mut stdout) {
                    error!("Encountered error sending to stdout: `{:?}`", e)
                }
                let _ = stdout.flush();
            }
            Err(_) => {
                error!("lost connection to bevy, exiting");
                return;
            }
        }
    }
}

#[derive(Debug, Resource)]
pub struct MessagesSend(SyncSender<WrtsMatchMessage>);

impl Deref for MessagesSend {
    type Target = SyncSender<WrtsMatchMessage>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Non-send resource
#[derive(Debug)]
pub struct MessagesRecv(Receiver<WrtsMatchMessage>);

impl Deref for MessagesRecv {
    type Target = Receiver<WrtsMatchMessage>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Component, Debug, Clone)]
pub struct Client {
    pub id: ClientId,
}

#[derive(Resource, Debug)]
struct InitMsgRes {
    init_msg: Option<WrtsMatchInitMessage>,
}

fn handle_init_msg(mut commands: Commands, mut res: ResMut<InitMsgRes>) {
    let init_msg = res.init_msg.take().unwrap();
    for cl in init_msg.clients {
        commands.spawn(Client { id: cl });
    }
    commands.remove_resource::<InitMsgRes>();
}

fn main() -> Result<()> {
    let init_msg = WrtsMatchInitMessage::recv_sync(&mut stdin()).unwrap();

    let (handler_tx, rx) = mpsc::sync_channel::<WrtsMatchMessage>(128);
    let (tx, handler_rx) = mpsc::sync_channel::<WrtsMatchMessage>(128);

    std::thread::spawn(move || {
        stdin_handler(handler_tx);
    });
    std::thread::spawn(move || {
        stdout_handler(handler_rx);
    });

    let exit = App::new()
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
        .insert_non_send_resource(MessagesRecv(rx))
        .insert_resource(MessagesSend(tx))
        .insert_resource(InitMsgRes {
            init_msg: Some(init_msg),
        })
        .add_systems(Startup, handle_init_msg)
        .run();

    info!("Bevy exited: `{exit:?}`");

    Ok(())
}
