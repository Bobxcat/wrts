use std::f32::consts::PI;

use crate::ship_template::*;

impl ShipTemplate {
    /// https://en.wikipedia.org/wiki/Japanese_battleship_Nagato
    pub(super) fn nagato() -> ShipTemplate {
        let ship_template = ShipTemplateId::nagato();
        let mut turret_templates = SlotMap::default();
        let main_battery = turret_templates.insert(TurretTemplate {
            reload_secs: 29.,
            damage: 1200.,
            muzzle_vel: 806.,
            max_range: 21_200.,
            dispersion: Dispersion {
                vertical: 6.,
                horizontal: 11.3,
                sigma: 1.8,
            },
            turn_rate: AngularSpeed::from_halfturn(47.4),
            barrel_count: 2,
            barrel_spacing: 3.,
        });
        ShipTemplate {
            id: ship_template,
            ship_class: ShipClass::Battleship,
            hull: Hull {
                length: 224.94,
                width: 34.6,
                freeboard: 9.,
                draft: 9.49,
            },
            max_speed: Speed::from_kts(26. * SHIP_SPEED_SCALE),
            engine_acceleration: Speed::from_kts(2. * SHIP_SPEED_SCALE),
            turning_rate: AngularSpeed::from_radps(0.135),
            max_health: 65_000.,
            detection: 16_600.,
            turret_templates,
            turret_instances: vec![
                TurretInstance {
                    ship_template,
                    template: main_battery,
                    location_on_ship: HullLocation::new_l(HullLocationAxis::FromMax(181.5)),
                    movement_angle: Some(AngleRange::from_angles_deg(34., -34.)),
                    firing_angle: None,
                    default_dir: PI,
                },
                TurretInstance {
                    ship_template,
                    template: main_battery,
                    location_on_ship: HullLocation::new_l(HullLocationAxis::FromMax(165.)),
                    movement_angle: Some(AngleRange::from_angles_deg(34., -34.)),
                    firing_angle: None,
                    default_dir: PI,
                },
                TurretInstance {
                    ship_template,
                    template: main_battery,
                    location_on_ship: HullLocation::new_l(HullLocationAxis::FromMax(66.)),
                    movement_angle: Some(AngleRange::from_angles_deg(-138., 138.)),
                    firing_angle: None,
                    default_dir: 0.,
                },
                TurretInstance {
                    ship_template,
                    template: main_battery,
                    location_on_ship: HullLocation::new_l(HullLocationAxis::FromMax(48.75)),
                    movement_angle: Some(AngleRange::from_angles_deg(-138., 138.)),
                    firing_angle: None,
                    default_dir: 0.,
                },
            ],
            torpedoes: None,
            consumables: Consumables::new(),
        }
    }
}
