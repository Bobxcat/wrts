//! Important math functions
mod generated_bullet_problem_solution;

use bevy::{
    math::{DVec2, dvec3},
    prelude::*,
};
use itertools::Itertools;
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

#[derive(Debug, Clone)]
pub struct BulletProblemRes {
    pub intersection_point: Vec2,
    pub intersection_time: f32,
    pub intersection_dist: f32,
    pub projectile_dir: Vec3,
    /// Rotation around the z axis
    /// (starting at x=1,y=0 moving counter-clockwise)
    pub projectile_azimuth: f32,
    /// Rotation towards the z axis
    /// (starting from the xy plane and roating towards the z axis)
    pub projectile_elevation: f32,
}

/// Calculates the direction of firing and intersection point of a projectile launched at a ship moving at a constant velocity
/// The projectile has constant lateral speed and is affected by gravity, so it's assumed to follow a parabola
pub fn bullet_problem(
    projectile_start: Vec2,
    ship_start: Vec2,
    ship_vel: Vec2,
    muzzle_vel: f64,
    gravity: f64,
) -> Option<BulletProblemRes> {
    let p = (ship_start - projectile_start).as_dvec2();
    let v = ship_vel.as_dvec2();

    let t =
        generated_bullet_problem_solution::GENERATED_CODE(gravity, p.x, p.y, muzzle_vel, v.x, v.y);
    let t = (t.is_finite() && t.im.abs() <= 0.0000001).then_some(t.re)?;

    let intersection = p + v * t;
    let azimuth = f64::atan2(intersection.y, intersection.x);
    let elevation = f64::asin(gravity * t / (2. * muzzle_vel)); // Checked
    let proj_dir = dvec3(
        elevation.cos() * intersection.normalize().x,
        elevation.cos() * intersection.normalize().y,
        elevation.sin(),
    );
    let dist = intersection.length();

    assert!(
        elevation.is_finite(),
        "If a real t was found, elevation must be a real number"
    );

    if cfg!(debug_assertions) {
        let proj_intersection = proj_dir.truncate() * muzzle_vel * t;
        let error = intersection.distance(proj_intersection);
        assert!(error <= 0.001);
        if error > 0.001 {
            eprintln!(
                "WARN: Large bullet problem error; error={error:.2} {{Iship={:.2},Iproj={:.2},p={:.6},v={:.2}\n    elev={:.8},azi={:.8}}}\n",
                intersection, proj_intersection, p, v, elevation, azimuth,
            );
        }
    }

    Some(BulletProblemRes {
        intersection_point: intersection.as_vec2() + projectile_start,
        intersection_time: t as f32,
        intersection_dist: dist as f32,
        projectile_dir: proj_dir.as_vec3(),
        projectile_azimuth: azimuth as f32,
        projectile_elevation: elevation as f32,
    })
}
