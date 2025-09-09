use glam::*;

use crate::ship_template::ShipTemplateId;

/// Returns whether or not `v` is within the sweep from `from` to `to`,
/// rotating clockwise
///
/// https://stackoverflow.com/questions/43383154/how-to-check-if-a-vector-is-within-two-vectors
pub fn vector_is_within_swept_angle(v: Vec2, from: Vec2, to: Vec2) -> bool {
    if Vec2::perp_dot(from, to) >= 0. {
        Vec2::perp_dot(from, v) >= 0. && Vec2::perp_dot(v, to) >= 0.
    } else {
        Vec2::perp_dot(from, v) >= 0. || Vec2::perp_dot(v, to) >= 0.
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GunRangeCalc {
    pub base_range: f32,
}

impl GunRangeCalc {
    pub fn run(self) -> f32 {
        self.base_range
    }
}

pub struct DamageCalcRes {
    pub damage_dealt: f64,
    //
}

#[derive(Debug, Clone, Copy)]
pub struct DamageCalc {
    pub ship: ShipTemplateId,
    pub ship_pos: Vec3,
    pub ship_rot: Quat,
    pub projectile_base_damage: f64,
    pub projectile_vel: Vec3,
    pub intersection_pos: Vec3,
    //
}

impl DamageCalc {
    pub fn run(self) -> f64 {
        self.projectile_base_damage
    }
}
