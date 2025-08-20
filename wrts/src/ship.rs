use bevy::prelude::*;
use rand::Fill;
use rand_distr::Distribution;
use wrts_match_shared::ship_template::ShipTemplate;

use crate::{Health, Team, Velocity};

const SHIP_SPEED_SCALE: f32 = 5.2;

#[derive(Debug, Component)]
#[require(Health, Sprite, Transform, Velocity)]
pub struct Ship {
    pub template: &'static ShipTemplate,
}
