use bevy::prelude::*;
use std::io::stdin;
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::{collections::HashMap, io::Write, ops::Deref};
use wrts_messaging::{Client2Match, Match2Client, Message, WrtsMatchMessage};

use wrts_messaging::{
    ClientId, ClientSharedInfo, RecvFromStream, WrtsMatchInitMessage, read_from_stream_sync,
    write_to_stream_sync,
};

pub use crate::networking::shared_entity_tracking::SharedEntityTracking;
use crate::ship::Ship;
use crate::{FireTarget, MoveOrder, Team};

pub struct NetworkingPlugin;

impl Plugin for NetworkingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, network_handshake)
            .add_systems(FixedUpdate, (read_messages, send_transform_updates));
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
                match &msg.msg {
                    Message::Match2Client(Match2Client::SetEntityPos { .. }) => {
                        trace!("Sending: {msg:?}")
                    }
                    _ => info!("Sending: {msg:?}"),
                }

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

impl MessagesSend {
    pub fn send(&self, msg: WrtsMatchMessage) {
        SyncSender::send(&self, msg)
            .expect("`MessagesSend` should never disconnect unless the bevy app has closed down")
    }
}

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
    use std::{collections::HashMap, ops::Index};

    use bevy::ecs::{entity::Entity, resource::Resource};
    use slotmap::{KeyData, SlotMap};
    use wrts_messaging::SharedEntityId;

    slotmap::new_key_type! {
        struct InnerSharedId;
    }

    /// Keeps track of the mapping between
    #[derive(Resource, Debug, Default, Clone)]
    pub struct SharedEntityTracking {
        shared2local: SlotMap<InnerSharedId, Entity>,
        local2shared: HashMap<Entity, InnerSharedId>,
    }

    impl SharedEntityTracking {
        fn shared_to_inner(id: SharedEntityId) -> InnerSharedId {
            InnerSharedId(KeyData::from_ffi(id.0))
        }

        fn inner_to_shared(inner: InnerSharedId) -> SharedEntityId {
            SharedEntityId(inner.0.as_ffi())
        }

        pub fn insert(&mut self, local: Entity) -> SharedEntityId {
            let inner = self.shared2local.insert(local);
            self.local2shared.insert(local, inner);
            Self::inner_to_shared(inner)
        }

        pub fn remove_by_local(&mut self, local: Entity) -> Option<(SharedEntityId, Entity)> {
            let id = self.local2shared.remove(&local)?;
            self.shared2local.remove(id).expect("unreachable");
            Some((Self::inner_to_shared(id), local))
        }

        pub fn remove_by_shared(
            &mut self,
            shared: SharedEntityId,
        ) -> Option<(SharedEntityId, Entity)> {
            let e = self.shared2local.remove(Self::shared_to_inner(shared))?;
            self.local2shared.remove(&e).expect("unreachable");
            Some((shared, e))
        }

        pub fn get_by_shared(&self, shared: SharedEntityId) -> Option<Entity> {
            self.shared2local
                .get(Self::shared_to_inner(shared))
                .copied()
        }

        pub fn get_by_local(&self, local: Entity) -> Option<SharedEntityId> {
            self.local2shared
                .get(&local)
                .copied()
                .map(Self::inner_to_shared)
        }
    }
}

#[derive(Component, Debug, Clone)]
pub struct ClientInfo {
    pub info: ClientSharedInfo,
}

fn network_handshake(world: &mut World) {
    info!(
        "`WrtsMatchMessage` in-memory size: {}B",
        std::mem::size_of::<WrtsMatchMessage>()
    );
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
    world.init_resource::<SharedEntityTracking>();
    world.insert_non_send_resource(MessagesRecv(msgs_rx));
}

fn read_messages(
    mut commands: Commands,
    msgs_rx: NonSend<MessagesRecv>,
    msgs_tx: Res<MessagesSend>,
    shared_entities: Res<SharedEntityTracking>,
    mut exit: EventWriter<AppExit>,

    ships: Query<&Ship>,
    teams: Query<&Team>,
) {
    loop {
        let WrtsMatchMessage {
            client: msg_sender,
            msg,
        } = match msgs_rx.try_recv() {
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
                msgs_tx.send(WrtsMatchMessage {
                    client: msg_sender,
                    msg: Message::Match2Client(Match2Client::PrintMsg(s)),
                });
            }
            Message::Client2Match(Client2Match::SetMoveOrder { id, waypoints }) => {
                let Some(local) = shared_entities.get_by_shared(id) else {
                    warn!("Client {msg_sender} sent message with bad id: {id:?}");
                    continue;
                };
                if teams
                    .get(local)
                    .ok()
                    .and_then(|team| (team.0 == msg_sender).then_some(()))
                    .is_none()
                {
                    warn!(
                        "Client {msg_sender} tried to SetMoveOrder on an entity not owned by them"
                    );
                    continue;
                }
                commands.entity(local).insert(MoveOrder { waypoints });
            }
            Message::Client2Match(Client2Match::SetFireTarg { id, targ }) => {
                let Some(local) = shared_entities.get_by_shared(id) else {
                    warn!("Client {msg_sender} sent message with bad id: {id:?}");
                    continue;
                };
                if teams
                    .get(local)
                    .ok()
                    .and_then(|team| (team.0 == msg_sender).then_some(()))
                    .is_none()
                {
                    warn!(
                        "Client {msg_sender} tried to SetMoveOrder on an entity not owned by them"
                    );
                    continue;
                }
                match targ {
                    Some(targ) => {
                        let Some(targ_local) = shared_entities.get_by_shared(targ) else {
                            warn!("Client {msg_sender} sent message with bad id: {targ:?}");
                            continue;
                        };
                        if ships.contains(targ_local) {
                            commands
                                .entity(local)
                                .insert(FireTarget { ship: targ_local });
                        } else {
                            warn!(
                                "Client {msg_sender} tried to SetFireTarg at a bad target: {targ:?}"
                            );
                        }
                    }
                    None => {
                        commands.entity(local).try_remove::<FireTarget>();
                    }
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

fn send_transform_updates(
    transforms: Query<(Entity, &Transform), Changed<Transform>>,
    clients: Query<&ClientInfo>,
    msgs_tx: Res<MessagesSend>,
    shared_entities: Res<SharedEntityTracking>,
) {
    for (local, trans) in transforms {
        let Some(shared) = shared_entities.get_by_local(local) else {
            continue;
        };
        for cl in clients {
            msgs_tx.send(WrtsMatchMessage {
                client: cl.info.id,
                msg: Message::Match2Client(Match2Client::SetEntityPos {
                    id: shared,
                    pos: trans.translation.truncate(),
                }),
            });
        }
    }
}
