use std::{collections::HashMap, sync::LazyLock};

use tokio::sync::{Mutex, broadcast};
use wrts_messaging::{ClientId, ClientSharedInfo};

/// When a `ClientsEvent` is received,
/// the listed changes must reflect the `Clients` data
/// by the time a lock can be made
///
/// Similarly, any changes to the `Clients` data
/// must correspond with a `ClientsEvent` when appropriate
#[derive(Debug, Clone)]
pub enum ClientsEvent {
    /// A client has connected
    ClientJoined { id: ClientId },
    /// A client has disconnected
    ClientLeft { id: ClientId },
}

/// Includes both the immutable and shared `ClientInfo`
/// and mutable data about the client, for which the lobby server
/// is the authoritative source
#[derive(Debug, Clone)]
pub struct ClientData {
    pub info: ClientSharedInfo,
}

pub struct Clients {
    pub id2info: HashMap<ClientId, ClientData>,
    events: broadcast::Sender<ClientsEvent>,
}

impl Clients {
    pub async fn lock() -> tokio::sync::MutexGuard<'static, Self> {
        static INSTANCE: LazyLock<Mutex<Clients>> = LazyLock::new(|| Mutex::new(Clients::new()));
        INSTANCE.lock().await
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ClientsEvent> {
        self.events.subscribe()
    }

    pub fn send(&mut self, event: ClientsEvent) {
        let _ = self.events.send(event);
    }

    fn new() -> Self {
        Self {
            id2info: HashMap::new(),
            events: broadcast::channel(1024).0,
        }
    }
}
