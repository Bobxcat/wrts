use bevy::prelude::*;
use rand::Fill;
use rand_distr::Distribution;

use crate::{Health, Team, Velocity};

const SHIP_SPEED_SCALE: f32 = 5.2;

#[derive(Debug, Component, Clone)]
#[require(Team, Health, Sprite, Transform, Velocity)]
pub struct Ship {
    pub speed: f32,
    pub turrets: Vec<Turret>,
    pub detection: f32,
}

impl Ship {
    pub fn bismarck() -> Ship {
        let main_battery_prefab = Turret {
            reload_timer: Timer::from_seconds(26., TimerMode::Repeating),
            damage: 100.,
            muzzle_vel: 820.,
            max_range: 21200.,
            dispersion: Dispersion {
                vertical: 6.,
                horizontal: 12.83,
                sigma: 1.8,
            },
            barrels: vec![vec2(2., 0.4), vec2(2., -0.4)],
            location_on_ship: Vec2::ZERO,
            sprite: Sprite::from_color(Color::linear_rgb(0.5, 0.5, 0.6), vec2(10., 5.)),
        };
        Ship {
            speed: 31. * SHIP_SPEED_SCALE,
            turrets: vec![
                main_battery_prefab.clone().with_location(vec2(30., 0.)),
                main_battery_prefab.clone().with_location(vec2(20., 0.)),
                main_battery_prefab.clone().with_location(vec2(-20., 0.)),
                main_battery_prefab.clone().with_location(vec2(-30., 0.)),
            ],
            detection: 15900.,
        }
    }

    pub fn oland() -> Ship {
        let main_battery_prefab = Turret {
            reload_timer: Timer::from_seconds(2.3, TimerMode::Repeating),
            damage: 10.,
            muzzle_vel: 850.,
            max_range: 10100.,
            dispersion: Dispersion {
                vertical: 3.5,
                horizontal: 9.,
                sigma: 2.0,
            },
            barrels: vec![vec2(1., 0.2), vec2(1., -0.2)],
            location_on_ship: Vec2::ZERO,
            sprite: Sprite::from_color(Color::linear_rgb(0.5, 0.5, 0.6), vec2(5., 2.5)),
        };
        Ship {
            speed: 10. * SHIP_SPEED_SCALE,
            // speed: 35. * SHIP_SPEED_SCALE,
            turrets: vec![
                main_battery_prefab.clone().with_location(vec2(5., 0.)),
                main_battery_prefab.clone().with_location(vec2(-5., 0.)),
            ],
            detection: 7200.,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Dispersion {
    // Vertical radius of the dispersion elliptic cone
    // The ellipse is drawn at 1 km
    pub vertical: f32,
    // Horizontal radius of the dispersion elliptic cone
    // The ellipse is drawn at 1 km
    pub horizontal: f32,
    pub sigma: f32,
}

impl Dispersion {
    pub fn apply_dispersion(&self, nominal_direction: Vec3) -> Vec3 {
        let dist = rand_distr::Normal::new(0., self.sigma).unwrap();
        let mut rng = rand::rng();
        let h_squared = self.horizontal * self.horizontal;
        let v_squared = self.vertical * self.vertical;
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
}

#[derive(Debug, Clone)]
pub struct Turret {
    pub reload_timer: Timer,
    pub damage: f64,
    pub muzzle_vel: f32,
    /// NOTE: a high max_range will not allow a shot to be made past
    /// the 45 degree shell distance at the given muzzle velocity
    pub max_range: f32,
    /// The dispersion per km of shell distance
    pub dispersion: Dispersion,
    /// The list of barrel positions on the turret
    pub barrels: Vec<Vec2>,
    // /// Rotation around the z axis
    // pub rotation: f32,
    pub location_on_ship: Vec2,
    pub sprite: Sprite,
}

impl Turret {
    pub fn with_location(mut self, location_on_ship: Vec2) -> Self {
        self.location_on_ship = location_on_ship;
        self
    }
}
