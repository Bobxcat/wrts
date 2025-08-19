use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use itertools::Itertools;
use wrts_messaging::{Client2Lobby, ClientId, Lobby2Client, Message};

use crate::{
    AppState,
    networking::{LobbyClientInfo, RecvNextErr, ServerConnection},
};

pub struct LobbyUiPlugin;

impl Plugin for LobbyUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::LobbyMenu), (setup_lobby_ui))
            .add_systems(
                FixedUpdate,
                (lobby_networking.pipe(lobby_networking_none_handler))
                    .run_if(in_state(AppState::LobbyMenu)),
            )
            .add_systems(
                Update,
                (update_lobby_clients_list,).run_if(in_state(AppState::LobbyMenu)),
            );
    }
}

#[derive(Component, Debug, Clone, Copy)]
struct LobbyClientsList;

#[derive(Component, Debug, Clone, Copy)]
struct LobbyClientsListEntry {
    tracking_client: ClientId,
}

pub fn setup_lobby_ui(mut commands: Commands) {
    commands.spawn((
        StateScoped(AppState::LobbyMenu),
        Node {
            width: Val::Percent(100.),
            height: Val::Percent(100.),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        children![(
            LobbyClientsList,
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::BLACK),
        )],
    ));
}

fn lobby_networking(
    mut commands: Commands,
    clients: Query<(Entity, &LobbyClientInfo)>,
    mut server: ResMut<ServerConnection>,
    mut has_readied: Local<bool>,
    mut next_state: ResMut<NextState<AppState>>,
) -> Option<()> {
    let mut clients_by_id: HashMap<ClientId, Entity> =
        clients.into_iter().map(|(e, c)| (c.id, e)).collect();

    if !*has_readied {
        server.send(Message::Client2Lobby(Client2Lobby::SetReadyForMatch {
            is_ready: true,
        }))?;
        *has_readied = true;
    }

    loop {
        let msg = match server.recv_next() {
            Ok(x) => x,
            Err(RecvNextErr::Empty) => return Some(()),
            Err(RecvNextErr::Disconnected) => return None,
        };

        let Message::Lobby2Client(msg) = msg else {
            error!("Received non-lobby2client message: {msg:?}");
            return None;
        };

        match msg {
            Lobby2Client::ClientJoined {
                client_id,
                username,
            } => {
                let e = commands
                    .spawn((
                        StateScoped(AppState::LobbyMenu),
                        LobbyClientInfo {
                            id: client_id,
                            user: username,
                        },
                    ))
                    .id();
                clients_by_id.insert(client_id, e);
            }
            Lobby2Client::ClientLeft { client_id } => {
                if let Some(e) = clients_by_id.remove(&client_id) {
                    commands.entity(e).despawn();
                } else {
                    error!(
                        "Received `ClientLeft` message without matching client to remove! {client_id}"
                    );
                    continue;
                };
            }
            Lobby2Client::MatchJoined {} => {
                next_state.set(AppState::InMatch);
                return Some(());
            }
            Lobby2Client::InitialInformation { .. } => {
                error!("Unexpected message: {msg:?}");
                return None;
            }
        }
    }
}

fn lobby_networking_none_handler(
    In(input): In<Option<()>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if let None = input {
        info!("`lobby_networking` disconnected");
        next_state.set(AppState::ConnectingToServer);
    }
}

fn update_lobby_clients_list(
    mut commands: Commands,
    lists: Query<(Entity, Option<&Children>), With<LobbyClientsList>>,
    list_entries: Query<(Entity, &LobbyClientsListEntry)>,
    clients_changed: Query<(Entity, &LobbyClientInfo), Changed<LobbyClientInfo>>,
    clients_all: Query<(Entity, &LobbyClientInfo)>,
) {
    let clients_changed_by_id: HashMap<ClientId, Entity> = clients_changed
        .into_iter()
        .map(|(e, c)| (c.id, e))
        .collect();

    let clients_all_by_id: HashMap<ClientId, Entity> =
        clients_all.into_iter().map(|(e, c)| (c.id, e)).collect();

    let spawn_entry_display =
        |mut commands: Commands, list: Entity, client_info: &LobbyClientInfo| {
            let disp = commands
                .spawn((
                    LobbyClientsListEntry {
                        tracking_client: client_info.id,
                    },
                    Node {
                        margin: UiRect::all(Val::Px(10.)),
                        ..default()
                    },
                    Text::new(&format!("Client: {client_info:?}")),
                ))
                .id();
            commands.entity(list).add_child(disp);
        };

    for (list, this_list_entries) in lists {
        let mut clients_displayed: HashSet<ClientId> = HashSet::new();
        for &entry in this_list_entries
            .map(|c| c.into_iter().collect_vec())
            .unwrap_or_default()
        {
            let (_, entry_data) = list_entries.get(entry).unwrap();
            let entry_client = entry_data.tracking_client;

            if let Some(&client) = clients_changed_by_id.get(&entry_client) {
                // If this entry tracks a client that needs an updated display
                let (_, client_info) = clients_changed.get(client).unwrap();

                spawn_entry_display(commands.reborrow(), list, client_info);
                clients_displayed.insert(entry_client);

                commands.entity(entry).despawn();
            } else if !clients_all_by_id.contains_key(&entry_client) {
                // If this entry tracks a client that no longer exists
                commands.entity(entry).despawn();
            } else {
                // If this entry tracks a client that exists and has an up-to-date display
                clients_displayed.insert(entry_client);
            }
        }

        for (_, client_info) in clients_all
            .into_iter()
            .filter(|(_, cl_info)| !clients_displayed.contains(&cl_info.id))
        {
            // If this client has no corresponding entry in this list
            spawn_entry_display(commands.reborrow(), list, client_info);
        }
    }
}
