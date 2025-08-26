use std::time::Duration;

use bevy::prelude::*;
use rand::Fill;
use rand_distr::Distribution;
use wrts_match_shared::ship_template::ShipTemplate;

use crate::{Health, Team};

const SHIP_SPEED_SCALE: f32 = 5.2;

#[derive(Debug)]
pub struct TurretState {
    /// Relative to ship-space
    pub dir: f32,
}

#[derive(Debug, Component)]
#[require(Health, Sprite, Transform)]
pub struct Ship {
    pub template: &'static ShipTemplate,
    pub turret_states: Vec<TurretState>,
    pub reloaded_torp_volleys: usize,
    pub reloading_torp_volleys: Vec<Duration>,
}
