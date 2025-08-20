use std::{collections::HashMap, time::Instant};

use bevy::prelude::*;
use wrts_messaging::{Client2Match, ClientSharedInfo, Match2Client, Message, SharedEntityId};

use crate::{
    AppState, Bullet, Detected, Health, MoveOrder, NoLongerDetected, PlayerSettings, Team,
    networking::{ClientInfo, ServerConnection, ThisClient},
    ship::Ship,
};

pub use shared_entity_tracking::SharedEntityTracking;

pub struct InMatchPlugin;

impl Plugin for InMatchPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SharedEntityTracking>()
            .add_systems(
                OnEnter(AppState::InMatch),
                (in_match_startup_networking.pipe(in_match_startup_networking_none_handler)),
            )
            .add_systems(
                OnExit(AppState::InMatch),
                clear_shared_entity_tracking_on_match_exit,
            )
            .add_systems(
                FixedUpdate,
                (in_match_networking.pipe(in_match_networking_none_handler))
                    .run_if(in_state(AppState::InMatch)),
            )
            .add_systems(Update, (|| {}).run_if(in_state(AppState::InMatch)));
    }
}

mod shared_entity_tracking {
    use std::{collections::HashMap, ops::Index};

    use bevy::prelude::*;
    use wrts_messaging::SharedEntityId;

    #[derive(Resource, Debug, Default)]
    pub struct SharedEntityTracking {
        entity2shared: HashMap<Entity, SharedEntityId>,
        shared2entity: HashMap<SharedEntityId, Entity>,
    }

    impl SharedEntityTracking {
        pub fn insert(&mut self, shared: SharedEntityId, local: Entity) {
            if let Some(old_shared) = self.entity2shared.insert(local, shared) {
                warn!("Inserted {old_shared:?} over {shared:?} for local entity: {local}");
            }
            if let Some(old_local) = self.shared2entity.insert(shared, local) {
                warn!("Inserted {old_local} over {local} for shared entity: {shared:?}");
            }
        }

        pub fn remove_by_shared(&mut self, shared: SharedEntityId) -> Option<Entity> {
            let Some(local) = self.shared2entity.remove(&shared) else {
                warn!("Tried removing by shared entity which doesn't exist: {shared:?}");
                return None;
            };
            self.entity2shared.remove(&local).expect("unreachable");
            Some(local)
        }

        pub fn remove_by_local(&mut self, local: Entity) -> Option<SharedEntityId> {
            let Some(shared) = self.entity2shared.remove(&local) else {
                warn!("Tried removing by local entity which doesn't exist: {local:?}");
                return None;
            };
            self.shared2entity.remove(&shared).expect("unreachable");
            Some(shared)
        }

        pub fn clear(&mut self) {
            self.entity2shared.clear();
            self.shared2entity.clear();
        }
    }

    impl Index<SharedEntityId> for SharedEntityTracking {
        type Output = Entity;

        fn index(&self, index: SharedEntityId) -> &Self::Output {
            &self.shared2entity[&index]
        }
    }

    impl Index<Entity> for SharedEntityTracking {
        type Output = SharedEntityId;

        fn index(&self, index: Entity) -> &Self::Output {
            &self.entity2shared[&index]
        }
    }
}

fn clear_shared_entity_tracking_on_match_exit(mut shared_entities: ResMut<SharedEntityTracking>) {
    shared_entities.clear();
}

fn in_match_startup_networking(
    mut commands: Commands,
    mut server: ResMut<ServerConnection>,
    settings: Res<PlayerSettings>,
) -> Option<()> {
    let Message::Match2Client(Match2Client::InitA { your_client }) = server.recv_blocking()? else {
        return None;
    };

    commands.insert_resource(ThisClient(your_client));

    server.send(Message::Client2Match(Client2Match::InitB {
        info: ClientSharedInfo {
            id: your_client,
            user: settings.username.clone(),
        },
    }))?;

    let Message::Match2Client(Match2Client::InitC { all_clients }) = server.recv_blocking()? else {
        return None;
    };

    assert!(
        all_clients.len() == 2,
        "Currently, there should always be two clients per game"
    );
    for info in all_clients {
        commands.spawn((
            StateScoped(AppState::InMatch),
            ClientInfo {
                id: info.id,
                user: info.user,
            },
        ));
    }

    Some(())
}

fn in_match_startup_networking_none_handler(
    In(input): In<Option<()>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if let None = input {
        info!("`in_match_startup_networking` disconnected");
        next_state.set(AppState::ConnectingToServer);
    }
}

#[deny(unused_variables)]
fn in_match_networking(
    mut commands: Commands,
    mut server: ResMut<ServerConnection>,
    mut shared_entities: ResMut<SharedEntityTracking>,
    mut transforms: Query<&mut Transform>,
) -> Option<()> {
    while let Ok(msg) = server.recv_next() {
        match msg {
            Message::Match2Client(Match2Client::PrintMsg(s)) => {
                info!("PrintMsg called: {s}");
            }
            Message::Match2Client(Match2Client::DestroyEntity(shared)) => {
                let Some(local) = shared_entities.remove_by_shared(shared) else {
                    continue;
                };
                commands.entity(local).despawn();
            }
            Message::Match2Client(Match2Client::SpawnShip {
                id,
                team,
                ship_base,
                health,
                pos,
                rot,
            }) => {
                let local = commands
                    .spawn((
                        StateScoped(AppState::InMatch),
                        Ship {
                            template: ship_base.to_template(),
                        },
                        Team(team),
                        Health(health),
                        Transform {
                            translation: pos.extend(0.),
                            rotation: rot,
                            ..default()
                        },
                    ))
                    .id();
                shared_entities.insert(id, local);
            }
            Message::Match2Client(Match2Client::SpawnBullet {
                id,
                team,
                owning_ship,
                damage,
                pos,
                rot,
            }) => {
                let local = commands
                    .spawn((
                        StateScoped(AppState::InMatch),
                        Bullet {
                            owning_ship: shared_entities[owning_ship],
                            damage,
                        },
                        Team(team),
                        Transform {
                            translation: pos,
                            rotation: rot,
                            ..default()
                        },
                    ))
                    .id();
                shared_entities.insert(id, local);
            }
            Message::Match2Client(Match2Client::SetEntityPos { id, pos }) => {
                let Ok(mut trans) = transforms.get_mut(shared_entities[id]) else {
                    continue;
                };
                trans.translation.x = pos.x;
                trans.translation.y = pos.y;
            }
            Message::Match2Client(Match2Client::SetMoveOrder { id, waypoints }) => {
                commands
                    .entity(shared_entities[id])
                    .insert(MoveOrder { waypoints });
            }
            Message::Match2Client(Match2Client::SetDetection {
                id,
                currently_detected,
                last_known_pos,
            }) => {
                let mut entity = commands.entity(shared_entities[id]);
                match currently_detected {
                    true => {
                        entity.insert(Detected);
                    }
                    false => {
                        entity.try_remove::<Detected>();
                    }
                }
                match last_known_pos {
                    Some((pos, rot)) => {
                        entity.insert(NoLongerDetected {
                            last_known: Transform {
                                translation: pos.extend(0.),
                                rotation: rot,
                                ..default()
                            },
                        });
                    }
                    None => {
                        entity.try_remove::<NoLongerDetected>();
                    }
                }
            }
            Message::Match2Client(Match2Client::InitA { .. })
            | Message::Match2Client(Match2Client::InitC { .. })
            | Message::Lobby2Client(_)
            | Message::Client2Lobby(_)
            | Message::Client2Match(_) => {
                error!("Unexpected message: {msg:?}");
            }
        }
    }

    if server.disconnected() {
        return None;
    }

    Some(())
}

fn in_match_networking_none_handler(
    In(input): In<Option<()>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if let None = input {
        info!("`in_match_networking` disconnected");
        next_state.set(AppState::ConnectingToServer);
    }
}

// fn send_move_order_updates(
//     mut server: ResMut<ServerConnection>,
//     shared_entities: Res<SharedEntityTracking>,
//     move_orders: Query<(Entity, &MoveOrder), Changed<MoveOrder>>,
// ) {
//     for (local, move_order) in move_orders {
//         let _ = server.send(Message::Client2Match(Client2Match::SetMoveOrder {
//             id: shared_entities[local],
//             waypoints: move_order.waypoints.clone(),
//         }));
//     }
// }
