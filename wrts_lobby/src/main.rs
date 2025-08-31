use std::time::Duration;

use anyhow::{Result, anyhow};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{Instrument, debug, info, info_span, level_filters::LevelFilter, warn};
use tracing_subscriber::EnvFilter;
use wrts_messaging::{
    Client2Lobby, ClientId, ClientSharedInfo, Lobby2Client, Message, RecvFromStream, SendToStream,
};
use wtransport::{Endpoint, Identity, ServerConfig, endpoint::IncomingSession};
use clap::Parser;

use crate::{
    clients::{ClientData, Clients, ClientsEvent},
    match_handler::{
        ClientHandler2Matchmaker, ClientHandlerMatchmakerSubscription, Matchmaker,
        Matchmaker2ClientHandler,
    },
    temp_dir::TempDirBuilder,
};

mod clients;
mod match_handler;
mod temp_dir;

#[deny(dead_code)]
fn init_logging() {
    let env_filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy();

    tracing_subscriber::fmt()
        .with_target(true)
        .with_level(true)
        .with_env_filter(env_filter)
        .init();
}

struct NewConnectionInfo {
    incoming_session: IncomingSession,
    client_id: ClientId,
    mm_subscription: ClientHandlerMatchmakerSubscription,
}

async fn handle_connection(info: NewConnectionInfo) {
    info!("Handling new client connection");
    let client_id = info.client_id;
    let abort_token = CancellationToken::new();
    let exit = handle_connection_inner(info, abort_token.clone()).await;
    match exit {
        Ok(()) => info!("{client_id} Exited successfully"),
        Err(err) => info!("{client_id} Exited with error: `{err}`"),
    }
    {
        let mut clients = Clients::lock().await;
        let client_info = clients.id2info.remove(&client_id);
        assert!(client_info.is_some());
        clients.send(ClientsEvent::ClientLeft { id: client_id });
    }
    abort_token.cancel();
}

async fn handle_connection_inner(
    NewConnectionInfo {
        incoming_session,
        client_id,
        mut mm_subscription,
    }: NewConnectionInfo,
    abort_token: CancellationToken,
) -> Result<()> {
    debug!("Waiting for session request...");

    let session_request = incoming_session.await?;

    debug!(
        "New session: Authority: '{}', Path: '{}'",
        session_request.authority(),
        session_request.path()
    );

    let connection = session_request.accept().await?;

    debug!("Remote address: `{}`", connection.remote_address());

    debug!("Waiting for stream to be accepted by client...");
    let (mut tx, mut rx) = connection.accept_bi().await?;

    debug!("Sending client initial information");

    Message::Lobby2Client(Lobby2Client::InitA { client_id })
        .send(&mut tx)
        .await?;

    let Message::Client2Lobby(Client2Lobby::InitB { username }) = Message::recv(&mut rx).await?
    else {
        return Err(anyhow!(
            "Expected network message: `Client2Lobby::InitialInformationResponse`"
        ));
    };

    debug!("username selected: `{username}`");

    let mut clients_events = {
        let mut clients = Clients::lock().await;
        clients.id2info.insert(
            client_id,
            ClientData {
                info: ClientSharedInfo {
                    id: client_id,
                    user: username,
                },
            },
        );
        clients.send(ClientsEvent::ClientJoined { id: client_id });
        // Note: this loop includes _this_ client
        for (&_cl_id, cl_data) in &clients.id2info {
            Message::Lobby2Client(Lobby2Client::ClientJoined {
                info: cl_data.info.clone(),
            })
            .send(&mut tx)
            .await?;
        }
        clients.subscribe()
    };

    let (client_tx, mut client_rx) = {
        let (handler2client_tx, mut handler2client_rx) = mpsc::channel::<Message>(64);
        let (client2handler_tx, client2handler_rx) = mpsc::channel::<Message>(64);

        let abort_token_cloned = abort_token.clone();

        tokio::spawn(
            async move {
                let abort_token = abort_token_cloned;
                loop {
                    let Some(msg) = handler2client_rx.recv().await else {
                        break;
                    };

                    tokio::select! {
                        res = msg.send(&mut tx) => {
                            if let Err(err) = res {
                                warn!("Failed to send to client: {err}");
                                break;
                            };
                        }
                        _ = abort_token.cancelled() => {
                            break;
                        }
                    }
                }
                abort_token.cancel();
            }
            .instrument(info_span!("handler2client")),
        );

        let abort_token_cloned = abort_token.clone();
        tokio::spawn(
            async move {
                let abort_token = abort_token_cloned;
                loop {
                    tokio::select! {
                        msg = Message::recv(&mut rx) => {
                            let msg = match msg {
                                Ok(msg) => msg,
                                Err(err) => {
                                    warn!("Client sent bad message: {err}");
                                    break;
                                }
                            };
                            let Ok(()) = client2handler_tx.send(msg).await else {
                                break;
                            };
                        }
                        _ = abort_token.cancelled() => {
                            break;
                        }
                    }
                }
                abort_token.cancel();
            }
            .instrument(info_span!("client2handler")),
        );

        (handler2client_tx, client2handler_rx)
    };

    enum ClientState {
        InLobby,
        InMatch {
            match_instance_tx: mpsc::Sender<Message>,
            match_instance_rx: mpsc::Receiver<Message>,
        },
    }

    let mut state = ClientState::InLobby;

    let process_clients_event = async |event: ClientsEvent| -> Result<()> {
        match event {
            ClientsEvent::ClientJoined { id } => {
                let clients = Clients::lock().await;
                client_tx
                    .send(Message::Lobby2Client(Lobby2Client::ClientJoined {
                        info: ClientSharedInfo {
                            id: id,
                            user: clients.id2info[&id].info.user.clone(),
                        },
                    }))
                    .await?;
            }
            ClientsEvent::ClientLeft { id } => {
                client_tx
                    .send(Message::Lobby2Client(Lobby2Client::ClientLeft {
                        client_id: id,
                    }))
                    .await?
            }
        }

        Ok(())
    };

    let handle_client_message_in_lobby = async |msg: Message| -> Result<()> {
        match msg {
            Message::Client2Lobby(Client2Lobby::SetReadyForMatch { is_ready }) => mm_subscription
                .tx
                .send(ClientHandler2Matchmaker::SetReadyForMatch { is_ready })
                .await
                .map_err(|_| anyhow!("Matchmaker disconnnected"))?,
            Message::Client2Lobby(Client2Lobby::InitB { .. })
            | Message::Lobby2Client(_)
            | Message::Client2Match(_)
            | Message::Match2Client(_) => warn!(
                "Unexpected message during `handle_client_message`: {:?}",
                msg
            ),
        }
        Ok(())
    };

    loop {
        match &mut state {
            ClientState::InLobby => {
                tokio::select! {
                    cl_msg = client_rx.recv() => {
                        let cl_msg = cl_msg.ok_or(anyhow!("Client disconnected"))?;
                        handle_client_message_in_lobby(cl_msg).await?
                    }
                    event = clients_events.recv() => {
                        process_clients_event(event?).await?
                    }
                    mm_msg = mm_subscription.rx.recv() => {
                        let mm_msg = mm_msg.ok_or(anyhow!("Matchmaker disconnected"))?;
                        match mm_msg {
                            Matchmaker2ClientHandler::MatchJoined { match_id: _, match_instance_tx, match_instance_rx } => {
                                state = ClientState::InMatch { match_instance_tx, match_instance_rx };
                                let _ = client_tx.send(Message::Lobby2Client(Lobby2Client::MatchJoined {  })).await;
                            },
                        }
                    }
                    _ = abort_token.cancelled() => {
                        return Err(anyhow!("Client disconnected"));
                    }
                }
            }
            ClientState::InMatch {
                match_instance_tx,
                match_instance_rx,
            } => {
                tokio::select! {
                    cl_msg = client_rx.recv() => {
                        let cl_msg = cl_msg.ok_or(anyhow!("Client disconnected"))?;
                        match_instance_tx.send(cl_msg).await.map_err(|_| anyhow!("Match instance disconnected"))?;
                    }
                    match_msg = match_instance_rx.recv() => {
                        let match_msg = match_msg.ok_or(anyhow!("Match instance disconnected"))?;
                        client_tx.send(match_msg).await.map_err(|_| anyhow!("Client disconnected"))?;
                    }
                    mm_msg = mm_subscription.rx.recv() => {
                        let mm_msg = mm_msg.ok_or(anyhow!("Matchmaker disconnected"))?;
                        match mm_msg {
                            Matchmaker2ClientHandler::MatchJoined { .. } => {
                                return Err(anyhow!("Matchmaker sent `MatchJoined` message when client already in match"))
                            },
                        }
                    }
                    _ = abort_token.cancelled() => {
                        return Err(anyhow!("Client disconnected"));
                    }
                }
            }
        }
    }
}

async fn trace_client_events() {
    let mut events = {
        let clients = Clients::lock().await;
        clients.subscribe()
    };
    while let Ok(ev) = events.recv().await {
        info!("{ev:?}");
    }
}

#[derive(Parser, Debug)]
enum Args {
    Lobby,
    Match,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    match args {
        Args::Lobby => {
            let _tmp_dir = TempDirBuilder::build();
            init_logging();

            tokio::spawn(trace_client_events().instrument(info_span!("Trace Clients Events")));

            let config = ServerConfig::builder()
                .with_bind_default(wrts_messaging::DEFAULT_PORT)
                .with_identity(Identity::self_signed(["localhost"]).unwrap())
                .keep_alive_interval(Some(Duration::from_secs(3)))
                .build();

            let ep = Endpoint::server(config)?;

            info!("Endpoint created");

            let mm = Matchmaker::spawn();

            for id in 0.. {
                let client_id = ClientId(id);
                info!("Open sessions: {}", ep.open_connections());
                info!("Awaiting session {client_id}");
                let session = ep.accept().await;
                let mm_subscription = mm.lock().await.subscribe(client_id);
                tokio::spawn(
                    handle_connection(NewConnectionInfo {
                        incoming_session: session,
                        client_id,
                        mm_subscription,
                    })
                    .instrument(info_span!("Client Connection", %client_id)),
                );
            }
        }
        Args::Match => {
            wrts_match::start_match().expect("Couldn't start match");
        }
    }

    Ok(())
}
