use crate::ship_template::*;

impl ShipTemplate {
    /// https://archive.org/details/yn509bogp193x
    pub(super) fn bismarck() -> ShipTemplate {
        let main_battery_prefab = Turret {
            reload_secs: 26.,
            damage: 100.,
            muzzle_vel: 820.,
            max_range: 21_200.,
            dispersion: Dispersion {
                vertical: 6.,
                horizontal: 12.83,
                sigma: 1.8,
            },
            barrel_count: 2,
            barrel_spacing: 1.,
            location_on_ship: HullLocation::centered(),
        };
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
            max_health: 60_000.,
            detection: 15_900.,
            turrets: main_battery_prefab.with_locations([
                HullLocation::new_l(HullLocationAxis::FromMin(46.15)),
                HullLocation::new_l(HullLocationAxis::FromMin(64.35)),
                HullLocation::new_l(HullLocationAxis::FromMin(174.35)),
                HullLocation::new_l(HullLocationAxis::FromMin(192.55)),
            ]),
        }
    }
}
