use std::f32::consts::PI;

use crate::ship_template::{consumables::Smoke, *};

impl ShipTemplate {
    /// https://en.wikipedia.org/wiki/Kiev-class_destroyer
    pub(super) fn kiev() -> ShipTemplate {
        let ship_template = ShipTemplateId::kiev();
        let mut turret_templates = SlotMap::default();
        let main_battery = turret_templates.insert(TurretTemplate {
            reload_secs: 5.,
            damage: 200.,
            muzzle_vel: 850.,
            max_range: 11_140.,
            dispersion: Dispersion {
                vertical: 3.5,
                horizontal: 8.8,
                sigma: 2.0,
            },
            turn_rate: AngularSpeed::from_halfturn(18.),
            barrel_count: 2,
            // Estimated distance
            barrel_spacing: 1.,
        });
        ShipTemplate {
            id: ship_template,
            ship_class: ShipClass::Destroyer,
            hull: Hull {
                length: 127.8,
                width: 11.7,
                // Estimated distance
                freeboard: 5.,
                draft: 4.2,
            },
            max_speed: Speed::from_kts(42.5 * SHIP_SPEED_SCALE),
            engine_acceleration: Speed::from_kts(8. * SHIP_SPEED_SCALE),
            turning_rate: AngularSpeed::from_radps(0.4),
            max_health: 17_500.,
            detection: 8_540.,
            turret_templates,
            turret_instances: vec![
                TurretInstance {
                    ship_template,
                    template: main_battery,
                    // Estimated distance
                    location_on_ship: HullLocation::new_l(HullLocationAxis::FromCenter(-50.)),
                    movement_angle: Some(AngleRange::from_angles_deg(25., -25.)),
                    firing_angle: None,
                    default_dir: PI,
                },
                TurretInstance {
                    ship_template,
                    template: main_battery,
                    // Estimated distance
                    location_on_ship: HullLocation::new_l(HullLocationAxis::FromCenter(40.)),
                    movement_angle: Some(AngleRange::from_angles_deg(-155., 155.)),
                    firing_angle: None,
                    default_dir: 0.,
                },
                TurretInstance {
                    ship_template,
                    template: main_battery,
                    // Estimated distance
                    location_on_ship: HullLocation::new_l(HullLocationAxis::FromCenter(50.)),
                    movement_angle: Some(AngleRange::from_angles_deg(-155., 155.)),
                    firing_angle: None,
                    default_dir: 0.,
                },
            ],
            torpedoes: Some(Torpedoes {
                reload: Duration::from_secs_f64(123.),
                volleys: 2,
                torps_per_volley: 5,
                spread: 10f32.to_radians(),
                damage: 14_400.,
                speed: Speed::from_kts(60. * SHIP_SPEED_SCALE),
                range: 7_000.,
                port_firing_angle: AngleRange::from_angles_deg(40., 140.),
            }),
            consumables: Consumables::new().with_smoke(Smoke {
                action_time: Duration::from_secs(10),
                dissapation: Duration::from_secs(40),
                radius: 450.,
                cooldown: Duration::from_secs(30),
                charges: 3,
            }),
        }
    }
}
