use std::f32::consts::PI;

use crate::ship_template::*;

impl ShipTemplate {
    /// https://archive.org/details/yn509bogp193x
    pub(super) fn bismarck() -> ShipTemplate {
        let mut turret_templates = SlotMap::default();
        let main_battery = turret_templates.insert(TurretTemplate {
            reload_secs: 26.,
            damage: 1000.,
            muzzle_vel: 820.,
            max_range: 21_200.,
            dispersion: Dispersion {
                vertical: 6.,
                horizontal: 12.83,
                sigma: 1.8,
            },
            barrel_count: 2,
            barrel_spacing: 1.,
        });
        ShipTemplate {
            id: ShipTemplateId::bismarck(),
            ship_class: ShipClass::Battleship,
            hull: Hull {
                length: 251.,
                width: 36.,
                freeboard: 8.7,
                draft: 9.3,
            },
            max_speed: Speed::from_kts(31. * SHIP_SPEED_SCALE),
            engine_acceleration: Speed::from_kts(3. * SHIP_SPEED_SCALE),
            turning_rate: 0.15,
            max_health: 60_000.,
            detection: 15_900.,
            turret_templates,
            turret_instances: vec![
                TurretInstance {
                    ship_template: ShipTemplateId::bismarck(),
                    template: main_battery,
                    location_on_ship: HullLocation::new_l(HullLocationAxis::FromMin(46.15)),
                    default_dir: 0.,
                },
                TurretInstance {
                    ship_template: ShipTemplateId::bismarck(),
                    template: main_battery,
                    location_on_ship: HullLocation::new_l(HullLocationAxis::FromMin(64.35)),
                    default_dir: 0.,
                },
                TurretInstance {
                    ship_template: ShipTemplateId::bismarck(),
                    template: main_battery,
                    location_on_ship: HullLocation::new_l(HullLocationAxis::FromMin(174.35)),
                    default_dir: PI,
                },
                TurretInstance {
                    ship_template: ShipTemplateId::bismarck(),
                    template: main_battery,
                    location_on_ship: HullLocation::new_l(HullLocationAxis::FromMin(192.55)),
                    default_dir: PI,
                },
            ],
        }
    }
}
