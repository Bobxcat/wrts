use glam::*;

use crate::ship_template::{Caliber, ShipTemplateId};

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

pub enum ProjectileHitRes {
    Hit { damage_dealt: f64 },
    Missed,
}

#[derive(Debug, Clone, Copy)]
pub struct ProjectileHitCalc {
    pub ship: ShipTemplateId,
    pub ship_pos: Vec2,
    pub ship_rot: Quat,
    pub projectile_base_damage: f64,
    pub projectile_caliber: Caliber,
    pub projectile_vel: Vec3,
    pub projectile_pos: Vec3,
}

impl ProjectileHitCalc {
    /// Assumes that the intersection position is on or within the ship hull
    pub fn run(self) -> ProjectileHitRes {
        // Calculate collisions in the local space of the ship hull
        let ship_rot_inv = self.ship_rot.normalize().inverse();
        let proj_pos = ship_rot_inv * (self.projectile_pos - self.ship_pos.extend(0.));
        let (ship_hull_min, ship_hull_max) = self.ship.to_template().hull.to_bounds();
        if Vec3::cmple(ship_hull_min, proj_pos).all() && Vec3::cmple(proj_pos, ship_hull_max).all()
        {
            let proj_vel = ship_rot_inv * self.projectile_vel;
            let proj_alignment = proj_vel.normalize().dot(Vec3::X).abs();
            let damage_dealt = self.projectile_base_damage * (1.5 + proj_alignment as f64);

            //

            ProjectileHitRes::Hit { damage_dealt }
        } else {
            ProjectileHitRes::Missed
        }
    }
}
