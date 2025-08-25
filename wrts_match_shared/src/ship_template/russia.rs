use std::f32::consts::PI;

use crate::ship_template::*;

impl ShipTemplate {
    /// https://en.wikipedia.org/wiki/Kiev-class_destroyer
    pub(super) fn kiev() -> ShipTemplate {
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
            barrel_count: 2,
            barrel_spacing: 1.,
        });
        ShipTemplate {
            id: ShipTemplateId::kiev(),
            ship_class: ShipClass::Destroyer,
            hull: Hull {
                length: 127.8,
                width: 11.7,
                freeboard: 5.,
                draft: 4.2,
            },
            max_speed: Speed::from_kts(42.5 * SHIP_SPEED_SCALE),
            engine_acceleration: Speed::from_kts(8. * SHIP_SPEED_SCALE),
            turning_rate: 0.45,
            max_health: 17_500.,
            detection: 8_540.,
            turret_templates,
            turret_instances: vec![
                TurretInstance {
                    ship_template: ShipTemplateId::kiev(),
                    template: main_battery,
                    location_on_ship: HullLocation::new_l(HullLocationAxis::FromCenter(50.)),
                    default_dir: 0.,
                },
                TurretInstance {
                    ship_template: ShipTemplateId::kiev(),
                    template: main_battery,
                    location_on_ship: HullLocation::new_l(HullLocationAxis::FromCenter(40.)),
                    default_dir: 0.,
                },
                TurretInstance {
                    ship_template: ShipTemplateId::kiev(),
                    template: main_battery,
                    location_on_ship: HullLocation::new_l(HullLocationAxis::FromCenter(-50.)),
                    default_dir: PI,
                },
            ],
        }
    }
}
