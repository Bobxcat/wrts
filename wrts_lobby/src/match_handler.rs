use std::{collections::HashMap, sync::Arc};

use itertools::Itertools;
use slotmap::SlotMap;
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use tracing::{Instrument, info_span, warn};
use wrts_messaging::{
    ClientId, Message, RecvFromStream, SendToStream, WrtsMatchInitMessage, WrtsMatchMessage,
};

use crate::temp_dir::WrtsMatchProcess;

slotmap::new_key_type! {
    pub struct MatchId;
}

pub enum Matchmaker2ClientHandler {
    MatchJoined {
        match_id: MatchId,
        match_instance_tx: mpsc::Sender<Message>,
        match_instance_rx: mpsc::Receiver<Message>,
    },
}

pub enum ClientHandler2Matchmaker {
    SetReadyForMatch { is_ready: bool },
}

pub struct ClientHandlerMatchmakerSubscription {
    pub tx: mpsc::Sender<ClientHandler2Matchmaker>,
    pub rx: mpsc::Receiver<Matchmaker2ClientHandler>,
}

enum ClientState {
    InLobby,
    ReadyForMatch,
    InMatch(MatchId),
}

struct MatchmakerClientData {
    disconnected: CancellationToken,
    tx: mpsc::Sender<Matchmaker2ClientHandler>,
    state: ClientState,
}

#[derive(Debug, Clone)]
struct ActiveMatch {
    id: MatchId,
    clients: [ClientId; 2],
}

async fn match_instance_router(
    match_instance: ActiveMatch,
    client_channels: HashMap<ClientId, (mpsc::Sender<Message>, mpsc::Receiver<Message>)>,
) {
    let mut process = WrtsMatchProcess::spawn().await.unwrap();

    WrtsMatchInitMessage {
        clients: match_instance.clients,
    }
    .send(&mut process.stdin)
    .await
    .unwrap();

    let (client_tx, mut client_rx): (HashMap<_, _>, Vec<_>) = client_channels
        .into_iter()
        .map(|(cl, (tx, rx))| ((cl, tx), (cl, rx)))
        .unzip();

    tokio::spawn({
        async move {
            loop {
                let Ok(msg) = WrtsMatchMessage::recv(&mut process.stdout).await else {
                    warn!("Match instance closed down");
                    return;
                };

                if let Err(_) = client_tx[&msg.client].send(msg.msg).await {
                    warn!("Client closed down");
                    return;
                }
            }
        }
    });

    'main_loop: loop {
        // Without yielding, this task wouldn't await until a client sends a message
        tokio::task::yield_now().await;

        for (client_id, rx) in &mut client_rx {
            let msg = match rx.try_recv() {
                Ok(msg) => msg,
                Err(mpsc::error::TryRecvError::Empty) => continue,
                Err(mpsc::error::TryRecvError::Disconnected) => break 'main_loop,
            };

            let res = WrtsMatchMessage {
                client: *client_id,
                msg,
            }
            .send(&mut process.stdin)
            .await;

            if let Err(e) = res {
                warn!(
                    "Error sending message to match instance {:?}: {e}",
                    match_instance.id
                );
                break 'main_loop;
            }
        }
    }
    let _ = process.process.kill();
}

struct MatchmakerSubscribeMsg {
    pub client_id: ClientId,
    pub send_subscribtion: oneshot::Sender<ClientHandlerMatchmakerSubscription>,
}

pub struct MatchmakerSubscriber {
    new_clients_tx: mpsc::Sender<MatchmakerSubscribeMsg>,
}

impl MatchmakerSubscriber {
    /// Subscribes a client handler to this matchmaker,
    /// which doesn't yet signify that client as ready to matchmake
    pub async fn subscribe(&self, client_id: ClientId) -> ClientHandlerMatchmakerSubscription {
        let (tx, rx) = oneshot::channel();
        let _ = self
            .new_clients_tx
            .send(MatchmakerSubscribeMsg {
                client_id,
                send_subscribtion: tx,
            })
            .await;
        rx.await.expect("Matchmaker closed down!")
    }
}

pub struct Matchmaker {
    active_matches: SlotMap<MatchId, ActiveMatch>,
    connected_clients: HashMap<ClientId, MatchmakerClientData>,
}

impl Matchmaker {
    pub fn spawn() -> MatchmakerSubscriber {
        let mm = Self {
            active_matches: SlotMap::default(),
            connected_clients: HashMap::default(),
        };
        let (tx, rx) = mpsc::channel(128);
        tokio::spawn(
            async move {
                let disconnect = CancellationToken::new();
                matchmaker_runner(disconnect.clone(), mm, rx).await;
                disconnect.cancel();
            }
            .instrument(info_span!("Matchmaker Runner")),
        );
        MatchmakerSubscriber { new_clients_tx: tx }
    }
}

enum MatchmakerMessage {
    Client2MM {
        client: ClientId,
        msg: ClientHandler2Matchmaker,
    },
    ClientJoined {
        subscribe: MatchmakerSubscribeMsg,
    },
}

async fn matchmaker_new_clients_router(
    disconnect: CancellationToken,
    tx: mpsc::Sender<MatchmakerMessage>,
    mut new_clients: mpsc::Receiver<MatchmakerSubscribeMsg>,
) {
    loop {
        tokio::select! {
            msg = new_clients.recv() => {
                let Some(msg) = msg else {
                    break;
                };
                if let Err(_) = tx.send(MatchmakerMessage::ClientJoined { subscribe: msg }).await {
                    break;
                }
            }
            _ = disconnect.cancelled() => {
                break;
            }
        }
    }
    disconnect.cancel();
}

async fn matchmaker_client_router(
    disconnect: CancellationToken,
    disconnect_client: CancellationToken,
    tx: mpsc::Sender<MatchmakerMessage>,
    mut client_msgs: mpsc::Receiver<ClientHandler2Matchmaker>,
    client: ClientId,
) {
    loop {
        tokio::select! {
            msg = client_msgs.recv() => {
                let Some(msg) = msg else {
                    break;
                };
                if let Err(_) = tx.send(MatchmakerMessage::Client2MM { client, msg }).await {
                    break;
                }
            }
            _ = disconnect.cancelled() => {
                break;
            }
        }
    }
    disconnect_client.cancel();
}

async fn matchmaker_runner(
    disconnect: CancellationToken,
    mut mm: Matchmaker,
    new_clients: mpsc::Receiver<MatchmakerSubscribeMsg>,
) {
    let (msgs_tx, mut msgs) = mpsc::channel(1024);
    tokio::spawn(matchmaker_new_clients_router(
        disconnect.clone(),
        msgs_tx.clone(),
        new_clients,
    ));

    while let Some(msg) = msgs.recv().await {
        let clients_disconnected = mm
            .connected_clients
            .iter()
            .filter_map(|(cl, cl_data)| {
                (cl_data.disconnected.is_cancelled() || cl_data.tx.is_closed()).then_some(*cl)
            })
            .collect_vec();

        for cl in clients_disconnected {
            warn!("Disconnected: {cl}");
            mm.connected_clients.remove(&cl);
        }

        match msg {
            MatchmakerMessage::Client2MM { client, msg } => match msg {
                ClientHandler2Matchmaker::SetReadyForMatch { is_ready } => {
                    let Some(cl_data) = mm.connected_clients.get_mut(&client) else {
                        continue;
                    };
                    match cl_data.state {
                        ClientState::InLobby | ClientState::ReadyForMatch => {
                            cl_data.state = match is_ready {
                                true => ClientState::ReadyForMatch,
                                false => ClientState::InLobby,
                            };
                        }
                        ClientState::InMatch(_) => continue,
                    }
                }
            },
            MatchmakerMessage::ClientJoined { subscribe } => {
                let (mmtx, clrx) = mpsc::channel(1024);
                let (cltx, mmrx) = mpsc::channel(1024);
                let disconnect_client = CancellationToken::new();
                tokio::spawn(matchmaker_client_router(
                    disconnect.clone(),
                    disconnect_client.clone(),
                    msgs_tx.clone(),
                    mmrx,
                    subscribe.client_id,
                ));

                mm.connected_clients.insert(
                    subscribe.client_id,
                    MatchmakerClientData {
                        disconnected: disconnect_client.clone(),
                        tx: mmtx,
                        state: ClientState::InLobby,
                    },
                );
                let sub = ClientHandlerMatchmakerSubscription { tx: cltx, rx: clrx };
                let _ = subscribe.send_subscribtion.send(sub);
            }
        }

        // Make a match if possible
        let clients_ready_for_match = mm
            .connected_clients
            .iter()
            .filter_map(|(cl, cl_data)| {
                matches!(cl_data.state, ClientState::ReadyForMatch).then_some(*cl)
            })
            .collect_vec();

        if clients_ready_for_match.len() >= 2 {
            let clients: [ClientId; 2] = std::array::from_fn(|i| clients_ready_for_match[i]);
            let match_id = mm.active_matches.insert_with_key(|match_id| ActiveMatch {
                id: match_id,
                clients,
            });
            let mut client_channels = HashMap::new();
            for cl in clients {
                let cl_data = mm.connected_clients.get_mut(&cl).unwrap();
                cl_data.state = ClientState::InMatch(match_id);
                let (match_instance_tx, rx) = mpsc::channel(1024);
                let (tx, match_instance_rx) = mpsc::channel(1024);
                if let Err(_) = cl_data
                    .tx
                    .send(Matchmaker2ClientHandler::MatchJoined {
                        match_id,
                        match_instance_tx,
                        match_instance_rx,
                    })
                    .await
                {
                    // Client disconnected, which will be handled when the `match_instance_router` notices a missing client
                }
                client_channels.insert(cl, (tx, rx));
            }

            tokio::spawn(
                match_instance_router(mm.active_matches[match_id].clone(), client_channels)
                    .instrument(info_span!("match_instance_router", ?match_id)),
            );
        }
    }

    warn!("Matchmaker disconnecting!");
}
