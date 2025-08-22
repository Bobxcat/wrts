use crate::ship_template::*;

impl ShipTemplate {
    /// https://en.wikipedia.org/wiki/HSwMS_%C3%96land_(J16)
    pub(super) fn oland() -> ShipTemplate {
        let main_battery_prefab = Turret {
            reload_secs: 2.3,
            damage: 10.,
            muzzle_vel: 850.,
            max_range: 10100.,
            dispersion: Dispersion {
                vertical: 3.5,
                horizontal: 9.,
                sigma: 2.0,
            },
            barrel_count: 2,
            barrel_spacing: 1.,
            location_on_ship: HullLocation::centered(),
        };
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
            max_health: 14100.,
            detection: 7200.,
            turrets: main_battery_prefab.with_locations([
                HullLocation::new_l(HullLocationAxis::FromCenter(80.)),
                HullLocation::new_l(HullLocationAxis::FromCenter(-80.)),
            ]),
        }
    }
}
