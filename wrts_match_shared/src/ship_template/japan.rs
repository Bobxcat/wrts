use std::f32::consts::{FRAC_PI_2, PI};

use crate::ship_template::*;

impl ShipTemplate {
    /// https://en.wikipedia.org/wiki/Japanese_battleship_Nagato
    pub(super) fn nagato() -> ShipTemplate {
        use HullLocationAxis::*;
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
            targeting_mode: TargetingMode::Primary,
        });
        let secondary_battery_140mm = turret_templates.insert(TurretTemplate {
            reload_secs: 8.0,
            damage: 250.,
            muzzle_vel: 850.,
            max_range: 5_600.,
            dispersion: Dispersion {
                vertical: 15.,
                horizontal: 30.,
                sigma: 1.8,
            },
            turn_rate: AngularSpeed::from_halfturn(5.),
            barrel_count: 1,
            barrel_spacing: 1.,
            targeting_mode: TargetingMode::Secondary,
        });
        let secondary_battery_127mm = turret_templates.insert(TurretTemplate {
            reload_secs: 5.,
            damage: 200.,
            muzzle_vel: 725.,
            max_range: 5_600.,
            dispersion: Dispersion {
                vertical: 20.,
                horizontal: 50.,
                sigma: 1.8,
            },
            turn_rate: AngularSpeed::from_halfturn(5.),
            barrel_count: 2,
            // Estimated distance
            barrel_spacing: 0.896,
            targeting_mode: TargetingMode::Secondary,
        });

        let secondary_battery_140mm_instances = [
            TurretInstance {
                ship_template,
                template: secondary_battery_140mm,
                location_on_ship: HullLocation {
                    l: FromMax(72.75),
                    w: FromCenter(12.),
                },
                // Estimated angle
                movement_angle: Some(AngleRange::from_angles_deg(10., 145.)),
                firing_angle: None,
                default_dir: FRAC_PI_2,
            },
            TurretInstance {
                ship_template,
                template: secondary_battery_140mm,
                location_on_ship: HullLocation {
                    l: FromMax(81.),
                    w: FromCenter(12.75),
                },
                movement_angle: Some(AngleRange::from_angles_deg(10., 145.)),
                firing_angle: None,
                default_dir: FRAC_PI_2,
            },
            TurretInstance {
                ship_template,
                template: secondary_battery_140mm,
                location_on_ship: HullLocation {
                    l: FromMax(85.5),
                    w: FromCenter(9.75),
                },
                movement_angle: Some(AngleRange::from_angles_deg(0., 130.)),
                firing_angle: None,
                default_dir: 0.,
            },
            TurretInstance {
                ship_template,
                template: secondary_battery_140mm,
                location_on_ship: HullLocation {
                    l: FromMax(91.5),
                    w: FromCenter(13.5),
                },
                movement_angle: Some(AngleRange::from_angles_deg(8., 143.)),
                firing_angle: None,
                default_dir: FRAC_PI_2,
            },
            TurretInstance {
                ship_template,
                template: secondary_battery_140mm,
                location_on_ship: HullLocation {
                    l: FromMax(94.5),
                    w: FromCenter(10.5),
                },
                movement_angle: Some(AngleRange::from_angles_deg(10., 140.)),
                firing_angle: None,
                default_dir: FRAC_PI_2,
            },
            TurretInstance {
                ship_template,
                template: secondary_battery_140mm,
                location_on_ship: HullLocation {
                    l: FromMax(99.),
                    w: FromCenter(13.5),
                },
                movement_angle: Some(AngleRange::from_angles_deg(9., 144.)),
                firing_angle: None,
                default_dir: FRAC_PI_2,
            },
            TurretInstance {
                ship_template,
                template: secondary_battery_140mm,
                location_on_ship: HullLocation {
                    l: FromMax(102.),
                    w: FromCenter(9.75),
                },
                movement_angle: Some(AngleRange::from_angles_deg(10., 135.)),
                firing_angle: None,
                default_dir: FRAC_PI_2,
            },
            TurretInstance {
                ship_template,
                template: secondary_battery_140mm,
                location_on_ship: HullLocation {
                    l: FromMax(108.),
                    w: FromCenter(13.95),
                },
                movement_angle: Some(AngleRange::from_angles_deg(9., 129.)),
                firing_angle: None,
                default_dir: FRAC_PI_2,
            },
            TurretInstance {
                ship_template,
                template: secondary_battery_140mm,
                location_on_ship: HullLocation {
                    l: FromMax(133.5),
                    w: FromCenter(12.75),
                },
                movement_angle: Some(AngleRange::from_angles_deg(50., 170.)),
                firing_angle: None,
                default_dir: FRAC_PI_2,
            },
            TurretInstance {
                ship_template,
                template: secondary_battery_140mm,
                location_on_ship: HullLocation {
                    l: FromMax(142.5),
                    w: FromCenter(9.),
                },
                movement_angle: Some(AngleRange::from_angles_deg(45., 170.)),
                firing_angle: None,
                default_dir: FRAC_PI_2,
            },
        ]
        .map(|instance| [instance.mirrored(), instance])
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

        let secondary_battery_127mm_instances = [
            TurretInstance {
                ship_template,
                template: secondary_battery_127mm,
                location_on_ship: HullLocation {
                    l: FromMax(94.05),
                    w: FromCenter(10.2),
                },
                movement_angle: Some(AngleRange::from_angles_deg(0., 165.)),
                firing_angle: None,
                default_dir: FRAC_PI_2,
            },
            TurretInstance {
                ship_template,
                template: secondary_battery_127mm,
                location_on_ship: HullLocation {
                    l: FromMax(132.),
                    w: FromCenter(13.5),
                },
                movement_angle: Some(AngleRange::from_angles_deg(20., 180.)),
                firing_angle: None,
                default_dir: FRAC_PI_2,
            },
        ]
        .map(|instance| [instance.mirrored(), instance])
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

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
            detection_when_firing_through_smoke: 16_700.,
            turret_templates,
            turret_instances: [
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
            ]
            .into_iter()
            .chain(secondary_battery_140mm_instances)
            .chain(secondary_battery_127mm_instances)
            .collect(),
            torpedoes: None,
            consumables: Consumables::new(),
        }
    }
}
