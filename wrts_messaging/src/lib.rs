use std::{
    fmt::Display,
    io::{self, Read, Write},
};

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
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
    InitialInformationResponse { username: String },
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
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Message {
    Client2Match(Client2Match),
    Match2Client(Match2Client),
    Client2Lobby(Client2Lobby),
    Lobby2Client(Lobby2Client),
}

impl Message {
    pub async fn send(&self, stream: &mut SendStream) -> Result<()> {
        let bytes = serde_json::to_vec(self)?;
        let length_prefix: [u8; 4] = (bytes.len() as u32).to_be_bytes();
        stream.write_all(&length_prefix).await?;
        stream.write_all(&bytes).await?;
        Ok(())
    }

    pub async fn recv(stream: &mut RecvStream) -> Result<Self> {
        let length_prefix = {
            let mut buf: [u8; 4] = [0u8; std::mem::size_of::<u32>()];
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
}
