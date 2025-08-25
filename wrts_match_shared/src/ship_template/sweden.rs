use std::f32::consts::PI;

use crate::ship_template::*;

impl ShipTemplate {
    /// https://en.wikipedia.org/wiki/HSwMS_%C3%96land_(J16)
    pub(super) fn oland() -> ShipTemplate {
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
            barrel_count: 2,
            barrel_spacing: 1.,
        });
        ShipTemplate {
            id: ShipTemplateId::oland(),
            ship_class: ShipClass::Destroyer,
            hull: Hull {
                length: 112.,
                width: 11.2,
                freeboard: 4.,
                draft: 3.4,
            },
            max_speed: Speed::from_kts(35. * SHIP_SPEED_SCALE),
            engine_acceleration: Speed::from_kts(5. * SHIP_SPEED_SCALE),
            turning_rate: 0.5,
            max_health: 14_100.,
            detection: 7_200.,
            turret_templates,
            turret_instances: vec![
                TurretInstance {
                    ship_template: ShipTemplateId::oland(),
                    template: main_battery,
                    location_on_ship: HullLocation::new_l(HullLocationAxis::FromCenter(40.)),
                    default_dir: 0.,
                },
                TurretInstance {
                    ship_template: ShipTemplateId::oland(),
                    template: main_battery,
                    location_on_ship: HullLocation::new_l(HullLocationAxis::FromCenter(-40.)),
                    default_dir: PI,
                },
            ],
        }
    }
}
