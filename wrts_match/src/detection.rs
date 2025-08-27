use bevy::prelude::*;
use ordered_float::OrderedFloat;
use wrts_messaging::{Match2Client, Message, WrtsMatchMessage};

use crate::{
    Team,
    networking::{ClientInfo, MessagesSend, SharedEntityTracking},
    ship::Ship,
};

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
}

fn update_detection(
    mut commands: Commands,
    detectors: Query<(&Team, &Transform), With<CanDetect>>,
    detectees: Query<(
        Entity,
        &Team,
        &Transform,
        &BaseDetection,
        &mut DetectionStatus,
        Option<&Ship>,
    )>,
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
        let detection = {
            let mut det = base_detection.0;
            if let Some(ship) = detectee_is_ship {
                if !detectee_status
                    .detection_increased_by_firing
                    .tick(time.delta())
                    .finished()
                {
                    det = ship
                        .template
                        .turret_templates
                        .values()
                        .max_by_key(|t| OrderedFloat(t.max_range))
                        .unwrap()
                        .max_range;
                }
            }
            det
        };

        detectee_status.is_detected = detectors.iter().any(|(detector_team, detector_trans)| {
            if detector_team == detectee_team {
                return false;
            }
            detector_trans
                .translation
                .distance(detectee_trans.translation)
                <= detection
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
