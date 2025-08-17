use std::{
    fmt::Display,
    io::{Read, Write},
    path::PathBuf,
    process::{self, Stdio},
    time::Duration,
};

use anyhow::{Result, anyhow};
use tokio::sync::{mpsc, oneshot};
use tracing::{Instrument, error, info, info_span, level_filters::LevelFilter};
use tracing_subscriber::EnvFilter;
use wrts_messaging::{Client2Lobby, ClientId, Lobby2Client, Message};
use wtransport::{Endpoint, Identity, ServerConfig, endpoint::IncomingSession};

use crate::{
    clients::{ClientInfo, Clients, ClientsEvent},
    temp_dir::TempDirBuilder,
};

mod clients;
mod temp_dir;

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
}

async fn handle_connection(info: NewConnectionInfo) {
    let client_id = info.client_id;
    let exit = handle_connection_inner(info).await;
    match exit {
        Ok(()) => info!("{client_id} Exited successfully"),
        Err(err) => error!("{client_id} Exited with error: `{err}`"),
    }
    {
        let mut clients = Clients::lock().await;
        clients.id2info.remove(&client_id);
        clients.send(ClientsEvent::ClientLeft { id: client_id });
    }
}

async fn handle_connection_inner(
    NewConnectionInfo {
        incoming_session,
        client_id,
    }: NewConnectionInfo,
) -> Result<()> {
    info!("Waiting for session request...");

    let session_request = incoming_session.await?;

    info!(
        "New session: Authority: '{}', Path: '{}'",
        session_request.authority(),
        session_request.path()
    );

    let connection = session_request.accept().await?;

    info!("Remote address: `{}`", connection.remote_address());

    info!("Waiting for stream to be accepted by client...");
    let (mut tx, mut rx) = connection.accept_bi().await?;

    info!("Sending client initial information");

    Message::Lobby2Client(Lobby2Client::InitialInformation { client_id })
        .send(&mut tx)
        .await?;

    let Message::Client2Lobby(Client2Lobby::InitialInformationResponse { username }) =
        Message::recv(&mut rx).await?
    else {
        return Err(anyhow!(
            "Expected network message: `Client2Lobby::InitialInformationResponse`"
        ));
    };

    info!("username selected: `{username}`");

    let mut clients_events = {
        let mut clients = Clients::lock().await;
        clients.id2info.insert(
            client_id,
            ClientInfo {
                id: client_id,
                user: username,
            },
        );
        clients.send(ClientsEvent::ClientJoined { id: client_id });
        // Note: this loop includes _this_ client
        for (&cl_id, cl_info) in &clients.id2info {
            Message::Lobby2Client(Lobby2Client::ClientJoined {
                client_id: cl_id,
                username: cl_info.user.clone(),
            })
            .send(&mut tx)
            .await?;
        }
        clients.subscribe()
    };

    let (client_tx, client_rx) = {
        let (handler2client_tx, mut handler2client_rx) = mpsc::channel::<Message>(64);
        let (client2handler_tx, client2handler_rx) = mpsc::channel::<Message>(64);

        tokio::spawn(
            async move {
                loop {
                    let Some(msg) = handler2client_rx.recv().await else {
                        return;
                    };
                    if let Err(err) = msg.send(&mut tx).await {
                        error!("Failed to send to client: {err}");
                        return;
                    };
                }
            }
            .instrument(info_span!("handler2client")),
        );

        tokio::spawn(
            async move {
                loop {
                    let msg = match Message::recv(&mut rx).await {
                        Ok(msg) => msg,
                        Err(err) => {
                            error!("Client sent bad message: {err}");
                            return;
                        }
                    };
                    let Ok(()) = client2handler_tx.send(msg).await else {
                        return;
                    };
                }
            }
            .instrument(info_span!("client2handler")),
        );

        (handler2client_tx, client2handler_rx)
    };

    loop {
        if client_tx.is_closed() || client_rx.is_closed() {
            return Err(anyhow!("Client disconnected"));
        }

        let event = clients_events.recv().await?;
        match event {
            ClientsEvent::ClientJoined { id } => {
                client_tx
                    .send(Message::Lobby2Client(Lobby2Client::ClientJoined {
                        client_id: id,
                        username: Clients::lock().await.id2info[&id].user.clone(),
                    }))
                    .await?
            }
            ClientsEvent::ClientLeft { id } => {
                client_tx
                    .send(Message::Lobby2Client(Lobby2Client::ClientLeft {
                        client_id: id,
                    }))
                    .await?
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

#[tokio::main]
async fn main() -> Result<()> {
    let _tmp_dir = TempDirBuilder::build();
    init_logging();

    tokio::spawn(trace_client_events().instrument(info_span!("trace_client_events")));

    let config = ServerConfig::builder()
        .with_bind_default(wrts_messaging::DEFAULT_PORT)
        .with_identity(Identity::self_signed(["localhost"]).unwrap())
        .keep_alive_interval(Some(Duration::from_secs(3)))
        .build();

    let ep = Endpoint::server(config)?;

    info!("Endpoint created");

    for id in 0.. {
        let client_id = ClientId(id);
        info!("Open sessions: {}", ep.open_connections());
        info!("Awaiting session {client_id}");
        let session = ep.accept().await;
        tokio::spawn(
            handle_connection(NewConnectionInfo {
                incoming_session: session,
                client_id,
            })
            .instrument(info_span!("Client Connection", id)),
        );
    }

    Ok(())
}
