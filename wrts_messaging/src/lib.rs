use std::{
    fmt::Display,
    io::{self},
    pin::Pin,
    task::Poll,
    time::Duration,
};

use anyhow::{Result, anyhow};
use glam::{Quat, Vec2, Vec3};
use pin_project::pin_project;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use wrts_match_shared::ship_template::ShipTemplateId;
use wtransport::{RecvStream, SendStream};

pub const DEFAULT_PORT: u16 = 4433;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SharedEntityId(pub u64);

impl std::fmt::Debug for SharedEntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "shared{:x}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ClientId(pub u32);

impl Display for ClientId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cl{}", self.0)
    }
}

/// Basic __immutable__ info associated with a client,
/// established when first connecting
///
/// Copies of this data are safe to hold and trust for the duration
/// of a client's connection, since it's immutable
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientSharedInfo {
    pub id: ClientId,
    pub user: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Client2Match {
    InitB {
        info: ClientSharedInfo,
    },
    Echo(String),
    SetMoveOrder {
        id: SharedEntityId,
        waypoints: Vec<Vec2>,
    },
    SetFireTarg {
        id: SharedEntityId,
        targ: Option<SharedEntityId>,
    },
    LaunchTorpedoVolley {
        ship: SharedEntityId,
        dir: Vec2,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Match2Client {
    InitA {
        your_client: ClientId,
    },
    InitC {
        all_clients: Vec<ClientSharedInfo>,
    },
    PrintMsg(String),
    DestroyEntity(SharedEntityId),
    SpawnShip {
        id: SharedEntityId,
        team: ClientId,
        ship_base: ShipTemplateId,
        health: f64,
        pos: Vec2,
        rot: Quat,
        turret_rots: Vec<f32>,
    },
    SpawnBullet {
        id: SharedEntityId,
        team: ClientId,
        owning_ship: SharedEntityId,
        damage: f64,
        pos: Vec3,
        rot: Quat,
    },
    SpawnTorpedo {
        id: SharedEntityId,
        team: ClientId,
        owning_ship: SharedEntityId,
        damage: f64,
        pos: Vec2,
        vel: Vec2,
    },
    SetReloadedTorps {
        id: SharedEntityId,
        ready_to_fire: usize,
        /// Remaining time until each volley is ready, in ascending order
        /// (the next volley to be ready is at index 0)
        still_reloading: Vec<Duration>,
    },
    SetTrans {
        id: SharedEntityId,
        pos: Vec3,
        rot: Quat,
    },
    SetTurretDirs {
        id: SharedEntityId,
        turret_dirs: Vec<f32>,
    },
    SetHealth {
        id: SharedEntityId,
        health: f64,
    },
    SetMoveOrder {
        id: SharedEntityId,
        waypoints: Vec<Vec2>,
    },
    SetDetection {
        id: SharedEntityId,
        currently_detected: bool,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Client2Lobby {
    /// Handshake part B
    InitB {
        username: String,
    },
    SetReadyForMatch {
        is_ready: bool,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Lobby2Client {
    /// Handshake part A
    InitA {
        client_id: ClientId,
    },
    ClientJoined {
        info: ClientSharedInfo,
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

/// Wraps a message so it can be sent to/from a `wrts_match` instance
#[derive(Debug, Serialize, Deserialize)]
pub struct WrtsMatchMessage {
    /// Refers to either
    /// * The target client of the message
    /// * The client that sent the message
    pub client: ClientId,
    pub msg: Message,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WrtsMatchInitMessage {
    pub clients: [ClientId; 2],
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

#[allow(async_fn_in_trait)]
pub trait SendToStream {
    async fn send<'a, T: 'a>(&self, stream: &'a mut T) -> Result<()>
    where
        TokioWebTransportCompat<'a, T>: tokio::io::AsyncWrite;
    fn send_sync<T>(&self, stream: &mut T) -> Result<()>
    where
        T: std::io::Write;
}

impl<M> SendToStream for M
where
    M: Serialize,
{
    async fn send<'a, T: 'a>(&self, stream: &'a mut T) -> Result<()>
    where
        TokioWebTransportCompat<'a, T>: tokio::io::AsyncWrite,
    {
        write_to_stream_async(self, stream).await
    }

    fn send_sync<T>(&self, stream: &mut T) -> Result<()>
    where
        T: std::io::Write,
    {
        write_to_stream_sync(self, stream)
    }
}

#[allow(async_fn_in_trait)]
pub trait RecvFromStream: Sized {
    async fn recv<'a, T: 'a>(stream: &'a mut T) -> Result<Self>
    where
        TokioWebTransportCompat<'a, T>: tokio::io::AsyncRead;
    fn recv_sync<T>(stream: &mut T) -> Result<Self>
    where
        T: std::io::Read;
}

impl<M> RecvFromStream for M
where
    M: DeserializeOwned,
{
    async fn recv<'a, T: 'a>(stream: &'a mut T) -> Result<Self>
    where
        TokioWebTransportCompat<'a, T>: tokio::io::AsyncRead,
    {
        read_from_stream_async(stream).await
    }

    fn recv_sync<T>(stream: &mut T) -> Result<Self>
    where
        T: std::io::Read,
    {
        read_from_stream_sync(stream)
    }
}

mod serialize_maybe_sync_macro {
    #[macro_export]
    macro_rules! maybe_sync_read_write {
        (read async $stream:ident, $buf:ident) => {
            tokio::io::AsyncReadExt::read_exact(&mut $stream, &mut $buf).await?;
        };
        (read $stream:ident, $buf:ident) => {
            std::io::Read::read_exact(&mut $stream.x, &mut $buf)?;
        };
        (write async $stream:ident, $buf:ident) => {
            tokio::io::AsyncWriteExt::write_all(&mut $stream, &$buf).await?;
        };
        (write $stream:ident, $buf:ident) => {
            std::io::Write::write_all(&mut $stream.x, &$buf)?;
        };
    }

    #[macro_export]
    macro_rules! serialize_maybe_sync {
        (async de $func_name:ident) => {
            serialize_maybe_sync!{
                ___internal; de, async, $func_name, { TokioWebTransportCompat<'a, T>: tokio::io::AsyncRead }
            }
        };
        (sync de $func_name:ident) => {
            serialize_maybe_sync!{
                ___internal; de,, $func_name, { T: std::io::Read }
            }
        };
        (async ser $func_name:ident) => {
            serialize_maybe_sync!{
                ___internal; ser, async, $func_name, { TokioWebTransportCompat<'a, T>: tokio::io::AsyncWrite }
            }
        };
        (sync ser $func_name:ident) => {
            serialize_maybe_sync!{
                ___internal; ser,, $func_name, { T: std::io::Write }
            }
        };

        {___internal; de, $($async:ident)?, $func_name:ident, {$($stream_trait_bound:tt)*}} => {
            pub $($async)? fn $func_name<'a, T: 'a, M: DeserializeOwned>(stream: &'a mut T) -> anyhow::Result<M>
            where
                $($stream_trait_bound)*
            {
                let mut stream = TokioWebTransportCompat::<'a, T>::from(stream);
                let length_prefix = {
                    let mut buf: [u8; 4] = [0; 4];
                    maybe_sync_read_write!(read $($async)? stream, buf);
                    u32::from_be_bytes(buf)
                };
                let limit = 1024 * 1024;
                if length_prefix > limit {
                    return Err(anyhow!(
                        "A message was recieved of length: {length_prefix}b! The limit is {limit}b"
                    ));
                }
                let mut data = vec![0u8; length_prefix as usize];
                maybe_sync_read_write!(read $($async)? stream, data);
                Ok(serde_json::from_slice(&data)?)
            }
        };
        {___internal; ser, $($async:ident)?, $func_name:ident, {$($stream_trait_bound:tt)*}} => {
            pub $($async)? fn $func_name<'a, T: 'a>(msg: &impl serde::Serialize, stream: &'a mut T) -> anyhow::Result<()>
            where
                $($stream_trait_bound)*
            {
                let mut stream = TokioWebTransportCompat::<'a, T>::from(stream);
                let bytes = serde_json::to_vec(msg)?;
                let length_prefix: [u8; 4] = (bytes.len() as u32).to_be_bytes();
                maybe_sync_read_write!(write $($async)? stream, length_prefix);
                maybe_sync_read_write!(write $($async)? stream, bytes);
                Ok(())
            }
        };
    }
}

serialize_maybe_sync!(sync de read_from_stream_sync);
serialize_maybe_sync!(async de read_from_stream_async);
serialize_maybe_sync!(sync ser write_to_stream_sync);
serialize_maybe_sync!(async ser write_to_stream_async);
