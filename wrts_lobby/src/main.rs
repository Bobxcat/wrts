use std::{
    fmt::Display,
    io::{Read, Write},
    path::PathBuf,
    process::{self, Stdio},
    time::Duration,
};

use anyhow::{Result, anyhow};
use tokio::sync::mpsc;
use tracing::{Instrument, error, info, info_span, level_filters::LevelFilter};
use tracing_subscriber::EnvFilter;
use wrts_messaging::{Client2Lobby, Lobby2Client, Message};
use wtransport::{Endpoint, Identity, ServerConfig, endpoint::IncomingSession};

use crate::temp_dir::TempDirBuilder;

mod clients;
mod temp_dir;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClientId(pub u32);

impl Display for ClientId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cl{}", self.0)
    }
}

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
    let res = handle_connection_inner(info).await;
    error!("{:?}", res);
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

    info!("Waiting for stream to be accepted by client...");
    let (mut client_tx, mut client_rx) = connection.open_bi().await?.await?;

    info!("Sending client initial information");

    Message::Lobby2Client(Lobby2Client::InitialInformation {
        client_id: client_id.0,
    })
    .send(&mut client_tx)
    .await?;

    let Message::Client2Lobby(Client2Lobby::InitialInformationResponse { username }) =
        Message::recv(&mut client_rx).await?
    else {
        return Err(anyhow!(
            "Expected network message: `Client2Lobby::InitialInformationResponse`"
        ));
    };

    info!("{client_id} username selected: {username}");

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let _tmp_dir = TempDirBuilder::build();
    init_logging();

    let config = ServerConfig::builder()
        .with_bind_default(4433)
        .with_identity(Identity::self_signed(["localhost"]).unwrap())
        .keep_alive_interval(Some(Duration::from_secs(3)))
        .build();

    let ep = Endpoint::server(config)?;

    info!("Endpoint created");

    let stderr = temp_dir::log_create(format!(
        "wrts_log_{:x}.txt",
        rand::random_range(0..(1024 * 1024))
    ))?;

    // let stderr = std::fs::File::create(&format!(
    //     "wrts_log_{:x}.txt",
    //     rand::random_range(0..(1024 * 1024))
    // ))?;

    let mut x = process::Command::new(temp_dir::wrts_match_exe())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(stderr.try_clone().unwrap()))
        .spawn()
        .unwrap();

    let mut stdin = x.stdin.take().unwrap();
    let mut stdout = x.stdout.take().unwrap();
    // let mut stderr = x.stderr.take().unwrap();

    stdin.write_all(b"Hello world!!")?;
    let mut buf = vec![0; 13];
    stdout.read_exact(&mut buf)?;
    println!("STDOUT(echo): `{}`", String::from_utf8_lossy(&buf));

    let mut buf = vec![0; 16];
    stdout.read_exact(&mut buf)?;
    println!("STDOUT: `{}`", String::from_utf8_lossy(&buf));

    let mut s = String::new();
    stdout.read_to_string(&mut s)?;
    println!("STDOUT: `{s}`");

    let _ = x.wait().unwrap();

    for id in 0.. {
        let client_id = ClientId(id);
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
