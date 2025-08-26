use bevy::prelude::*;
use rand_distr::Distribution;
use wrts_match_shared::ship_template::{Dispersion, ShipTemplate, ShipTemplateId};

use crate::{Health, Team, Velocity};

const SHIP_SPEED_SCALE: f32 = 5.2;

#[derive(Debug, Clone)]
pub struct TurretState {
    pub dir: f32,
    pub reload_timer: Timer,
}

#[derive(Debug, Component, Clone)]
#[require(Team, Health, Transform, Velocity)]
pub struct Ship {
    pub template: &'static ShipTemplate,
    pub turret_states: Vec<TurretState>,
    pub curr_speed: f32,
    pub torpedo_reloads: Vec<Timer>,
    // pub torpedoes_reloaded: usize,
    // pub torpedo_reload_timer: Timer,
}

pub fn apply_dispersion(dispersion: &Dispersion, nominal_direction: Vec3) -> Vec3 {
    let dist = rand_distr::Normal::new(0., dispersion.sigma).unwrap();
    let mut rng = rand::rng();
    let h_squared = dispersion.horizontal * dispersion.horizontal;
    let v_squared = dispersion.vertical * dispersion.vertical;
    let ellipse_pos = loop {
        let x = dist.sample(&mut rng);
        let y = dist.sample(&mut rng);

        if x * x / h_squared + y * y / v_squared <= 1. {
            break vec2(x, y);
        }
    };

    let elevation = f32::atan2(ellipse_pos.y, 1000.);
    let elev_rot_axis = Vec3::cross(nominal_direction, Vec3::Z).normalize();
    let dir = Mat3::from_axis_angle(elev_rot_axis, elevation) * nominal_direction;

    let azimuth = f32::atan2(ellipse_pos.x, 1000.);
    Mat3::from_axis_angle(Vec3::Z, azimuth) * dir
}
