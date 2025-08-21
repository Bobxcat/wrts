mod germany;
mod russia;
mod sweden;

use glam::{Vec2, vec2};
use paste::paste;
use serde::{Deserialize, Serialize};

const SHIP_SPEED_SCALE: f32 = 5.2;

/// Template information
#[derive(Debug)]
pub struct ShipTemplate {
    pub id: ShipTemplateId,
    pub ship_class: ShipClass,
    pub max_speed: f32,
    pub max_health: f64,
    pub detection: f32,
    pub turrets: Vec<Turret>,
}

/// A unique numerical identifier for each ship template,
/// used for temporary serialization/deserialization.
/// Note that `ShipTemplateId`s can change between versions
///
/// For storage, use [ShipTemplateId::to_name] and [ShipTemplateId::from_name]
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShipClass {
    Battleship,
    CruiserHeavy,
    CruiserLight,
    Destroyer,
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
