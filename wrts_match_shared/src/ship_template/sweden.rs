use std::f32::consts::PI;

use crate::ship_template::*;

impl ShipTemplate {
    /// https://en.wikipedia.org/wiki/HSwMS_%C3%96land_(J16)
    pub(super) fn oland() -> ShipTemplate {
        let ship_template = ShipTemplateId::oland();
        let mut turret_templates = SlotMap::default();
        let main_battery = turret_templates.insert(TurretTemplate {
            reload_secs: 2.3,
            damage: 150.,
            muzzle_vel: 850.,
            max_range: 10_100.,
            dispersion: Dispersion {
                vertical: 3.5,
                horizontal: 9.,
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
                length: 112.,
                width: 11.2,
                // Estimated distance
                freeboard: 4.,
                draft: 3.4,
            },
            max_speed: Speed::from_kts(35. * SHIP_SPEED_SCALE),
            engine_acceleration: Speed::from_kts(5. * SHIP_SPEED_SCALE),
            turning_rate: AngularSpeed::from_radps(0.45),
            max_health: 14_100.,
            detection: 7_200.,
            turret_templates,
            turret_instances: vec![
                TurretInstance {
                    ship_template,
                    template: main_battery,
                    // Estimated distance
                    location_on_ship: HullLocation::new_l(HullLocationAxis::FromCenter(-40.)),
                    movement_angle: None,
                    firing_angle: Some(AngleRange::from_angles_deg(25., -25.)),
                    default_dir: PI,
                },
                TurretInstance {
                    ship_template,
                    template: main_battery,
                    // Estimated distance
                    location_on_ship: HullLocation::new_l(HullLocationAxis::FromCenter(40.)),
                    movement_angle: None,
                    firing_angle: Some(AngleRange::from_angles_deg(-155., 155.)),
                    default_dir: 0.,
                },
            ],
            torpedoes: Some(Torpedoes {
                reload: Duration::from_secs_f64(70.),
                volleys: 2,
                torps_per_volley: 3,
                spread: 6f32.to_radians(),
                damage: 10_700.,
                speed: Speed::from_kts(80. * SHIP_SPEED_SCALE),
                range: 12_000.,
                port_firing_angle: AngleRange::from_angles_deg(60., 120.),
            }),
            consumables: Consumables::new(),
        }
    }
}
