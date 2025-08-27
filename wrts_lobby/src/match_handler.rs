use std::{collections::HashMap, sync::Arc};

use itertools::Itertools;
use slotmap::SlotMap;
use tokio::sync::{Mutex, mpsc};
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
    tx: mpsc::Sender<Matchmaker2ClientHandler>,
    rx: mpsc::Receiver<ClientHandler2Matchmaker>,
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

pub struct Matchmaker {
    active_matches: SlotMap<MatchId, ActiveMatch>,
    connected_clients: HashMap<ClientId, MatchmakerClientData>,
}

impl Matchmaker {
    pub fn spawn() -> Arc<Mutex<Self>> {
        let mm = Arc::new(Mutex::new(Self {
            active_matches: SlotMap::default(),
            connected_clients: HashMap::default(),
        }));
        tokio::spawn(matchmaker_runner(mm.clone()).instrument(info_span!("Matchmaker Runner")));
        mm
    }

    /// Subscribes a client handler to this matchmaker,
    /// which doesn't yet signify that client as ready to matchmake
    pub fn subscribe(&mut self, client_id: ClientId) -> ClientHandlerMatchmakerSubscription {
        let (mmtx, clrx) = mpsc::channel(1024);
        let (cltx, mmrx) = mpsc::channel(1024);

        self.connected_clients.insert(
            client_id,
            MatchmakerClientData {
                tx: mmtx,
                rx: mmrx,
                state: ClientState::InLobby,
            },
        );
        ClientHandlerMatchmakerSubscription { tx: cltx, rx: clrx }
    }
}

async fn matchmaker_runner(mm: Arc<Mutex<Matchmaker>>) {
    loop {
        tokio::task::yield_now().await;
        let mut mm = mm.lock().await;

        // Handle client messages
        let mut clients_disconnected = Vec::new();
        for (&cl, cl_data) in &mut mm.connected_clients {
            loop {
                match cl_data.rx.try_recv() {
                    Ok(ClientHandler2Matchmaker::SetReadyForMatch { is_ready }) => {
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
                    Err(mpsc::error::TryRecvError::Empty) => break,
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        clients_disconnected.push(cl);
                        break;
                    }
                }
            }
        }

        for cl in clients_disconnected {
            warn!("Disconnected: {cl}");
            mm.connected_clients.remove(&cl);
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
}
