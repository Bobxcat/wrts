use std::time::Duration;

use bevy::prelude::*;
use wrts_messaging::{Client2Match, ClientSharedInfo, Match2Client, Message};

use crate::{
    AppState, Bullet, DetectionStatus, Health, MoveOrder, PlayerSettings, SmokePuff, Team, Torpedo,
    Velocity,
    networking::{ClientInfo, ServerConnection, ThisClient},
    ship::{
        self, DetectionIndicatorDisplay, Ship, ShipModifiersDisplay, ShipUI, ShipUITrackedShip,
        TurretState,
    },
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

        pub fn get_by_shared(&self, shared: SharedEntityId) -> Option<Entity> {
            self.shared2entity.get(&shared).copied()
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
    this_client: Res<ThisClient>,
) -> Option<()> {
    // Note: All network actions are queued instead of running of a query,
    // so that previous actions are flushed (i.e. creating a ship then updating that ship's position)

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
                turret_rots,
            }) => {
                let turret_states = {
                    let turret_instances = &ship_base.to_template().turret_instances;
                    let mut turret_states = vec![];
                    for turret_idx in 0..turret_instances.len() {
                        turret_states.push(TurretState {
                            dir: turret_rots[turret_idx],
                        });
                    }
                    turret_states
                };
                // Spawn the ship
                let local = commands
                    .spawn((
                        StateScoped(AppState::InMatch),
                        Ship {
                            template: ship_base.to_template(),
                            turret_states,
                            reloaded_torp_volleys: 0,
                            reloading_torp_volleys_remaining_time: vec![
                                Duration::ZERO;
                                ship_base
                                    .to_template()
                                    .torpedoes
                                    .as_ref()
                                    .map(|t| t.volleys)
                                    .unwrap_or(0)
                            ],
                        },
                        DetectionStatus::Never,
                        Team(team),
                        Health(health),
                        Transform {
                            translation: pos.extend(0.),
                            rotation: rot,
                            ..default()
                        },
                    ))
                    .id();

                // Spawn the ship ui tracking this ship
                commands
                    .spawn((
                        StateScoped(AppState::InMatch),
                        ShipUI,
                        ShipUITrackedShip(local),
                        Node {
                            position_type: PositionType::Absolute,
                            flex_direction: FlexDirection::Column,
                            ..default()
                        },
                        BackgroundColor(Color::linear_rgba(0.4, 0.4, 0.6, 0.6)),
                        BorderRadius::all(Val::Px(5.)),
                    ))
                    .with_children(|commands| {
                        commands.spawn((
                            crate::ship::ShipUIFirstRow,
                            Node {
                                border: UiRect::all(Val::Px(2.)),
                                flex_direction: FlexDirection::Row,
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::Center,
                                ..default()
                            },
                            children![
                                (
                                    //
                                    DetectionIndicatorDisplay,
                                    ShipUITrackedShip(local),
                                    ImageNode::default(),
                                ),
                                (
                                    //
                                    Text(ship_base.to_name().to_string()),
                                )
                            ],
                        ));

                        if Team(team).is_this_client(*this_client) {
                            commands.spawn((
                                ShipModifiersDisplay,
                                ShipUITrackedShip(local),
                                Node {
                                    border: UiRect::all(Val::Px(1.)),
                                    flex_direction: FlexDirection::Row,
                                    align_items: AlignItems::Center,
                                    justify_content: JustifyContent::Center,
                                    ..default()
                                },
                            ));
                        }
                    });

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
            Message::Match2Client(Match2Client::SpawnTorpedo {
                id,
                team,
                owning_ship,
                damage,
                pos,
                vel,
            }) => {
                let local = commands
                    .spawn((
                        StateScoped(AppState::InMatch),
                        Torpedo {
                            owning_ship: shared_entities[owning_ship],
                            damage,
                            speed: vel.length(),
                        },
                        DetectionStatus::Never,
                        Team(team),
                        Transform {
                            translation: pos.extend(0.),
                            rotation: Quat::from_rotation_z(vel.to_angle()),
                            ..default()
                        },
                    ))
                    .id();
                shared_entities.insert(id, local);
            }
            Message::Match2Client(Match2Client::SpawnSmokePuff { id, pos, radius }) => {
                let local = commands
                    .spawn((
                        StateScoped(AppState::InMatch),
                        SmokePuff { radius },
                        Transform {
                            translation: pos.extend(0.),
                            ..default()
                        },
                    ))
                    .id();
                shared_entities.insert(id, local);
            }
            Message::Match2Client(Match2Client::SetSmokeConsumableState { id, state }) => {
                commands.queue(move |world: &mut World| {
                    let Some(local) = world.resource::<SharedEntityTracking>().get_by_shared(id)
                    else {
                        return;
                    };

                    let mut entity = world.entity_mut(local);
                    let new_state = match state {
                        wrts_messaging::SmokeConsumableState::Deploying {
                            charges_unused,
                            action_time_remaining,
                        } => ship::SmokeConsumableState {
                            charges_unused,
                            action_state: ship::SmokeConsumableActionState::Deploying {
                                time_remaining: action_time_remaining,
                            },
                        },
                        wrts_messaging::SmokeConsumableState::Recharging {
                            charges_unused,
                            recharge_time_remaining,
                        } => ship::SmokeConsumableState {
                            charges_unused,
                            action_state: ship::SmokeConsumableActionState::Recharging {
                                time_remaining: recharge_time_remaining,
                            },
                        },
                        wrts_messaging::SmokeConsumableState::Recharged { charges_unused } => {
                            ship::SmokeConsumableState {
                                charges_unused,
                                action_state: ship::SmokeConsumableActionState::Recharged,
                            }
                        }
                    };
                    entity.insert(new_state);
                });
            }
            Message::Match2Client(Match2Client::SetReloadedTorps {
                id,
                ready_to_fire,
                still_reloading,
            }) => {
                commands.queue(move |world: &mut World| {
                    let Some(local) = world.resource::<SharedEntityTracking>().get_by_shared(id)
                    else {
                        return;
                    };
                    let mut entity = world.entity_mut(local);
                    let mut ship = entity.get_mut::<Ship>().unwrap();
                    ship.reloaded_torp_volleys = ready_to_fire;
                    ship.reloading_torp_volleys_remaining_time = still_reloading;
                });
            }
            Message::Match2Client(Match2Client::SetTrans { id, pos, rot }) => {
                commands.queue(move |world: &mut World| {
                    let Some(local) = world.resource::<SharedEntityTracking>().get_by_shared(id)
                    else {
                        return;
                    };
                    let mut entity = world.entity_mut(local);
                    let mut trans = entity.get_mut::<Transform>().unwrap();
                    trans.translation = pos;
                    trans.rotation = rot;
                });
            }
            Message::Match2Client(Match2Client::SetVelocity { id, vel }) => {
                commands.queue(move |world: &mut World| {
                    let Some(local) = world.resource::<SharedEntityTracking>().get_by_shared(id)
                    else {
                        return;
                    };
                    let mut entity = world.entity_mut(local);
                    entity.insert(Velocity(vel));
                    //
                });
            }
            Message::Match2Client(Match2Client::SetTurretDirs { id, turret_dirs }) => {
                commands.queue(move |world: &mut World| {
                    let Some(local) = world.resource::<SharedEntityTracking>().get_by_shared(id)
                    else {
                        return;
                    };

                    let mut entity = world.entity_mut(local);
                    let mut ship = entity.get_mut::<Ship>().unwrap();
                    for turret_idx in 0..turret_dirs.len() {
                        ship.turret_states[turret_idx].dir = turret_dirs[turret_idx];
                    }
                });
            }
            Message::Match2Client(Match2Client::SetHealth {
                id,
                health: new_health,
            }) => {
                commands.queue(move |world: &mut World| {
                    let Some(local) = world.resource::<SharedEntityTracking>().get_by_shared(id)
                    else {
                        return;
                    };
                    let mut entity = world.entity_mut(local);
                    let mut health = entity.get_mut::<Health>().unwrap();
                    health.0 = new_health;
                });
            }
            Message::Match2Client(Match2Client::SetMoveOrder { id, waypoints }) => {
                commands
                    .entity(shared_entities[id])
                    .insert(MoveOrder { waypoints });
            }
            Message::Match2Client(Match2Client::SetDetection {
                id,
                currently_detected,
            }) => {
                commands.queue(move |world: &mut World| {
                    let local = world.resource::<SharedEntityTracking>()[id];
                    let mut entity = world.entity_mut(local);
                    let mut det = entity.get_mut::<DetectionStatus>().unwrap();

                    *det = match (det.clone(), currently_detected) {
                        (_, true) => DetectionStatus::Detected,
                        (DetectionStatus::Never, false) => DetectionStatus::Never,
                        (DetectionStatus::Detected, false)
                        | (DetectionStatus::UnDetected, false) => DetectionStatus::UnDetected,
                    };
                });
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
