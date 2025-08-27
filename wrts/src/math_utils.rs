//! Important math functions

use bevy::{math::DVec2, prelude::*};
use num_complex::{Complex, ComplexFloat};

/// Returns the angle, in radians from the ground, that a bullet needs to be fired from to arrive at the given distance from its origin
pub fn gun_angle_for_distance(
    dist: f64,
    muzzle_vel: f64,
    gravity: f64,
    prefer_low: bool,
) -> Option<f64> {
    let disc = muzzle_vel.powi(4) - dist * dist * gravity * gravity;
    if disc < 0. {
        return None;
    }
    let signed_sqrt_disc = match prefer_low {
        true => 1.,
        false => -1.,
    } * disc.sqrt();
    let v_x = f64::sqrt((muzzle_vel * muzzle_vel + signed_sqrt_disc) * 0.5);
    let v_y = (dist * gravity) / (2. * v_x);
    let theta = f64::atan2(v_y, v_x);
    Some(theta)
}

/// Returns the bullet velocity vector
pub fn bullet_vel_for_shot(
    origin: Vec2,
    dest: Vec2,
    muzzle_vel: f64,
    gravity: f64,
    prefer_low: bool,
) -> Option<Vec3> {
    let dir = (dest - origin).try_normalize()?;
    let angle = gun_angle_for_distance(
        Vec2::distance(origin, dest) as f64,
        muzzle_vel,
        gravity,
        prefer_low,
    )?;

    let rot_axis = Vec3::cross(dir.extend(0.), Vec3::Z).normalize();
    let dir3 = Mat3::from_axis_angle(rot_axis, angle as f32) * dir.extend(0.);
    Some(dir3 * muzzle_vel as f32)
}

pub fn max_dist_for_vel(muzzle_vel: f64, gravity: f64) -> f64 {
    let v_x = muzzle_vel * f64::cos(std::f64::consts::FRAC_PI_4);
    let v_y = v_x;
    v_x * 2. * v_y / gravity
}

#[derive(Debug, Clone, Copy)]
pub struct TorpedoProblemRes {
    pub intersection_point: Vec2,
    pub projectile_dir: Vec2,
    pub intersection_time: f64,
}

/// Calculates the intersection between a projectile being launched at a ship and the ship,
/// assuming both the ship and projectile move at a constant velocity
pub fn torpedo_problem(
    projectile_start: Vec2,
    ship_start: Vec2,
    ship_vel: Vec2,
    muzzle_vel: f64,
) -> Option<TorpedoProblemRes> {
    // Wolfram alpha black box
    let p = ship_start.as_dvec2() - projectile_start.as_dvec2();
    let v = ship_vel.as_dvec2();
    let s = muzzle_vel;
    let disc = f64::powi(2. * p.x * v.x + 2. * p.y * v.y, 2)
        - 4. * (p.x * p.x + p.y * p.y) * (-s * s + v.x * v.x + v.y * v.y);
    if disc < 0. {
        return None;
    }
    let t_num = -f64::sqrt(disc) - 2. * p.x * v.x - 2. * p.y * v.y;
    let t_den = 2. * (-s * s + v.x * v.x + v.y * v.y);
    let t = t_num / t_den;
    // ...

    let intersection_point = ship_start + ship_vel * t as f32;
    let projectile_dir = (intersection_point - projectile_start).normalize();
    Some(TorpedoProblemRes {
        intersection_point,
        projectile_dir,
        intersection_time: t,
    })
}

/// Calculates the time of intersection with newton's method
pub(crate) fn _bullet_problem_newtons(g: f64, p: DVec2, muzzle_vel: f64, v: DVec2) -> Complex<f64> {
    let s_p = Complex::from(muzzle_vel);
    let g = Complex::from(g);

    let t_proj = |dist: Complex<f64>| -> Complex<f64> {
        Complex::from(1. / muzzle_vel)
            * dist
            * Complex::sqrt(
                Complex::from(0.25)
                    * dist.powi(2)
                    * g.powi(2)
                    * (Complex::from(0.5)
                        * (s_p.powi(2) + Complex::sqrt(s_p.powi(4) - dist.powi(2) * g.powi(2))))
                    .powi(-2)
                    + Complex::from(1.),
            )
    };
    let t_err = |t_est: Complex<f64>| -> Complex<f64> {
        let x = Complex::from(p.x) + Complex::from(v.x) * t_est;
        let y = Complex::from(p.y) + Complex::from(v.y) * t_est;
        let dist = Complex::sqrt(x * x + y * y);
        t_proj(dist) - t_est
    };

    let mut t = Complex::new(1., 0.);
    for _ in 0..10 {
        // Newton's method with a computed derivative
        // Note: t_err is a pretty smooth function so the delta doesn't need to be too small
        const DERIV_DELTA: f64 = 0.00001;
        let t_err_value = t_err(t);
        let t_err_derivative = (t_err(t + DERIV_DELTA) - t_err_value) / DERIV_DELTA;
        t = t - t_err_value / t_err_derivative;
    }

    t
}
