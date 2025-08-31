use bevy::prelude::*;
use wrts_messaging::{Match2Client, Message, WrtsMatchMessage};

use crate::{
    Team, math_utils,
    networking::{ClientInfo, MessagesSend, SharedEntityTracking},
    ship::{Ship, SmokePuff},
};

const MIN_DETECTION: f32 = 2_000.;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct DetectionSystems;

pub struct DetectionPlugin;

impl Plugin for DetectionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, update_detection.in_set(DetectionSystems));
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub struct BaseDetection(pub f32);

#[derive(Component, Debug, Clone, Copy)]
pub struct CanDetect;

#[derive(Component, Debug, Clone)]
pub struct DetectionStatus {
    pub is_detected: bool,
    pub detection_increased_by_firing: Timer,
    pub detection_increased_by_firing_at_range: f32,
}

fn detector_detects_detectee(
    detector_pos: Vec2,

    pos: Vec2,
    base_detection: f32,
    base_detection_when_firing_through_smoke: f32,
    detection_increased_by_firing: Option<f32>,
    smoke_puffs: Query<(&SmokePuff, &Transform)>,
) -> bool {
    let blocked_by_smoke = math_utils::cast_line_segment(
        detector_pos,
        pos,
        smoke_puffs
            .iter()
            .map(|(puff, puff_trans)| math_utils::Circle {
                pos: puff_trans.translation.truncate(),
                radius: puff.radius,
            }),
    )
    .is_some();

    let mut detection = base_detection;
    if let Some(firing_range) = detection_increased_by_firing {
        if blocked_by_smoke {
            detection = base_detection_when_firing_through_smoke;
        } else {
            detection = firing_range;
        }
    } else if blocked_by_smoke {
        // FIXME? Only block vision through smoke for ships
        detection = MIN_DETECTION;
    }

    detection = detection.max(MIN_DETECTION);

    detector_pos.distance(pos) <= detection
}

fn update_detection(
    detectors: Query<(&Team, &Transform), With<CanDetect>>,
    detectees: Query<(
        Entity,
        &Team,
        &Transform,
        &BaseDetection,
        &mut DetectionStatus,
        Option<&Ship>,
    )>,
    smoke_puffs: Query<(&SmokePuff, &Transform)>,
    clients: Query<&ClientInfo>,
    shared_entities: Res<SharedEntityTracking>,
    msgs_tx: Res<MessagesSend>,
    time: Res<Time>,
) {
    for (
        detectee,
        detectee_team,
        detectee_trans,
        base_detection,
        mut detectee_status,
        detectee_is_ship,
    ) in detectees
    {
        let old_detectee_status = detectee_status.clone();

        let detection_increased_by_firing = detectee_is_ship.is_some_and(|_| {
            !detectee_status
                .detection_increased_by_firing
                .tick(time.delta())
                .finished()
        });

        let base_detection_when_firing_through_smoke = detectee_is_ship
            .map(|ship| ship.template.detection_when_firing_through_smoke)
            .unwrap_or(f32::MAX);

        detectee_status.is_detected = detectors.iter().any(|(detector_team, detector_trans)| {
            if detector_team == detectee_team {
                return false;
            }
            detector_detects_detectee(
                detector_trans.translation.truncate(),
                detectee_trans.translation.truncate(),
                base_detection.0,
                base_detection_when_firing_through_smoke,
                detection_increased_by_firing
                    .then_some(detectee_status.detection_increased_by_firing_at_range),
                smoke_puffs,
            )
        });

        if !detectee_status.is_detected {
            detectee_status.detection_increased_by_firing =
                Timer::from_seconds(0., TimerMode::Once);
        }

        if old_detectee_status.is_detected != detectee_status.is_detected {
            if let Some(shared) = shared_entities.get_by_local(detectee) {
                for cl in clients {
                    msgs_tx.send(WrtsMatchMessage {
                        client: cl.info.id,
                        msg: Message::Match2Client(Match2Client::SetDetection {
                            id: shared,
                            currently_detected: detectee_status.is_detected,
                        }),
                    });
                }
            }
        }
    }
}
