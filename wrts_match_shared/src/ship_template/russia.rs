use crate::ship_template::*;

impl ShipTemplate {
    /// https://en.wikipedia.org/wiki/Kiev-class_destroyer
    pub(super) fn kiev() -> ShipTemplate {
        let main_battery_prefab = Turret {
            reload_secs: 5.,
            damage: 10.,
            muzzle_vel: 850.,
            max_range: 11_140.,
            dispersion: Dispersion {
                vertical: 3.5,
                horizontal: 8.8,
                sigma: 2.0,
            },
            barrel_count: 2,
            barrel_spacing: 1.,
            location_on_ship: HullLocation::centered(),
        };
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
            max_health: 17_500.,
            detection: 8_540.,
            turrets: main_battery_prefab.with_locations([
                HullLocation::new_l(HullLocationAxis::FromCenter(50.)),
                HullLocation::new_l(HullLocationAxis::FromCenter(40.)),
                HullLocation::new_l(HullLocationAxis::FromCenter(-50.)),
            ]),
        }
    }
}
