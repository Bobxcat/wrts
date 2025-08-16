use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use itertools::Itertools;
use wrts_messaging::ClientId;

use crate::{
    AppState,
    networking::{ClientInfo, ServerConnection},
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
                (update_lobby_clients_list).run_if(in_state(AppState::LobbyMenu)),
            );
    }
}

#[derive(Component, Debug, Clone, Copy)]
struct LobbyClientsList;

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
    clients: Query<(Entity, &ClientInfo)>,
    mut server: ResMut<ServerConnection>,
) -> Option<()> {
    let msgs = server.recv_all()?;

    if msgs.len() > 0 {
        info!("Messages received: {:?}", msgs);
    }

    Some(())
}

fn lobby_networking_none_handler(
    input: In<Option<()>>,
    mut commands: Commands,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if let None = *input {
        info!("`lobby_networking` disconnected");
        next_state.set(AppState::ConnectingToServer);
    }
}

fn update_lobby_clients_list(
    mut commands: Commands,
    lists: Query<(Entity, &Children), With<LobbyClientsList>>,
    clients: Query<(Entity, &ClientInfo)>,
) {
    let clients_by_id: HashMap<ClientId, Entity> =
        clients.into_iter().map(|(e, c)| (c.id, e)).collect();

    for (list, list_entries) in lists {
        let mut clients_to_be_added = clients_by_id.clone();
        for &entry in list_entries {
            // let entry_clientinfo = ;
            commands.entity(entry).despawn();
        }
        for (client_id, client) in clients_to_be_added.into_iter().sorted() {
            let (_, client_info) = clients.get(client).unwrap();
            let display = commands
                .spawn((
                    Node {
                        margin: UiRect::all(Val::Px(50.)),
                        ..default()
                    },
                    Text::new(format!("{:#?}", client_info)),
                ))
                .id();
            commands.entity(list).add_child(display);
        }
    }
}
