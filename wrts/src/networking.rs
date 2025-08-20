use std::{net::SocketAddr, str::FromStr};

use crate::{AppState, PlayerSettings};
use anyhow::{Result, anyhow};
use bevy::{log::tracing::Instrument, prelude::*};
use tokio::sync::mpsc;
use wrts_messaging::{Client2Lobby, ClientId, Lobby2Client, Message, RecvFromStream, SendToStream};
use wtransport::{ClientConfig, Endpoint};

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThisClient(pub ClientId);

/// Note that all `ClientInfo`s are cleared when leaving [AppState::LobbyMenu] or [AppState::InMatch]
#[derive(Component, Debug, Clone)]
pub struct ClientInfo {
    pub id: ClientId,
    pub user: String,
}

pub enum RecvNextErr {
    Disconnected,
    Empty,
}

#[derive(Resource, Debug)]
pub struct ServerConnection {
    this_client: ClientId,
    tx: mpsc::Sender<Message>,
    rx: mpsc::Receiver<Message>,
    disconnection: mpsc::Receiver<()>,
    disconnection_triggered: bool,
}

impl ServerConnection {
    pub fn this_client(&self) -> ClientId {
        self.this_client
    }

    pub fn disconnected(&mut self) -> bool {
        if self.disconnection_triggered {
            return true;
        }
        match self.disconnection.try_recv() {
            Ok(()) | Err(mpsc::error::TryRecvError::Disconnected) => {
                self.disconnection_triggered = true
            }
            Err(mpsc::error::TryRecvError::Empty) => (),
        }
        self.disconnection_triggered
    }

    /// `None` means the server is disconnected
    #[must_use]
    pub fn send(&mut self, msg: Message) -> Option<()> {
        if self.disconnected() {
            return None;
        }
        let res = self.tx.blocking_send(msg).ok();
        if let None = res {
            self.disconnection_triggered = true
        }
        res
    }

    #[must_use]
    pub fn recv_next(&mut self) -> Result<Message, RecvNextErr> {
        if self.disconnected() {
            return Err(RecvNextErr::Disconnected);
        }
        match self.rx.try_recv() {
            Ok(msg) => Ok(msg),
            Err(mpsc::error::TryRecvError::Empty) => return Err(RecvNextErr::Empty),
            Err(mpsc::error::TryRecvError::Disconnected) => {
                self.disconnection_triggered = true;
                return Err(RecvNextErr::Disconnected);
            }
        }
    }

    /// `None` means the server is disconnected
    #[must_use]
    pub fn recv_blocking(&mut self) -> Option<Message> {
        if self.disconnected() {
            return None;
        }
        let res = self.rx.blocking_recv();
        if res.is_none() {
            self.disconnection_triggered = true;
        }
        res
    }
}

pub struct NetworkingPlugin;

impl Plugin for NetworkingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            OnEnter(AppState::ConnectingToServer),
            (setup_connecting_to_network_ui, clear_server_connection),
        )
        .add_systems(
            Update,
            (update_join_server_button, update_join_server_state_display)
                .run_if(in_state(AppState::ConnectingToServer)),
        );
    }
}

fn clear_server_connection(mut commands: Commands) {
    commands.remove_resource::<ServerConnection>();
}

#[derive(Component, Debug, Clone, Copy)]
struct IPAddressField;

#[derive(Component, Debug, Clone, Copy)]
struct JoinServerButton;

#[derive(Component, Debug, Clone, Copy)]
struct JoinStateDisplay;

fn setup_connecting_to_network_ui(mut commands: Commands) {
    let text_color = Color::linear_rgb(0.2, 0.4, 0.4);

    commands.spawn((
        StateScoped(AppState::ConnectingToServer),
        Node {
            display: Display::Flex,
            width: Val::Percent(100.),
            height: Val::Percent(100.),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        children![(
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                ..default()
            },
            children![
                (
                    JoinServerButton,
                    Node {
                        margin: UiRect::all(Val::Px(50.0)),
                        ..default()
                    },
                    Text::new("Join Server!"),
                    TextFont {
                        font_size: 67.0,
                        ..default()
                    },
                    TextColor(text_color),
                    ImageNode::solid_color(Color::WHITE),
                    Button,
                ),
                (
                    IPAddressField,
                    Node {
                        margin: UiRect::all(Val::Px(50.0)),
                        ..default()
                    },
                    Text::new(format!("127.0.0.1:{}", wrts_messaging::DEFAULT_PORT)),
                    TextFont {
                        font_size: 67.0,
                        ..default()
                    },
                    TextColor(text_color),
                    ImageNode::solid_color(Color::WHITE),
                ),
            ]
        ),],
    ));
}

fn update_join_server_button(
    mut commands: Commands,
    button: Query<&Interaction, (With<JoinServerButton>, Changed<Interaction>)>,
    ip_address: Query<&Text, With<IPAddressField>>,
    settings: Res<PlayerSettings>,
    mut next_app_state: ResMut<NextState<AppState>>,
) {
    let Ok(&button) = button.single() else {
        assert!(button.is_empty());
        return;
    };
    let ip_address = ip_address.single().unwrap();
    match button {
        Interaction::Pressed => {
            info!("JoinServerButton pressed, starting handshake");
            let Ok(ip) = SocketAddr::from_str(ip_address.as_str()) else {
                return;
            };
            let (to_bevy, mut rx) = mpsc::channel(4096);
            let (tx, from_bevy) = mpsc::channel(1024);
            let (network_failure, recv_network_failure) = mpsc::channel(64);
            let info = NetworkStartInfo {
                ip,
                to_bevy,
                from_bevy,
                network_failure,
            };
            network_start(info);

            let Message::Lobby2Client(Lobby2Client::InitA {
                client_id: this_client,
            }) = rx.blocking_recv().unwrap()
            else {
                todo!()
            };

            commands.insert_resource(ThisClient(this_client));

            info!("Client ID assigned: {this_client}");

            tx.blocking_send(Message::Client2Lobby(Client2Lobby::InitB {
                username: settings.username.clone(),
            }))
            .unwrap();

            commands.insert_resource(ServerConnection {
                this_client,
                tx,
                rx,
                disconnection: recv_network_failure,
                disconnection_triggered: false,
            });

            next_app_state.set(AppState::LobbyMenu);
            info!("Server handshake finished, changing state");
        }
        _ => (),
    }
}

fn update_join_server_state_display(mut display: Query<&mut Text, With<JoinStateDisplay>>) {
    // let _display = display.single_mut().unwrap();
    //
}

struct NetworkStartInfo {
    ip: SocketAddr,
    to_bevy: mpsc::Sender<Message>,
    from_bevy: mpsc::Receiver<Message>,
    network_failure: mpsc::Sender<()>,
}

fn network_start(info: NetworkStartInfo) {
    std::thread::spawn(move || {
        let exit = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .unwrap()
            .block_on(network_start_async(info).instrument(info_span!("network_start")));
        match exit {
            Ok(()) => info!("Networking exited successfully"),
            Err(err) => error!("Networking exited with error: `{err}`"),
        }
    });
}

async fn network_start_async(
    NetworkStartInfo {
        ip,
        to_bevy,
        mut from_bevy,
        network_failure,
    }: NetworkStartInfo,
) -> Result<()> {
    let config = ClientConfig::builder()
        .with_bind_default()
        .with_no_cert_validation()
        .build();

    let addr = format!("https://{ip}");
    info!("Connecting at: {addr}");
    let connection = Endpoint::client(config)?.connect(addr).await?;
    let (mut to_server, mut from_server) = connection.open_bi().await?.await?;

    let network_failure_handle = network_failure.clone();
    let mut handles = vec![];
    handles.push(tokio::spawn(
        async move {
            loop {
                let Some(msg) = from_bevy.recv().await else {
                    error!("EXIT: bevy closed");
                    return;
                };
                if let Err(err) = msg.send(&mut to_server).await {
                    error!("EXIT: {err}");
                    let _ = network_failure_handle.send(()).await;
                    return;
                }
            }
        }
        .instrument(info_span!("bevy2server")),
    ));

    handles.push(tokio::spawn(
        async move {
            loop {
                let cleanup = async {
                    to_bevy.closed().await;
                    network_failure.closed().await;
                };
                let msg = match Message::recv(&mut from_server).await {
                    Ok(msg) => msg,
                    Err(err) => {
                        error!("EXIT: {err}");
                        let _ = network_failure.send(()).await;
                        cleanup.await;
                        return;
                    }
                };
                if let Err(_) = to_bevy.send(msg).await {
                    error!("EXIT: bevy closed");
                    cleanup.await;
                    return;
                }
            }
        }
        .instrument(info_span!("server2bevy")),
    ));

    for handle in handles {
        let _res = handle.await?;
    }

    Ok(())
}
