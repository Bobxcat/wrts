use crate::ship_template::*;

impl ShipTemplate {
    pub(super) fn oland() -> ShipTemplate {
        let main_battery_prefab = Turret {
            reload_secs: 4.,
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
        };
        ShipTemplate {
            id: ShipTemplateId::oland(),
            ship_class: ShipClass::Destroyer,
            max_speed: 35. * SHIP_SPEED_SCALE,
            max_health: 20_000.,
            detection: 7200.,
            turrets: vec![
                main_battery_prefab.clone().with_location(vec2(5., 0.)),
                main_battery_prefab.clone().with_location(vec2(-5., 0.)),
            ],
        }
    }
}
