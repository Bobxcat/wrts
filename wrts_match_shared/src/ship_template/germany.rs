use crate::ship_template::*;

impl ShipTemplate {
    pub(super) fn bismarck() -> ShipTemplate {
        let main_battery_prefab = Turret {
            reload_secs: 26.,
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
        };
        ShipTemplate {
            id: ShipTemplateId::bismarck(),
            ship_class: ShipClass::Battleship,
            max_speed: 31. * SHIP_SPEED_SCALE,
            max_health: 60_000.,
            detection: 15900.,
            turrets: vec![
                main_battery_prefab.clone().with_location(vec2(30., 0.)),
                main_battery_prefab.clone().with_location(vec2(20., 0.)),
                main_battery_prefab.clone().with_location(vec2(-20., 0.)),
                main_battery_prefab.clone().with_location(vec2(-30., 0.)),
            ],
        }
    }
}
