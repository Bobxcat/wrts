use std::f32::consts::PI;

use crate::ship_template::*;

impl ShipTemplate {
    /// https://archive.org/details/yn509bogp193x
    pub(super) fn bismarck() -> ShipTemplate {
        let ship_template = ShipTemplateId::bismarck();
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
            turn_rate: AngularSpeed::from_halfturn(36.),
            barrel_count: 2,
            // Estimated distance
            barrel_spacing: 1.,
        });
        ShipTemplate {
            id: ship_template,
            ship_class: ShipClass::Battleship,
            hull: Hull {
                length: 251.,
                width: 36.,
                freeboard: 8.7,
                draft: 9.3,
            },
            max_speed: Speed::from_kts(31. * SHIP_SPEED_SCALE),
            engine_acceleration: Speed::from_kts(3. * SHIP_SPEED_SCALE),
            turning_rate: AngularSpeed::from_radps(0.15),
            max_health: 60_000.,
            detection: 15_900.,
            detection_when_firing_through_smoke: 15_100.,
            turret_templates,
            turret_instances: vec![
                TurretInstance {
                    ship_template,
                    template: main_battery,
                    location_on_ship: HullLocation::new_l(HullLocationAxis::FromMin(46.15)),
                    movement_angle: Some(AngleRange::from_angles_deg(34., -34.)),
                    firing_angle: None,
                    default_dir: PI,
                },
                TurretInstance {
                    ship_template,
                    template: main_battery,
                    location_on_ship: HullLocation::new_l(HullLocationAxis::FromMin(64.35)),
                    movement_angle: Some(AngleRange::from_angles_deg(34., -34.)),
                    firing_angle: None,
                    default_dir: PI,
                },
                TurretInstance {
                    ship_template,
                    template: main_battery,
                    location_on_ship: HullLocation::new_l(HullLocationAxis::FromMin(174.35)),
                    movement_angle: Some(AngleRange::from_angles_deg(-138., 138.)),
                    firing_angle: None,
                    default_dir: 0.,
                },
                TurretInstance {
                    ship_template,
                    template: main_battery,
                    location_on_ship: HullLocation::new_l(HullLocationAxis::FromMin(192.55)),
                    movement_angle: Some(AngleRange::from_angles_deg(-138., 138.)),
                    firing_angle: None,
                    default_dir: 0.,
                },
            ],
            torpedoes: None,
            consumables: Consumables::new(),
        }
    }
    /// * https://en.wikipedia.org/wiki/German_cruiser_Admiral_Hipper
    /// * https://en.wikipedia.org/wiki/Admiral_Hipper-class_cruiser
    /// * Jane's WW2 ships
    pub(super) fn hipper() -> ShipTemplate {
        let ship_template = ShipTemplateId::hipper();
        let mut turret_templates = SlotMap::default();
        let main_battery = turret_templates.insert(TurretTemplate {
            reload_secs: 10.5,
            damage: 400.,
            muzzle_vel: 925.,
            max_range: 17_700.,
            dispersion: Dispersion {
                vertical: 4.,
                horizontal: 8.75,
                sigma: 1.9,
            },
            turn_rate: AngularSpeed::from_halfturn(36.),
            barrel_count: 2,
            barrel_spacing: 3.,
        });
        ShipTemplate {
            id: ship_template,
            ship_class: ShipClass::CruiserHeavy,
            hull: Hull {
                length: 202.8,
                width: 21.3,
                freeboard: 4.35,
                draft: 5.4,
            },
            max_speed: Speed::from_kts(32. * SHIP_SPEED_SCALE),
            engine_acceleration: Speed::from_kts(4. * SHIP_SPEED_SCALE),
            turning_rate: AngularSpeed::from_radps(0.20),
            max_health: 43_800.,
            detection: 15_900.,
            detection_when_firing_through_smoke: 8_500.,
            turret_templates,
            turret_instances: vec![
                TurretInstance {
                    ship_template,
                    template: main_battery,
                    location_on_ship: HullLocation::new_l(HullLocationAxis::FromMax(175.5)),
                    movement_angle: Some(AngleRange::from_angles_deg(36., -36.)),
                    firing_angle: None,
                    default_dir: PI,
                },
                TurretInstance {
                    ship_template,
                    template: main_battery,
                    location_on_ship: HullLocation::new_l(HullLocationAxis::FromMax(164.25)),
                    movement_angle: Some(AngleRange::from_angles_deg(36., -36.)),
                    firing_angle: None,
                    default_dir: PI,
                },
                TurretInstance {
                    ship_template,
                    template: main_battery,
                    location_on_ship: HullLocation::new_l(HullLocationAxis::FromMax(57.75)),
                    movement_angle: Some(AngleRange::from_angles_deg(-145., 145.)),
                    firing_angle: None,
                    default_dir: 0.,
                },
                TurretInstance {
                    ship_template,
                    template: main_battery,
                    location_on_ship: HullLocation::new_l(HullLocationAxis::FromMax(46.5)),
                    movement_angle: Some(AngleRange::from_angles_deg(-145., 145.)),
                    firing_angle: None,
                    default_dir: 0.,
                },
            ],
            torpedoes: None,
            consumables: Consumables::new(),
        }
    }
}
