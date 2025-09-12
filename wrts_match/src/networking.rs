use bevy::prelude::*;
use itertools::Itertools;
use std::io::stdin;
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::time::Duration;
use std::{collections::HashMap, io::Write, ops::Deref};
use wrts_messaging::{Client2Match, Match2Client, Message, SharedEntityId, WrtsMatchMessage};

use wrts_messaging::{
    ClientId, ClientSharedInfo, RecvFromStream, WrtsMatchInitMessage, write_to_stream_sync,
};

use crate::detection::{BaseDetection, DetectionStatus};
pub use crate::networking::shared_entity_tracking::SharedEntityTracking;
use crate::ship::{Ship, SmokeConsumableState, SmokeDeploying, TurretStates};
use crate::{FireTarget, Health, MoveOrder, Team, Torpedo, Velocity};

pub struct NetworkingPlugin;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReadClientMessagesSystem;
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct UpdateClientsSystem;

impl Plugin for NetworkingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, network_handshake)
            .configure_sets(FixedUpdate, ReadClientMessagesSystem)
            .add_systems(
                FixedUpdate,
                (read_messages,).in_set(ReadClientMessagesSystem),
            )
            .configure_sets(FixedUpdate, UpdateClientsSystem)
            .add_systems(
                FixedUpdate,
                (
                    send_transform_updates,
                    send_velocity_updates,
                    send_turret_state_updates,
                    send_health_updates,
                    send_torpedo_reload_updates,
                    send_smoke_consumable_state_updates,
                )
                    .in_set(UpdateClientsSystem),
            );
    }
}

fn stdin_handler(tx: SyncSender<WrtsMatchMessage>) {
    let mut stdin = std::io::stdin().lock();
    loop {
        match WrtsMatchMessage::recv_sync(&mut stdin) {
            Ok(msg) => {
                info!("Receiving: {msg:?}");
                if let Err(_) = tx.send(msg) {
                    error!("lost connection to bevy, exiting");
                    return;
                }
            }
            Err(e) => {
                error!("Error receiving WrtsMatchMessage: `{e}`");
                error!("Exiting stdin_handler");
                return;
            }
        }
    }
}

fn stdout_handler(rx: Receiver<WrtsMatchMessage>) {
    let mut stdout = std::io::stdout().lock();
    loop {
        match rx.recv() {
            Ok(msg) => {
                match &msg.msg {
                    Message::Match2Client(Match2Client::SetTrans { .. })
                    | Message::Match2Client(Match2Client::SetTurretDirs { .. })
                    | Message::Match2Client(Match2Client::SetVelocity { .. })
                    | Message::Match2Client(Match2Client::SetSmokeConsumableState { .. })
                    | Message::Match2Client(Match2Client::SetReloadedTorps { .. }) => {
                        trace!("Sending: {msg:?}")
                    }
                    _ => info!("Sending: {msg:?}"),
                }

                if let Err(e) = write_to_stream_sync(&msg, &mut stdout) {
                    error!("Encountered error sending to stdout: `{:?}`", e)
                }
                let _ = stdout.flush();
            }
            Err(_) => {
                error!("lost connection to bevy, exiting");
                return;
            }
        }
    }
}

#[derive(Debug, Resource)]
pub struct MessagesSend(SyncSender<WrtsMatchMessage>);

impl MessagesSend {
    pub fn send(&self, msg: WrtsMatchMessage) {
        SyncSender::send(&self, msg)
            .expect("`MessagesSend` should never disconnect unless the bevy app has closed down")
    }
}

impl Deref for MessagesSend {
    type Target = SyncSender<WrtsMatchMessage>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Non-send resource
#[derive(Debug)]
pub struct MessagesRecv(Receiver<WrtsMatchMessage>);

impl Deref for MessagesRecv {
    type Target = Receiver<WrtsMatchMessage>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

mod shared_entity_tracking {
    use std::collections::HashMap;

    use bevy::ecs::{entity::Entity, resource::Resource};
    use slotmap::{KeyData, SlotMap};
    use wrts_messaging::SharedEntityId;

    slotmap::new_key_type! {
        struct InnerSharedId;
    }

    /// Keeps track of the mapping between
    #[derive(Resource, Debug, Default, Clone)]
    pub struct SharedEntityTracking {
        shared2local: SlotMap<InnerSharedId, Entity>,
        local2shared: HashMap<Entity, InnerSharedId>,
    }

    impl SharedEntityTracking {
        fn shared_to_inner(id: SharedEntityId) -> InnerSharedId {
            InnerSharedId(KeyData::from_ffi(id.0))
        }

        fn inner_to_shared(inner: InnerSharedId) -> SharedEntityId {
            SharedEntityId(inner.0.as_ffi())
        }

        pub fn insert(&mut self, local: Entity) -> SharedEntityId {
            let inner = self.shared2local.insert(local);
            self.local2shared.insert(local, inner);
            Self::inner_to_shared(inner)
        }

        pub fn remove_by_local(&mut self, local: Entity) -> Option<(SharedEntityId, Entity)> {
            let id = self.local2shared.remove(&local)?;
            self.shared2local.remove(id).expect("unreachable");
            Some((Self::inner_to_shared(id), local))
        }

        pub fn remove_by_shared(
            &mut self,
            shared: SharedEntityId,
        ) -> Option<(SharedEntityId, Entity)> {
            let e = self.shared2local.remove(Self::shared_to_inner(shared))?;
            self.local2shared.remove(&e).expect("unreachable");
            Some((shared, e))
        }

        pub fn get_by_shared(&self, shared: SharedEntityId) -> Option<Entity> {
            self.shared2local
                .get(Self::shared_to_inner(shared))
                .copied()
        }

        pub fn get_by_local(&self, local: Entity) -> Option<SharedEntityId> {
            self.local2shared
                .get(&local)
                .copied()
                .map(Self::inner_to_shared)
        }
    }
}

#[derive(Component, Debug, Clone)]
pub struct ClientInfo {
    pub info: ClientSharedInfo,
}

fn network_handshake(world: &mut World) {
    info!(
        "`WrtsMatchMessage` in-memory size: {}B",
        std::mem::size_of::<WrtsMatchMessage>()
    );
    let init_msg = WrtsMatchInitMessage::recv_sync(&mut stdin()).unwrap();

    let (handler_tx, msgs_rx) = mpsc::sync_channel::<WrtsMatchMessage>(128);
    let (msgs_tx, handler_rx) = mpsc::sync_channel::<WrtsMatchMessage>(128);

    std::thread::spawn(move || {
        stdin_handler(handler_tx);
    });
    std::thread::spawn(move || {
        stdout_handler(handler_rx);
    });

    let client_infos = {
        let mut infos = HashMap::new();
        for cl in init_msg.clients {
            let _ = msgs_tx.send(WrtsMatchMessage {
                client: cl,
                msg: Message::Match2Client(Match2Client::InitA { your_client: cl }),
            });
        }

        for _ in 0..init_msg.clients.len() {
            match msgs_rx.recv() {
                Ok(WrtsMatchMessage {
                    client: _,
                    msg: Message::Client2Match(Client2Match::InitB { info }),
                }) => {
                    infos.insert(info.id, info);
                }
                res => {
                    error!(
                        "Expected one `InitA` message per client assigned to this match! Instead, got: {res:?}"
                    );
                    world.send_event(AppExit::from_code(1)).unwrap();
                    return;
                }
            };
        }
        infos
    };

    for (_, cl_info) in client_infos.clone() {
        let _ = msgs_tx.send(WrtsMatchMessage {
            client: cl_info.id,
            msg: Message::Match2Client(Match2Client::InitC {
                all_clients: client_infos.values().cloned().collect(),
            }),
        });
        world.spawn(ClientInfo { info: cl_info });
    }

    world.insert_resource(MessagesSend(msgs_tx));
    world.init_resource::<SharedEntityTracking>();
    world.insert_non_send_resource(MessagesRecv(msgs_rx));
}

fn read_messages(
    mut commands: Commands,
    msgs_rx: NonSend<MessagesRecv>,
    msgs_tx: Res<MessagesSend>,
    shared_entities: Res<SharedEntityTracking>,
    mut exit: EventWriter<AppExit>,

    ships: Query<(&Ship, &Transform)>,
    teams: Query<&Team>,
) {
    loop {
        let WrtsMatchMessage {
            client: msg_sender,
            msg,
        } = match msgs_rx.try_recv() {
            Ok(msg) => msg,
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => {
                error!("Messaging disconnected, exiting");
                exit.write(AppExit::from_code(1));
                return;
            }
        };
        match msg {
            Message::Client2Match(Client2Match::Echo(s)) => {
                msgs_tx.send(WrtsMatchMessage {
                    client: msg_sender,
                    msg: Message::Match2Client(Match2Client::PrintMsg(s)),
                });
            }
            Message::Client2Match(Client2Match::SetMoveOrder { id, waypoints }) => {
                let Some(local) = shared_entities.get_by_shared(id) else {
                    warn!("Client {msg_sender} sent message with bad id: {id:?}");
                    continue;
                };
                if teams
                    .get(local)
                    .ok()
                    .and_then(|team| (team.0 == msg_sender).then_some(()))
                    .is_none()
                {
                    warn!(
                        "Client {msg_sender} tried to SetMoveOrder on an entity not owned by them"
                    );
                    continue;
                }
                commands.entity(local).insert(MoveOrder { waypoints });
            }
            Message::Client2Match(Client2Match::SetFireTarg { id, targ }) => {
                let Some(local) = shared_entities.get_by_shared(id) else {
                    warn!("Client {msg_sender} sent message with bad id: {id:?}");
                    continue;
                };
                if teams
                    .get(local)
                    .ok()
                    .and_then(|team| (team.0 == msg_sender).then_some(()))
                    .is_none()
                {
                    warn!(
                        "Client {msg_sender} tried to SetMoveOrder on an entity not owned by them"
                    );
                    continue;
                }
                match targ {
                    Some(targ) => {
                        let Some(targ_local) = shared_entities.get_by_shared(targ) else {
                            warn!("Client {msg_sender} sent message with bad id: {targ:?}");
                            continue;
                        };
                        if ships.contains(targ_local) {
                            commands
                                .entity(local)
                                .insert(FireTarget { ship: targ_local });
                        } else {
                            warn!(
                                "Client {msg_sender} tried to SetFireTarg at a bad target: {targ:?}"
                            );
                        }
                    }
                    None => {
                        commands.entity(local).try_remove::<FireTarget>();
                    }
                }
            }
            Message::Client2Match(Client2Match::LaunchTorpedoVolley { ship, dir }) => {
                commands.queue(LaunchTorpedoVolleyCommand {
                    msg_sender,
                    owning_ship_id: ship,
                    dir,
                });
            }
            Message::Client2Match(Client2Match::UseConsumableSmoke { ship }) => {
                commands.queue(UseConsumableSmokeCommand {
                    msg_sender,
                    ship_id: ship,
                });
            }
            Message::Client2Match(Client2Match::InitB { .. })
            | Message::Match2Client(_)
            | Message::Client2Lobby(_)
            | Message::Lobby2Client(_) => {
                error!("Received unexpected message: {msg:?}");
            }
        };
    }
}

struct LaunchTorpedoVolleyCommand {
    msg_sender: ClientId,
    owning_ship_id: SharedEntityId,
    dir: Vec2,
}

impl Command for LaunchTorpedoVolleyCommand {
    fn apply(self, world: &mut World) -> () {
        let msg_sender = self.msg_sender;
        let Some(owning_ship_local) = world
            .resource::<SharedEntityTracking>()
            .get_by_shared(self.owning_ship_id)
        else {
            warn!(
                "Client {msg_sender} sent message with bad ship id: {:?}",
                self.owning_ship_id
            );
            return;
        };
        if world
            .get::<Team>(owning_ship_local)
            .and_then(|team| (team.0 == msg_sender).then_some(()))
            .is_none()
        {
            warn!(
                "Client {msg_sender} tried to LaunchTorpedoVolley on an entity not owned by them"
            );
            return;
        }
        let Some((mut ship, ship_trans)) = world
            .query::<(&mut Ship, &Transform)>()
            .get_mut(world, owning_ship_local)
            .ok()
        else {
            warn!(
                "Client {msg_sender} tried to LaunchTorpedoVolley on a ship that doesn't exist anymore"
            );
            return;
        };
        let ship_dir = ship_trans.rotation.to_euler(EulerRot::ZYX).0;
        let Some(torpedoes) = ship.template.torpedoes.as_ref() else {
            warn!(
                "Client {msg_sender} tried to LaunchTorpedoVolley on a ship that doesn't have torpedoes"
            );
            return;
        };
        let Some((_volley_idx, volley_timer)) = ship
            .torpedo_reloads
            .iter_mut()
            .enumerate()
            .find(|(_idx, timer)| timer.finished())
        else {
            // Not reloaded
            return;
        };
        let can_fire = torpedoes
            .port_firing_angle
            .rotated_by(ship_dir)
            .contains(self.dir)
            || torpedoes
                .starboard_firing_angle()
                .rotated_by(ship_dir)
                .contains(self.dir);
        if !can_fire {
            return;
        }

        volley_timer.reset();
        let ship_pos = ship_trans.translation.truncate();

        for torp_idx in 0..torpedoes.torps_per_volley {
            let angle_offset = {
                let s = torpedoes.spread / (torpedoes.torps_per_volley - 1) as f32;
                (torp_idx as f32 - 0.5 * (torpedoes.torps_per_volley - 1) as f32) * s
            };
            let dir = self.dir.rotate(Vec2::from_angle(angle_offset));
            let vel = dir * torpedoes.speed.mps();
            let rot = Quat::from_rotation_z(vel.to_angle());
            let pos = ship_pos + dir * 50.;

            let entity = {
                world
                    .spawn((
                        Torpedo {
                            owning_ship: owning_ship_local,
                            damage: torpedoes.damage,
                            inital_pos: pos,
                            max_range: torpedoes.range,
                        },
                        Team(self.msg_sender),
                        Transform {
                            translation: pos.extend(0.),
                            rotation: rot,
                            ..default()
                        },
                        Velocity(vel.extend(0.)),
                        BaseDetection(2_000.),
                        DetectionStatus {
                            is_detected: false,
                            detection_increased_by_firing: Timer::new(
                                Duration::ZERO,
                                TimerMode::Once,
                            )
                            .tick(Duration::MAX)
                            .clone(),
                            detection_increased_by_firing_at_range: 0.,
                        },
                    ))
                    .id()
            };

            let shared_id = world.resource_mut::<SharedEntityTracking>().insert(entity);

            let mut clients = world.query::<&ClientInfo>();
            let msgs_tx = world.get_resource::<MessagesSend>().unwrap();

            for cl in clients.iter(world) {
                msgs_tx.send(WrtsMatchMessage {
                    client: cl.info.id,
                    msg: Message::Match2Client(Match2Client::SpawnTorpedo {
                        id: shared_id,
                        team: self.msg_sender,
                        owning_ship: self.owning_ship_id,
                        damage: torpedoes.damage,
                        pos,
                        vel,
                    }),
                });
            }
        }
    }
}

struct UseConsumableSmokeCommand {
    msg_sender: ClientId,
    ship_id: SharedEntityId,
}

impl Command for UseConsumableSmokeCommand {
    fn apply(self, world: &mut World) -> () {
        let Self {
            msg_sender,
            ship_id,
        } = self;
        let Some(ship_local) = world
            .resource::<SharedEntityTracking>()
            .get_by_shared(self.ship_id)
        else {
            warn!("Client {msg_sender} sent message with bad ship id: {ship_id:?}");
            return;
        };
        if world
            .get::<Team>(ship_local)
            .and_then(|team| (team.0 == msg_sender).then_some(()))
            .is_none()
        {
            warn!("Client {msg_sender} tried to UseConsumableSmoke on an entity not owned by them");
            return;
        }

        if let Some(_ship_smoke_deploying) = world.get::<SmokeDeploying>(ship_local) {
            return;
        }

        let Some((ship, mut ship_smoke_state)) = world
            .query::<(&Ship, &mut SmokeConsumableState)>()
            .get_mut(world, ship_local)
            .ok()
        else {
            warn!(
                "Client {msg_sender} tried to UseConsumableSmoke on a ship that doesn't exist anymore or doesn't have smoke"
            );
            return;
        };

        if ship_smoke_state.charges_unused.unwrap_or(usize::MAX) == 0 {
            return;
        }

        if ship_smoke_state.cooldown_timer.finished() {
            if let Some(charges_unused) = &mut ship_smoke_state.charges_unused {
                *charges_unused -= 1;
            }

            let smoke = ship.template.consumables.smoke().unwrap();
            ship_smoke_state.cooldown_timer.reset();
            world.entity_mut(ship_local).insert(SmokeDeploying {
                action_timer: Timer::new(smoke.action_time, TimerMode::Once),
                puff_timer: Timer::new(Duration::from_secs(2), TimerMode::Repeating),
            });
        }
    }
}

fn send_transform_updates(
    transforms: Query<(Entity, &Transform, Option<(&DetectionStatus, &Team)>), Changed<Transform>>,
    clients: Query<&ClientInfo>,
    msgs_tx: Res<MessagesSend>,
    shared_entities: Res<SharedEntityTracking>,
) {
    let clients = clients.iter().map(|cl| cl.info.id).collect_vec();
    for (local, trans, detection) in transforms {
        let clients_to_update: Vec<ClientId>;
        if let Some((detection, team)) = detection
            && !detection.is_detected
        {
            clients_to_update = vec![team.0];
        } else {
            clients_to_update = clients.clone();
        }
        let Some(shared) = shared_entities.get_by_local(local) else {
            continue;
        };
        for cl in clients_to_update {
            msgs_tx.send(WrtsMatchMessage {
                client: cl,
                msg: Message::Match2Client(Match2Client::SetTrans {
                    id: shared,
                    pos: trans.translation,
                    rot: trans.rotation,
                }),
            });
        }
    }
}

fn send_velocity_updates(
    transforms: Query<(Entity, &Velocity, Option<(&DetectionStatus, &Team)>), Changed<Transform>>,
    clients: Query<&ClientInfo>,
    msgs_tx: Res<MessagesSend>,
    shared_entities: Res<SharedEntityTracking>,
) {
    let clients = clients.iter().map(|cl| cl.info.id).collect_vec();
    for (local, vel, detection) in transforms {
        let clients_to_update: Vec<ClientId>;
        if let Some((detection, team)) = detection
            && !detection.is_detected
        {
            clients_to_update = vec![team.0];
        } else {
            clients_to_update = clients.clone();
        }
        let Some(shared) = shared_entities.get_by_local(local) else {
            continue;
        };
        for cl in clients_to_update {
            msgs_tx.send(WrtsMatchMessage {
                client: cl,
                msg: Message::Match2Client(Match2Client::SetVelocity {
                    id: shared,
                    vel: vel.0.truncate(),
                }),
            });
        }
    }
}

fn send_turret_state_updates(
    ships: Query<(Entity, &TurretStates)>,
    clients: Query<&ClientInfo>,
    msgs_tx: Res<MessagesSend>,
    shared_entities: Res<SharedEntityTracking>,
) {
    let clients = clients.iter().map(|cl| cl.info.id).collect_vec();
    for (local, turret_states) in ships {
        let Some(shared) = shared_entities.get_by_local(local) else {
            continue;
        };
        for cl in clients.clone() {
            msgs_tx.send(WrtsMatchMessage {
                client: cl,
                msg: Message::Match2Client(Match2Client::SetTurretDirs {
                    id: shared,
                    turret_dirs: turret_states
                        .states
                        .iter()
                        .map(|state| state.dir)
                        .collect_vec(),
                }),
            })
        }
    }
}

fn send_health_updates(
    healths: Query<(Entity, &Health), Changed<Health>>,
    clients: Query<&ClientInfo>,
    msgs_tx: Res<MessagesSend>,
    shared_entities: Res<SharedEntityTracking>,
) {
    let clients = clients.iter().map(|cl| cl.info.id).collect_vec();
    for (local, health) in healths {
        let Some(shared) = shared_entities.get_by_local(local) else {
            continue;
        };
        for cl in clients.clone() {
            msgs_tx.send(WrtsMatchMessage {
                client: cl,
                msg: Message::Match2Client(Match2Client::SetHealth {
                    id: shared,
                    health: health.0,
                }),
            })
        }
    }
}

fn send_torpedo_reload_updates(
    ships: Query<(Entity, &Ship, &Team)>,
    msgs_tx: Res<MessagesSend>,
    shared_entities: Res<SharedEntityTracking>,
) {
    for (local, ship, ship_team) in ships {
        let Some(shared) = shared_entities.get_by_local(local) else {
            continue;
        };

        if ship.template.torpedoes.is_none() {
            continue;
        }

        let timers = &ship.torpedo_reloads;
        let ready_to_fire = timers.iter().filter(|timer| timer.finished()).count();
        let still_reloading = timers
            .iter()
            .filter_map(|timer| (!timer.finished()).then_some(timer.remaining()))
            .sorted()
            .collect_vec();

        assert_eq!(ready_to_fire + still_reloading.len(), timers.len());

        msgs_tx.send(WrtsMatchMessage {
            client: ship_team.0,
            msg: Message::Match2Client(Match2Client::SetReloadedTorps {
                id: shared,
                ready_to_fire,
                still_reloading: still_reloading.clone(),
            }),
        })
    }
}

fn send_smoke_consumable_state_updates(
    smokers: Query<(Entity, &SmokeConsumableState, Option<&SmokeDeploying>)>,
    clients: Query<&ClientInfo>,
    msgs_tx: Res<MessagesSend>,
    shared_entities: Res<SharedEntityTracking>,
) {
    for (local, smoke_state, smoke_deploying) in smokers {
        let Some(shared) = shared_entities.get_by_local(local) else {
            continue;
        };

        let charges_unused = smoke_state.charges_unused.map(|x| x as u16);

        let state = if let Some(smoke_deploying) = smoke_deploying {
            wrts_messaging::SmokeConsumableState::Deploying {
                charges_unused,
                action_time_remaining: smoke_deploying.action_timer.remaining(),
            }
        } else {
            if smoke_state.cooldown_timer.finished() {
                wrts_messaging::SmokeConsumableState::Recharged { charges_unused }
            } else {
                wrts_messaging::SmokeConsumableState::Recharging {
                    charges_unused,
                    recharge_time_remaining: smoke_state.cooldown_timer.remaining(),
                }
            }
        };

        for client in clients {
            msgs_tx.send(WrtsMatchMessage {
                client: client.info.id,
                msg: Message::Match2Client(Match2Client::SetSmokeConsumableState {
                    id: shared,
                    state,
                }),
            })
        }
    }
}
