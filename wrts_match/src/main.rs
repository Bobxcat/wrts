use std::{
    collections::HashMap,
    io::{Write, stdin},
    ops::{Deref, DerefMut},
    sync::mpsc::{self, Receiver, SyncSender},
};

use bevy::{
    log::LogPlugin, prelude::*, render::RenderPlugin, window::ExitCondition, winit::WinitPlugin,
};
use wrts_messaging::{
    Client2Match, ClientId, ClientSharedInfo, Match2Client, Message, RecvFromStream,
    WrtsMatchInitMessage, WrtsMatchMessage, read_from_stream_sync, write_to_stream_sync,
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
    pub info: ClientSharedInfo,
}

#[derive(Resource, Debug)]
struct InitMsgRes {
    init_msg: Option<WrtsMatchInitMessage>,
}

fn handle_init_msg(
    mut commands: Commands,
    mut res: ResMut<InitMsgRes>,
    msgs_tx: Res<MessagesSend>,
    msgs_rx: NonSendMut<MessagesRecv>,
    mut app_exit: EventWriter<AppExit>,
) {
    let init_msg = res.init_msg.take().unwrap();
    commands.remove_resource::<InitMsgRes>();

    let client_infos = {
        let mut infos = HashMap::new();
        for cl in init_msg.clients {
            let _ = msgs_tx.send(WrtsMatchMessage {
                client: cl,
                msg: Message::Match2Client(Match2Client::InitA { your_client: cl }),
            });
        }

        for _ in 0..init_msg.clients.len() {
            match msgs_rx.recv() {
                Ok(WrtsMatchMessage {
                    client: _,
                    msg: Message::Client2Match(Client2Match::InitB { info }),
                }) => {
                    infos.insert(info.id, info);
                }
                res => {
                    error!(
                        "Expected one `InitA` message per client assigned to this match! Instead, got: {res:?}"
                    );
                    app_exit.write(AppExit::from_code(1));
                    return;
                }
            };
        }
        infos
    };

    for (_, cl_info) in client_infos.clone() {
        let _ = msgs_tx.send(WrtsMatchMessage {
            client: cl_info.id,
            msg: Message::Match2Client(Match2Client::InitC {
                all_clients: client_infos.values().cloned().collect(),
            }),
        });
        commands.spawn(Client { info: cl_info });
    }
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
