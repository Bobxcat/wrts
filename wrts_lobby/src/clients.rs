use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
};

use tokio::sync::{Mutex, broadcast};
use wrts_messaging::ClientId;

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

#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub id: ClientId,
    pub user: String,
}

pub struct Clients {
    pub id2info: HashMap<ClientId, ClientInfo>,
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
