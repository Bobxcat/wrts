use std::{
    fmt::Display,
    io::{self, Read, Write},
    pin::Pin,
    task::Poll,
};

use anyhow::{Result, anyhow};
use pin_project::pin_project;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use wtransport::{RecvStream, SendStream};

pub const DEFAULT_PORT: u16 = 4433;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ClientId(pub u32);

impl Display for ClientId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cl{}", self.0)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Client2Match {
    //
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Match2Client {
    //
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Client2Lobby {
    /// Handshake part B
    InitialInformationResponse {
        username: String,
    },
    SetReadyForMatch {
        is_ready: bool,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Lobby2Client {
    /// Handshake part A
    InitialInformation {
        client_id: ClientId,
    },
    ClientJoined {
        client_id: ClientId,
        username: String,
    },
    ClientLeft {
        client_id: ClientId,
    },
    MatchJoined {},
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Message {
    Client2Match(Client2Match),
    Match2Client(Match2Client),
    Client2Lobby(Client2Lobby),
    Lobby2Client(Lobby2Client),
}

#[pin_project]
pub struct TokioWebTransportCompat<'a, T> {
    #[pin]
    x: &'a mut T,
}

impl<'a, T> From<&'a mut T> for TokioWebTransportCompat<'a, T> {
    fn from(value: &'a mut T) -> Self {
        Self { x: value }
    }
}

impl tokio::io::AsyncRead for TokioWebTransportCompat<'_, RecvStream> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.project().x.poll_read(cx, buf)
    }
}

impl tokio::io::AsyncWrite for TokioWebTransportCompat<'_, SendStream> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<std::result::Result<usize, io::Error>> {
        self.project().x.poll_write(cx, buf)
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<std::result::Result<(), io::Error>> {
        self.project().x.poll_flush(cx)
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<std::result::Result<(), io::Error>> {
        self.project().x.poll_shutdown(cx)
    }
}

impl tokio::io::AsyncRead for TokioWebTransportCompat<'_, tokio::process::ChildStdout> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.project().x.poll_read(cx, buf)
    }
}

impl tokio::io::AsyncWrite for TokioWebTransportCompat<'_, tokio::process::ChildStdin> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<std::result::Result<usize, io::Error>> {
        self.project().x.poll_write(cx, buf)
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<std::result::Result<(), io::Error>> {
        self.project().x.poll_flush(cx)
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<std::result::Result<(), io::Error>> {
        self.project().x.poll_shutdown(cx)
    }
}

impl Message {
    pub async fn send<'a, T: 'a>(&self, stream: &'a mut T) -> Result<()>
    where
        TokioWebTransportCompat<'a, T>: tokio::io::AsyncWrite,
    {
        serialize_to_stream(self, stream).await
    }

    pub async fn recv<'a, T: 'a>(stream: &'a mut T) -> Result<Self>
    where
        TokioWebTransportCompat<'a, T>: tokio::io::AsyncRead,
    {
        deserialize_from_stream(stream).await
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WrtsMatchMessage {
    /// Refers to either
    /// * The target client of the message
    /// * The client that sent the message
    pub client: ClientId,
    pub msg: Message,
}

impl WrtsMatchMessage {
    pub async fn send<'a, T: 'a>(&self, stream: &'a mut T) -> Result<()>
    where
        TokioWebTransportCompat<'a, T>: tokio::io::AsyncWrite,
    {
        serialize_to_stream(self, stream).await
    }

    pub async fn recv<'a, T: 'a>(stream: &'a mut T) -> Result<Self>
    where
        TokioWebTransportCompat<'a, T>: tokio::io::AsyncRead,
    {
        deserialize_from_stream(stream).await
    }
}

pub async fn serialize_to_stream<'a, T: 'a>(msg: &impl Serialize, stream: &'a mut T) -> Result<()>
where
    TokioWebTransportCompat<'a, T>: tokio::io::AsyncWrite,
{
    let mut stream = TokioWebTransportCompat::<'a, T>::from(stream);
    let bytes = serde_json::to_vec(msg)?;
    let length_prefix: [u8; 4] = (bytes.len() as u32).to_be_bytes();
    stream.write_all(&length_prefix).await?;
    stream.write_all(&bytes).await?;
    Ok(())
}

pub async fn deserialize_from_stream<'a, T: 'a, M: DeserializeOwned>(stream: &'a mut T) -> Result<M>
where
    TokioWebTransportCompat<'a, T>: tokio::io::AsyncRead,
{
    let mut stream = TokioWebTransportCompat::<'a, T>::from(stream);
    let length_prefix = {
        let mut buf: [u8; 4] = [0; 4];
        stream.read_exact(&mut buf).await?;
        u32::from_be_bytes(buf)
    };
    let limit = 1024 * 1024;
    if length_prefix > limit {
        return Err(anyhow!(
            "A message was recieved of length: {length_prefix}b! The limit is {limit}b"
        ));
    }
    let mut data = vec![0u8; length_prefix as usize];
    stream.read_exact(&mut data).await?;
    Ok(serde_json::from_slice(&data)?)
}
