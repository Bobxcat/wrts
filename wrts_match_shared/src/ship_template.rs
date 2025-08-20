use glam::{Vec2, vec2};
use paste::paste;
use serde::{Deserialize, Serialize};

const SHIP_SPEED_SCALE: f32 = 5.2;

/// Template information
#[derive(Debug)]
pub struct ShipTemplate {
    pub id: ShipTemplateId,
    pub max_speed: f32,
    pub max_health: f64,
    pub detection: f32,
    pub turrets: Vec<Turret>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ShipTemplateId(u32);

macro_rules! make_ship_template_ids {
    (make_ids; $ship_name:ident {$($ship_id_expr:tt)*}) => {
        paste! {
            const [<$ship_name:upper _ID>]: u32 = $($ship_id_expr)*;

            pub const fn $ship_name() -> Self {
                Self(Self::[<$ship_name:upper _ID>])
            }
        }
    };
    (make_ids; $ship_name:ident) => {
        make_ship_template_ids!(make_ids; $ship_name {0});
    };
    (make_ids; $ship_name1:ident $ship_name2:ident  $($others:ident)*) => {
        make_ship_template_ids!(make_ids; $ship_name1 {Self:: [<$ship_name2:upper _ID>]  + 1});
        make_ship_template_ids!(make_ids; $ship_name2 $($others)*);
    };

    (make_name2id; $($ship_names:ident)*) => {
        pub fn from_name(name: &str) -> Option<ShipTemplateId> {
            let name: String = name.to_lowercase();
            Some(match String::as_str(&name) {
                $(stringify!($ship_names) => Self::$ship_names(),)*
                _ => return None,
            })
        }
    };

    (make_id2name; $($ship_names:ident)*) => {
        pub fn to_name(self) -> &'static str {
            paste!{
                match self {
                    $(Self(Self::[<$ship_names:upper _ID>]) => stringify!($ship_names),)*
                    _ => unreachable!("Impossible ShipTemplateId encountered: `{self:?}`"),
                }
            }
        }
    };

    (make_id2template; $($ship_names:ident)*) => {
        pub fn to_template(self) -> &'static ShipTemplate {
            paste!{
                match self {
                    $(Self(Self::[<$ship_names:upper _ID>]) => {
                        static ___STORE: ::std::sync::LazyLock<ShipTemplate> = ::std::sync::LazyLock::new(ShipTemplate::$ship_names);
                        &___STORE
                    },)*
                    _ => unreachable!("Impossible ShipTemplateId encountered: `{self:?}`"),
                }
            }
        }
    };

    ($($ship_names:ident)*) => {
        impl ShipTemplateId {
            make_ship_template_ids!(make_ids; $($ship_names)*);
            make_ship_template_ids!(make_name2id; $($ship_names)*);
            make_ship_template_ids!(make_id2name; $($ship_names)*);
            make_ship_template_ids!(make_id2template; $($ship_names)*);
        }
    };
}

make_ship_template_ids!(oland bismarck kiev);

impl ShipTemplate {
    pub fn from_name(name: &str) -> Option<&'static Self> {
        ShipTemplateId::from_name(name).map(Self::from_id)
    }

    pub fn from_id(id: ShipTemplateId) -> &'static Self {
        id.to_template()
    }

    fn oland() -> ShipTemplate {
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
            max_speed: 35. * SHIP_SPEED_SCALE,
            max_health: 20_000.,
            detection: 7200.,
            turrets: vec![
                main_battery_prefab.clone().with_location(vec2(5., 0.)),
                main_battery_prefab.clone().with_location(vec2(-5., 0.)),
            ],
        }
    }

    fn bismarck() -> ShipTemplate {
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

    fn kiev() -> ShipTemplate {
        todo!()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Dispersion {
    // Vertical radius of the dispersion elliptic cone
    // The ellipse is drawn at 1 km
    pub vertical: f32,
    // Horizontal radius of the dispersion elliptic cone
    // The ellipse is drawn at 1 km
    pub horizontal: f32,
    pub sigma: f32,
}

#[derive(Debug, Clone)]
pub struct Turret {
    pub reload_secs: f32,
    pub damage: f64,
    pub muzzle_vel: f32,
    /// NOTE: a high max_range will not allow a shot to be made past
    /// the 45 degree shell distance at the given muzzle velocity
    pub max_range: f32,
    /// The dispersion per km of shell distance
    pub dispersion: Dispersion,
    /// The list of barrel positions on the turret
    pub barrels: Vec<Vec2>,
    // /// Rotation around the z axis
    // pub rotation: f32,
    pub location_on_ship: Vec2,
}

impl Turret {
    pub fn with_location(mut self, location_on_ship: Vec2) -> Self {
        self.location_on_ship = location_on_ship;
        self
    }
}
