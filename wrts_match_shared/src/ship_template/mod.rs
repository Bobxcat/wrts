mod germany;
mod russia;
mod sweden;

use glam::{EulerRot, Quat, Vec2, vec2};
use paste::paste;
use serde::{Deserialize, Serialize};
use slotmap::SlotMap;

const SHIP_SPEED_SCALE: f32 = 5.2;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Speed(f32);

impl Speed {
    /// meters per second (game units)
    pub fn from_mps(mps: f32) -> Self {
        Self(mps)
    }

    /// meters per second (game units)
    pub fn mps(self) -> f32 {
        self.0
    }

    /// knots
    pub fn from_kts(kts: f32) -> Self {
        Self(kts / 1.94384)
    }

    /// knots
    pub fn kts(self) -> f32 {
        self.0 * 1.94384
    }
}

/// Template information
#[derive(Debug)]
pub struct ShipTemplate {
    pub id: ShipTemplateId,
    pub ship_class: ShipClass,
    pub hull: Hull,
    pub max_speed: Speed,
    /// Speed gained per second
    pub engine_acceleration: Speed,
    /// Max radians/sec of turning this boat
    /// can perform
    pub turning_rate: f32,
    // pub rudder_acceleration: f32,
    pub max_health: f64,
    pub detection: f32,
    pub turret_templates: SlotMap<TurretTemplateId, TurretTemplate>,
    pub turret_instances: Vec<TurretInstance>,
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

/// * https://naval-encyclopedia.com/ww2
/// * https://archive.org/details/ship-design-drawings
#[derive(Debug)]
pub struct Hull {
    /// Overall length (o/a or "length overall")
    pub length: f32,
    /// The beam of the hull
    pub width: f32,
    /// Height of the hull above the water
    pub freeboard: f32,
    /// Height of the hull below the water
    pub draft: f32,
}

#[derive(Debug, Clone, Copy)]
pub enum HullLocationAxis {
    Centered,
    /// Distance from the back of right of the ship,
    /// offset by the center of this axis
    FromCenter(f32),
    /// Distance from the back or right of the ship
    FromMin(f32),
    /// Distance from the front or left of the ship
    FromMax(f32),
}

impl HullLocationAxis {
    /// Returns the offset on this axis relative to the center of the hull
    fn with_hull_axis(self, hull_length: f32) -> f32 {
        match self {
            HullLocationAxis::Centered => 0.,
            HullLocationAxis::FromCenter(x) => x,
            HullLocationAxis::FromMin(x) => x - 0.5 * hull_length,
            HullLocationAxis::FromMax(x) => hull_length - x,
        }
    }
}

/// The 2d position of an item located on a ship's hull
#[derive(Debug, Clone, Copy)]
pub struct HullLocation {
    /// Along the length of the ship, from back to front
    pub l: HullLocationAxis,
    /// Along the width of the ship, from right to left
    pub w: HullLocationAxis,
}

impl HullLocation {
    pub fn centered() -> Self {
        Self {
            l: HullLocationAxis::Centered,
            w: HullLocationAxis::Centered,
        }
    }

    pub fn new(l: HullLocationAxis, w: HullLocationAxis) -> Self {
        Self { l, w }
    }

    /// `w` will be `Centered`
    pub fn new_l(l: HullLocationAxis) -> Self {
        Self {
            l,
            w: HullLocationAxis::Centered,
        }
    }

    fn to_offset(&self, hull: &Hull) -> Vec2 {
        vec2(
            self.l.with_hull_axis(hull.length),
            self.w.with_hull_axis(hull.width),
        )
    }
    pub fn to_absolute(&self, hull: &Hull, ship_pos: Vec2, ship_rot: Quat) -> Vec2 {
        let (z_rot, _, _) = ship_rot.to_euler(EulerRot::ZXY);
        let rotated = Vec2::from_angle(z_rot).rotate(self.to_offset(hull));
        ship_pos + rotated
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Dispersion {
    /// Vertical radius of the dispersion elliptic cone.
    /// The ellipse is drawn at 1 km
    pub vertical: f32,
    /// Horizontal radius of the dispersion elliptic cone.
    /// The ellipse is drawn at 1 km
    pub horizontal: f32,
    pub sigma: f32,
}

slotmap::new_key_type! {
    pub struct TurretTemplateId;
}

#[derive(Debug, Clone)]
pub struct TurretTemplate {
    pub reload_secs: f32,
    pub damage: f64,
    pub muzzle_vel: f32,
    /// NOTE: a high max_range will not allow a shot to be made past
    /// the 45 degree shell distance at the given muzzle velocity
    pub max_range: f32,
    /// The dispersion per km of shell distance
    pub dispersion: Dispersion,
    /// The list of barrel positions on the turret
    pub barrel_count: u8,
    /// The distance between adjacent barrels on the turret
    pub barrel_spacing: f32,
}

#[derive(Debug)]
pub struct TurretInstance {
    pub ship_template: ShipTemplateId,
    pub template: TurretTemplateId,
    pub location_on_ship: HullLocation,
    pub default_dir: f32,
}

impl TurretInstance {
    pub fn turret_template(&self) -> &'static TurretTemplate {
        &self.ship_template.to_template().turret_templates[self.template]
    }

    pub fn absolute_pos(&self, ship_pos: Vec2, ship_rot: Quat) -> Vec2 {
        self.location_on_ship.to_absolute(
            &self.ship_template.to_template().hull,
            ship_pos,
            ship_rot,
        )
    }
}
