use bevy::prelude::*;
use std::io::stdin;
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::{collections::HashMap, io::Write, ops::Deref};
use wrts_messaging::{Client2Match, Match2Client, Message, WrtsMatchMessage};

use wrts_messaging::{
    ClientId, ClientSharedInfo, RecvFromStream, WrtsMatchInitMessage, read_from_stream_sync,
    write_to_stream_sync,
};

pub struct NetworkingPlugin;

impl Plugin for NetworkingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, network_handshake)
            .add_systems(FixedUpdate, read_messages);
    }
}

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

mod shared_entity_tracking {
    use std::collections::HashMap;

    use bevy::ecs::{entity::Entity, resource::Resource};
    use slotmap::{KeyData, SlotMap};
    use wrts_messaging::SharedEntityId;

    slotmap::new_key_type! {
        struct InnerId;
    }

    /// Keeps track of the mapping between
    #[derive(Resource, Debug, Default, Clone)]
    pub struct SharedEntityTracking {
        id2entity: SlotMap<InnerId, Entity>,
        entity2id: HashMap<Entity, InnerId>,
    }

    impl SharedEntityTracking {
        fn id_to_inner(id: SharedEntityId) -> InnerId {
            InnerId(KeyData::from_ffi(id.0))
        }

        fn inner_to_id(inner: InnerId) -> SharedEntityId {
            SharedEntityId(inner.0.as_ffi())
        }

        pub fn new() -> Self {
            Default::default()
        }

        pub fn insert(&mut self, entity: Entity) -> SharedEntityId {
            let inner = self.id2entity.insert(entity);
            self.entity2id.insert(entity, inner);
            Self::inner_to_id(inner)
        }

        pub fn remove_by_entity(&mut self, entity: Entity) -> Option<(SharedEntityId, Entity)> {
            //
        }

        pub fn remove_by_id(&mut self, id: SharedEntityId) -> Option<(SharedEntityId, Entity)> {
            let e = self.id2entity.remove(Self::id_to_inner(id))?;
            self.entity2id.remove(&e).expect("unreachable");
            Some((id, e))
        }
    }
}

#[derive(Component, Debug, Clone)]
pub struct ClientInfo {
    pub info: ClientSharedInfo,
}

fn network_handshake(world: &mut World) {
    let init_msg = WrtsMatchInitMessage::recv_sync(&mut stdin()).unwrap();

    let (handler_tx, msgs_rx) = mpsc::sync_channel::<WrtsMatchMessage>(128);
    let (msgs_tx, handler_rx) = mpsc::sync_channel::<WrtsMatchMessage>(128);

    std::thread::spawn(move || {
        stdin_handler(handler_tx);
    });
    std::thread::spawn(move || {
        stdout_handler(handler_rx);
    });

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
                    world.send_event(AppExit::from_code(1)).unwrap();
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
        world.spawn(ClientInfo { info: cl_info });
    }

    world.insert_resource(MessagesSend(msgs_tx));
    world.insert_non_send_resource(MessagesRecv(msgs_rx));
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
                exit.write(AppExit::from_code(1));
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
                    exit.write(AppExit::from_code(1));
                }
            }
            Message::Client2Match(Client2Match::InitB { .. })
            | Message::Match2Client(_)
            | Message::Client2Lobby(_)
            | Message::Lobby2Client(_) => {
                error!("Received unexpected message: {msg:?}");
            }
        };
    }
}
