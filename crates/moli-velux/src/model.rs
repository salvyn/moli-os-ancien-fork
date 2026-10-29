//! A VELUX ACTIVE home as its cloud describes it: modules (`homesdata`) and
//! their state (`homestatus`). The gateway (NXG) says rain and departure
//! mode; each window, blind or shutter (NXO) its position, 0 closed to 100
//! open, and where it is heading; the indoor climate sensor (NXS) reads its
//! room (temperature in tenths of a degree, humidity, CO₂, light, air
//! quality); the departure switch (NXD) only has a battery.
//!
//! Orders go to `syncapi/v1/setstate`, as the app sends them: a blind or a
//! shutter takes a position (or -1: stop) as it is; a roof window's position
//! is signed ([`crate::sign`]), and a window is stopped by the gateway
//! (`stop_movements`), which stops everything moving.

use std::sync::Arc;

use moli_core::{Access, Kind, PointSpec, Semantic, Unit, Value};
use serde_json::{Value as Json, json};

use crate::sign::Signer;

/// What a module is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Gateway,
    Window,
    /// A blind, an awning or a roller shutter: something in front of a window.
    Cover,
    Sensor,
    Departure,
}

#[derive(Clone, Debug)]
pub struct Module {
    pub id: String,
    pub role: Role,
    pub name: String,
    pub room: Option<String>,
    pub(crate) room_id: Option<String>,
    /// `velux_type` (`window`, `blind`, `shutter`…), else the module type.
    pub model: String,
    /// The gateway it is reached through.
    pub bridge: Option<String>,
}

/// The modules of a home, as `homesdata` lists them.
#[derive(Clone, Debug)]
pub struct Home {
    pub id: String,
    pub timezone: String,
    pub modules: Vec<Module>,
}

/// Where an order sends a window or a blind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// 0 closed to 100 open.
    Position(i64),
    Stop,
}

/// The position the cloud reads as « stop ».
const STOP: i64 = -1;

impl Home {
    /// The home of one `homesdata` entry, with the modules Moli knows.
    #[must_use]
    pub fn parse(home: &Json) -> Option<Self> {
        let rooms = home["rooms"].as_array();
        let room_name = |id: &str| {
            rooms?
                .iter()
                .find(|r| r["id"] == id)
                .and_then(|r| r["name"].as_str())
                .map(|n| n.trim().to_owned())
        };
        let modules = home["modules"]
            .as_array()?
            .iter()
            .filter_map(|m| {
                let kind = m["type"].as_str()?;
                let velux = m["velux_type"].as_str();
                let role = match (kind, velux) {
                    ("NXG", _) => Role::Gateway,
                    ("NXO", Some("window")) => Role::Window,
                    ("NXO", _) => Role::Cover,
                    ("NXS", _) => Role::Sensor,
                    ("NXD", _) => Role::Departure,
                    _ => return None,
                };
                let room_id = m["room_id"].as_str().map(str::to_owned);
                Some(Module {
                    id: m["id"].as_str()?.to_owned(),
                    role,
                    name: m["name"].as_str().unwrap_or(kind).trim().to_owned(),
                    room: room_id.as_deref().and_then(room_name),
                    room_id,
                    model: velux.unwrap_or(kind).to_owned(),
                    bridge: m["bridge"].as_str().map(str::to_owned),
                })
            })
            .collect();
        Some(Self {
            id: home["id"].as_str()?.to_owned(),
            timezone: home["timezone"].as_str().unwrap_or("UTC").to_owned(),
            modules,
        })
    }

    /// The whole `setstate` body for these module orders, as the app sends it.
    #[must_use]
    pub fn setstate(&self, modules: &[Json]) -> Json {
        json!({
            "app_type": "app_velux",
            "app_version": crate::cloud::APP_VERSION,
            "home": { "id": self.id, "timezone": self.timezone, "modules": modules },
        })
    }
}

/// The gateway's orders for a whole family of modules at once, as the app's
/// « all » buttons: one `setstate` for all of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    /// Every blind, awning or shutter (`blinds`).
    Covers,
    /// Every roof window Moli can move (`windows`).
    Windows,
}

impl Group {
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Self::Covers => "blinds",
            Self::Windows => "windows",
        }
    }

    #[must_use]
    pub fn of_key(key: &str) -> Option<Self> {
        match key {
            "blinds" => Some(Self::Covers),
            "windows" => Some(Self::Windows),
            _ => None,
        }
    }

    fn role(self) -> Role {
        match self {
            Self::Covers => Role::Cover,
            Self::Windows => Role::Window,
        }
    }
}

impl Home {
    /// The group orders this home has: its blinds always, its windows with
    /// the gateway's key (`signed`). Write-only, open, stop or close; a
    /// person's in a guarded room too, so an agent asks first.
    #[must_use]
    pub fn group_points(&self, signed: bool) -> Vec<PointSpec> {
        [
            (Group::Covers, "pilotes.velux.tous_stores"),
            (Group::Windows, "pilotes.velux.toutes_fenetres"),
        ]
        .into_iter()
        .filter(|(group, _)| {
            self.modules
                .iter()
                .any(|m| m.role == group.role() && m.movable(signed))
        })
        .map(|(group, label)| {
            let mut control = spec(
                group.key(),
                label,
                Kind::Enum {
                    values: ["open", "stop", "close"].map(Arc::from).to_vec(),
                },
                None,
                Semantic::Control,
            );
            control.access = Access {
                read: false,
                write: true,
            };
            control
        })
        .collect()
    }

    /// The `setstate` modules for `group` going to `target`: every one of
    /// them (a window's position signed, each with its own nonce), or the
    /// gateway alone for windows to stop.
    pub fn group_order(
        &self,
        group: Group,
        target: Target,
        mut signer: Option<&mut Signer>,
        now: u64,
    ) -> Result<Vec<Json>, String> {
        let members: Vec<&Module> = self
            .modules
            .iter()
            .filter(|m| m.role == group.role())
            .collect();
        if members.is_empty() {
            return Err(moli_i18n::tr("pilotes.velux.module_inconnu"));
        }
        if group == Group::Windows && target == Target::Stop {
            // One stop for the gateway stops every window.
            return members[0].order(Target::Stop, None, now).map(|o| vec![o]);
        }
        members
            .into_iter()
            .map(|m| m.order(target, signer.as_deref_mut(), now))
            .collect()
    }
}

/// A module's state in one `homestatus`, `None` when it is not there (the
/// gateway unreachable: the cloud lists it among errors).
fn state<'a>(status: &'a Json, id: &str) -> Option<&'a Json> {
    status["modules"].as_array()?.iter().find(|m| m["id"] == id)
}

fn room<'a>(status: &'a Json, id: Option<&str>) -> Option<&'a Json> {
    status["rooms"]
        .as_array()?
        .iter()
        .find(|r| id.is_some_and(|id| r["id"] == id))
}

/// A measure of the sensor's room.
struct Measure {
    /// Its name in `homestatus`.
    cloud: &'static str,
    key: &'static str,
    label: &'static str,
    unit: Option<Unit>,
    semantic: Semantic,
    /// Sent in tenths (the temperature: 191 is 19.1 °C).
    tenths: bool,
}

const MEASURES: [Measure; 5] = [
    Measure {
        cloud: "temperature",
        key: "temperature",
        label: "pilotes.velux.temperature",
        unit: Some(Unit::Celsius),
        semantic: Semantic::Temperature,
        tenths: true,
    },
    Measure {
        cloud: "humidity",
        key: "humidity",
        label: "pilotes.velux.humidite",
        unit: Some(Unit::Percent),
        semantic: Semantic::Humidity,
        tenths: false,
    },
    Measure {
        cloud: "co2",
        key: "co2",
        label: "pilotes.velux.co2",
        unit: Some(Unit::Ppm),
        semantic: Semantic::Other,
        tenths: false,
    },
    Measure {
        cloud: "lux",
        key: "illuminance",
        label: "pilotes.velux.luminosite",
        unit: Some(Unit::Lux),
        semantic: Semantic::Illuminance,
        tenths: false,
    },
    Measure {
        cloud: "air_quality",
        key: "air_quality",
        label: "pilotes.velux.qualite_air",
        unit: None,
        semantic: Semantic::Other,
        tenths: false,
    },
];

impl Module {
    /// Whether Moli can move it: a blind always, a window with the
    /// gateway's key (`signed`).
    #[must_use]
    pub fn movable(&self, signed: bool) -> bool {
        match self.role {
            Role::Cover => true,
            Role::Window => signed,
            _ => false,
        }
    }

    /// The points this module shows, from what its first state carries;
    /// `signed`: the gateway's key is known (windows can be moved).
    #[must_use]
    pub fn points(&self, status: &Json, signed: bool) -> Vec<PointSpec> {
        let own = state(status, &self.id);
        let has = |key: &str| own.is_some_and(|s| !s[key].is_null());
        let mut out = Vec::new();
        match self.role {
            Role::Gateway => {
                out.push(spec(
                    "rain",
                    "pilotes.velux.pluie",
                    Kind::Binary,
                    None,
                    Semantic::Other,
                ));
                // Locked (away), nothing moves; unlocked, windows may open
                // again: an agent asks a person first, as for a window.
                let mut departure = spec(
                    "departure",
                    "pilotes.velux.depart",
                    Kind::Binary,
                    None,
                    Semantic::Control,
                );
                departure.access.write = true;
                out.push(departure);
            }
            Role::Window | Role::Cover => {
                let write = self.movable(signed);
                // Opening a roof window lets the rain and anyone in: an agent
                // asks a person first, as for a machine.
                let semantic = if self.role == Role::Window {
                    Semantic::Control
                } else {
                    Semantic::Other
                };
                let mut position = percent("percent_control", "pilotes.velux.ouverture", semantic);
                position.access.write = write;
                out.push(position);
                // Where it is heading (its position is where it is now).
                out.push(percent(
                    "target_position",
                    "pilotes.velux.cible",
                    Semantic::Other,
                ));
                if write {
                    let mut control = spec(
                        "control",
                        "pilotes.velux.commande",
                        Kind::Enum {
                            values: ["open", "stop", "close"].map(Arc::from).to_vec(),
                        },
                        None,
                        semantic,
                    );
                    control.access = Access {
                        read: false,
                        write: true,
                    };
                    out.push(control);
                }
                out.push(spec(
                    "moving",
                    "pilotes.velux.en_mouvement",
                    Kind::Binary,
                    None,
                    Semantic::Other,
                ));
                if has("battery_state") {
                    out.push(spec(
                        "battery_low",
                        "pilotes.velux.pile_faible",
                        Kind::Binary,
                        None,
                        Semantic::BatteryLow,
                    ));
                }
            }
            Role::Sensor => {
                let measured = room(status, self.room_id.as_deref());
                for m in &MEASURES {
                    if measured.is_some_and(|r| r[m.cloud].is_number()) {
                        out.push(spec(m.key, m.label, number(), m.unit.clone(), m.semantic));
                    }
                }
            }
            Role::Departure => {}
        }
        if matches!(self.role, Role::Sensor | Role::Departure) && has("battery_percent") {
            out.push(spec(
                "battery",
                "pilotes.velux.pile",
                number(),
                Some(Unit::Percent),
                Semantic::Battery,
            ));
        }
        out
    }

    /// The module's values in one `homestatus`, and whether it answers.
    /// `None`: it is not in this status at all.
    #[must_use]
    pub fn values(&self, status: &Json) -> Option<(bool, Vec<(&'static str, Value)>)> {
        let own = state(status, &self.id)?;
        let mut out = Vec::new();
        let flag = |v: &Json| v.as_bool().map(Value::from);
        match self.role {
            Role::Gateway => {
                out.extend(flag(&own["is_raining"]).map(|v| ("rain", v)));
                out.extend(flag(&own["locked"]).map(|v| ("departure", v)));
            }
            Role::Window | Role::Cover => {
                let now = own["current_position"].as_i64();
                // −1 after a stop: no target then, and nothing moving.
                let target = own["target_position"]
                    .as_i64()
                    .filter(|t| (0..=100).contains(t));
                out.extend(now.map(|p| ("percent_control", Value::from(p))));
                if let Some(now) = now {
                    out.push(("moving", Value::from(target.is_some_and(|t| t != now))));
                }
                out.extend(target.map(|t| ("target_position", Value::from(t))));
                if let Some(battery) = own["battery_state"].as_str() {
                    let low = matches!(battery, "low" | "very_low");
                    out.push(("battery_low", Value::from(low)));
                }
            }
            Role::Sensor => {
                if let Some(measured) = room(status, self.room_id.as_deref()) {
                    for m in &MEASURES {
                        let v = &measured[m.cloud];
                        let value = match (m.tenths, v.as_i64(), v.as_f64()) {
                            (false, Some(n), _) => Value::from(n),
                            (true, _, Some(t)) => Value::from((t * 10.0).round() / 100.0),
                            (false, None, Some(x)) => Value::from(x),
                            _ => continue,
                        };
                        out.push((m.key, value));
                    }
                }
            }
            Role::Departure => {}
        }
        if matches!(self.role, Role::Sensor | Role::Departure) {
            out.extend(
                own["battery_percent"]
                    .as_i64()
                    .map(|b| ("battery", Value::from(b))),
            );
        }
        // The gateway's own state has no `reachable`: there, it answers.
        let answers = own["reachable"].as_bool().unwrap_or(true);
        Some((answers, out))
    }
}

impl Module {
    /// Where a command on `key` sends this module.
    pub fn target(&self, key: &str, value: &Value) -> Result<Target, String> {
        match (key, value) {
            ("percent_control", v) => v
                .as_f64()
                .filter(|p| (0.0..=100.0).contains(p))
                .map(|p| {
                    #[allow(clippy::cast_possible_truncation)]
                    Target::Position(p.round() as i64)
                })
                .ok_or_else(|| moli_i18n::tr("pilotes.velux.position_invalide")),
            ("control" | "blinds" | "windows", Value::Text(t)) => match &**t {
                "open" => Ok(Target::Position(100)),
                "close" => Ok(Target::Position(0)),
                "stop" => Ok(Target::Stop),
                _ => Err(moli_i18n::tr("pilotes.velux.position_invalide")),
            },
            _ => Err(moli_i18n::tr("pilotes.velux.lecture_seule")),
        }
    }

    /// The gateway's order to lock (`away`: departure mode, nothing moves)
    /// or unlock (`home`): the app's scenarios. Locking needs no key;
    /// unlocking lets windows open again, so it is signed like them.
    pub fn departure(
        &self,
        locked: &Value,
        signer: Option<&mut Signer>,
        now: u64,
    ) -> Result<Json, String> {
        if self.role != Role::Gateway {
            return Err(moli_i18n::tr("pilotes.velux.lecture_seule"));
        }
        match locked {
            Value::Bool(true) => Ok(json!({ "id": self.id, "scenario": "away" })),
            Value::Bool(false) => {
                let signer =
                    signer.ok_or_else(|| moli_i18n::tr("pilotes.velux.sans_cle_depart"))?;
                let stamp = signer.sign_scenario(&self.id, "home", now);
                Ok(json!({
                    "id": self.id,
                    "scenario": "home",
                    "timestamp": stamp.timestamp,
                    "nonce": stamp.nonce,
                    "sign_key_id": signer.id,
                    "hash_scenario": stamp.hash,
                }))
            }
            _ => Err(moli_i18n::tr("pilotes.velux.depart_invalide")),
        }
    }

    /// The `setstate` module entry for `target`: the module itself, or the
    /// gateway for a window to stop. A window's position is signed (`signer`
    /// at `now`, seconds since 1970); without a signer it is refused.
    pub fn order(
        &self,
        target: Target,
        signer: Option<&mut Signer>,
        now: u64,
    ) -> Result<Json, String> {
        let bridge = self
            .bridge
            .as_deref()
            .ok_or_else(|| moli_i18n::tr("pilotes.velux.sans_passerelle"))?;
        match (self.role, target) {
            (Role::Cover, Target::Position(p)) => {
                Ok(json!({ "id": self.id, "bridge": bridge, "target_position": p }))
            }
            (Role::Cover, Target::Stop) => {
                Ok(json!({ "id": self.id, "bridge": bridge, "target_position": STOP }))
            }
            (Role::Window, Target::Stop) => Ok(json!({ "id": bridge, "stop_movements": "all" })),
            (Role::Window, Target::Position(p)) => {
                let signer = signer.ok_or_else(|| moli_i18n::tr("pilotes.velux.sans_cle"))?;
                let stamp = signer.sign(&self.id, p, now);
                Ok(json!({
                    "id": self.id,
                    "bridge": bridge,
                    "target_position": p,
                    "timestamp": stamp.timestamp,
                    "nonce": stamp.nonce,
                    "sign_key_id": signer.id,
                    "hash_target_position": stamp.hash,
                }))
            }
            _ => Err(moli_i18n::tr("pilotes.velux.lecture_seule")),
        }
    }
}

fn number() -> Kind {
    Kind::Numeric {
        min: None,
        max: None,
        step: None,
    }
}

/// A position, 0 (closed) to 100 (open).
fn percent(key: &str, label: &str, semantic: Semantic) -> PointSpec {
    let kind = Kind::Numeric {
        min: Some(0.0),
        max: Some(100.0),
        step: Some(1.0),
    };
    spec(key, label, kind, Some(Unit::Percent), semantic)
}

fn spec(key: &str, label: &str, kind: Kind, unit: Option<Unit>, semantic: Semantic) -> PointSpec {
    PointSpec {
        key: Arc::from(key),
        label: moli_i18n::tr(label).into(),
        kind,
        access: Access {
            read: true,
            write: false,
        },
        unit,
        semantic,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real KIX 300 home (gateway, 7 windows, 8 blinds, a sensor, a
    /// departure switch), ids made up.
    fn kix300() -> (Home, Json) {
        let text = include_str!("../../../integrations/velux/fixtures/kix300.json");
        let file: Json = serde_json::from_str(text).unwrap();
        (
            Home::parse(&file["homesdata"]).unwrap(),
            file["homestatus"].clone(),
        )
    }

    fn module<'a>(home: &'a Home, name: &str) -> &'a Module {
        home.modules.iter().find(|m| m.name == name).unwrap()
    }

    fn value(m: &Module, status: &Json, key: &str) -> Option<Value> {
        m.values(status)?
            .1
            .into_iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v)
    }

    #[test]
    fn every_module_has_its_role() {
        let (home, _) = kix300();
        let count = |r| home.modules.iter().filter(|m| m.role == r).count();
        assert_eq!(home.modules.len(), 18);
        assert_eq!(count(Role::Gateway), 1);
        assert_eq!(count(Role::Window), 7);
        assert_eq!(count(Role::Cover), 8);
        assert_eq!(count(Role::Sensor), 1);
        assert_eq!(count(Role::Departure), 1);
    }

    #[test]
    fn a_module_keeps_its_velux_room() {
        let (home, _) = kix300();
        assert_eq!(
            module(&home, "Store Chambre").room.as_deref(),
            Some("Chambre")
        );
        assert_eq!(
            module(&home, "Fenêtre grenier").room.as_deref(),
            Some("Grenier")
        );
        assert_eq!(module(&home, "Commande de départ").room, None, "in no room");
    }

    #[test]
    fn positions_go_from_closed_to_open() {
        let (home, status) = kix300();
        let open = module(&home, "Mezzanine Droite");
        assert_eq!(
            value(open, &status, "percent_control"),
            Some(Value::Int(61))
        );
        assert_eq!(value(open, &status, "moving"), Some(Value::Bool(false)));
        let blind = module(&home, "Store grenier");
        assert_eq!(
            value(blind, &status, "percent_control"),
            Some(Value::Int(18))
        );
        let closed = module(&home, "Store Chambre");
        assert_eq!(
            value(closed, &status, "percent_control"),
            Some(Value::Int(0))
        );
    }

    #[test]
    fn a_module_on_its_way_is_moving() {
        let (home, mut status) = kix300();
        let blind = module(&home, "Store Chambre");
        for m in status["modules"].as_array_mut().unwrap() {
            if m["id"] == blind.id.as_str() {
                m["target_position"] = 100.into();
            }
        }
        assert_eq!(value(blind, &status, "moving"), Some(Value::Bool(true)));
        assert_eq!(
            value(blind, &status, "target_position"),
            Some(Value::Int(100))
        );
        for m in status["modules"].as_array_mut().unwrap() {
            if m["id"] == blind.id.as_str() {
                m["target_position"] = (-1).into();
            }
        }
        assert_eq!(
            value(blind, &status, "target_position"),
            None,
            "stopped: no target"
        );
        assert_eq!(
            value(blind, &status, "moving"),
            Some(Value::Bool(false)),
            "stopped"
        );
    }

    #[test]
    fn the_sensor_reads_its_room() {
        let (home, status) = kix300();
        let sensor = home
            .modules
            .iter()
            .find(|m| m.role == Role::Sensor)
            .unwrap();
        let keys: Vec<_> = sensor
            .points(&status, false)
            .iter()
            .map(|p| p.key.to_string())
            .collect();
        assert_eq!(
            keys,
            [
                "temperature",
                "humidity",
                "co2",
                "illuminance",
                "air_quality",
                "battery"
            ]
        );
        assert_eq!(
            value(sensor, &status, "temperature"),
            Some(Value::Float(19.1))
        );
        assert_eq!(value(sensor, &status, "co2"), Some(Value::Int(694)));
        assert_eq!(value(sensor, &status, "battery"), Some(Value::Int(76)));
    }

    #[test]
    fn the_gateway_says_rain_and_departure() {
        let (home, status) = kix300();
        let gateway = home
            .modules
            .iter()
            .find(|m| m.role == Role::Gateway)
            .unwrap();
        let (answers, values) = gateway.values(&status).unwrap();
        assert!(answers);
        assert_eq!(
            values,
            [
                ("rain", Value::Bool(false)),
                ("departure", Value::Bool(false))
            ]
        );
    }

    fn writable(m: &Module, status: &Json, signed: bool) -> Vec<String> {
        m.points(status, signed)
            .iter()
            .filter(|p| p.access.write)
            .map(|p| p.key.to_string())
            .collect()
    }

    #[test]
    fn blinds_move_and_windows_need_the_key() {
        let (home, status) = kix300();
        for m in &home.modules {
            let (without, with) = (writable(m, &status, false), writable(m, &status, true));
            match m.role {
                Role::Cover => {
                    assert_eq!(without, ["percent_control", "control"], "{}", m.name);
                    assert_eq!(with, without);
                }
                Role::Window => {
                    assert!(without.is_empty(), "{}", m.name);
                    assert_eq!(with, ["percent_control", "control"], "{}", m.name);
                }
                Role::Gateway => {
                    assert_eq!(without, ["departure"]);
                    assert_eq!(with, without);
                }
                _ => assert!(with.is_empty(), "{}", m.name),
            }
        }
    }

    fn gateway(home: &Home) -> &Module {
        home.modules
            .iter()
            .find(|m| m.role == Role::Gateway)
            .unwrap()
    }

    #[test]
    fn the_gateway_moves_all_blinds_and_with_the_key_all_windows() {
        let (home, _) = kix300();
        let keys = |signed| {
            home.group_points(signed)
                .iter()
                .map(|p| (p.key.to_string(), p.access.write, p.access.read, p.semantic))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            keys(false),
            [("blinds".into(), true, false, Semantic::Control)]
        );
        assert_eq!(
            keys(true),
            [
                ("blinds".into(), true, false, Semantic::Control),
                ("windows".into(), true, false, Semantic::Control)
            ]
        );
    }

    #[test]
    fn a_group_order_is_one_setstate_for_all() {
        let (home, _) = kix300();
        let text = |t: &str| Value::Text(t.into());
        let target = gateway(&home).target("blinds", &text("close")).unwrap();
        let blinds = home.group_order(Group::Covers, target, None, 0).unwrap();
        assert_eq!(blinds.len(), 8);
        assert!(
            blinds
                .iter()
                .all(|o| o["target_position"] == 0 && o["bridge"].is_string())
        );

        assert!(
            home.group_order(Group::Windows, Target::Position(100), None, 0)
                .is_err(),
            "windows without the key"
        );
        let mut signer =
            Signer::new("AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=", "key-id").unwrap();
        let windows = home
            .group_order(
                Group::Windows,
                Target::Position(100),
                Some(&mut signer),
                1_700_000_000,
            )
            .unwrap();
        assert_eq!(windows.len(), 7);
        // Each signed with its own nonce: the cloud refuses a pair twice.
        let nonces: Vec<_> = windows
            .iter()
            .map(|o| o["nonce"].as_u64().unwrap())
            .collect();
        assert_eq!(nonces, [0, 1, 2, 3, 4, 5, 6]);
        assert!(
            windows
                .iter()
                .all(|o| o["hash_target_position"].is_string())
        );

        let stop = home
            .group_order(Group::Windows, Target::Stop, None, 0)
            .unwrap();
        assert_eq!(
            stop,
            [json!({ "id": "aa:bb:cc:dd:ee:01", "stop_movements": "all" })]
        );
    }

    #[test]
    fn the_departure_mode_locks_freely_and_unlocks_with_the_key() {
        let (home, _) = kix300();
        let gw = gateway(&home);
        assert_eq!(
            gw.departure(&Value::Bool(true), None, 0).unwrap(),
            json!({ "id": "aa:bb:cc:dd:ee:01", "scenario": "away" })
        );
        assert!(
            gw.departure(&Value::Bool(false), None, 0).is_err(),
            "no key"
        );
        let mut signer =
            Signer::new("AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=", "key-id").unwrap();
        let home_again = gw
            .departure(&Value::Bool(false), Some(&mut signer), 1_700_000_000)
            .unwrap();
        assert_eq!(home_again["scenario"], "home");
        assert_eq!(home_again["sign_key_id"], "key-id");
        assert_eq!(home_again["nonce"], 0);
        assert!(home_again["hash_scenario"].is_string());
        assert!(home_again.get("bridge").is_none(), "the gateway itself");
        assert!(gw.departure(&Value::Int(1), None, 0).is_err());
        let blind = module(&home, "Store grenier");
        assert!(blind.departure(&Value::Bool(true), None, 0).is_err());
    }

    #[test]
    fn a_window_is_a_machine_for_agents_a_blind_is_not() {
        let (home, status) = kix300();
        let semantic = |name: &str| {
            module(&home, name)
                .points(&status, true)
                .into_iter()
                .find(|p| &*p.key == "control")
                .unwrap()
                .semantic
        };
        assert_eq!(semantic("Salon"), Semantic::Control);
        assert_eq!(semantic("Store Salon"), Semantic::Other);
    }

    #[test]
    fn commands_become_targets() {
        let (home, _) = kix300();
        let blind = module(&home, "Store grenier");
        let text = |t: &str| Value::Text(t.into());
        assert_eq!(
            blind.target("control", &text("open")),
            Ok(Target::Position(100))
        );
        assert_eq!(
            blind.target("control", &text("close")),
            Ok(Target::Position(0))
        );
        assert_eq!(blind.target("control", &text("stop")), Ok(Target::Stop));
        assert_eq!(
            blind.target("percent_control", &Value::Float(42.4)),
            Ok(Target::Position(42))
        );
        assert!(
            blind
                .target("percent_control", &Value::Float(120.0))
                .is_err()
        );
        assert!(blind.target("control", &text("up")).is_err());
        assert!(blind.target("moving", &Value::Bool(true)).is_err());
    }

    #[test]
    fn a_blind_order_is_the_apps() {
        let (home, _) = kix300();
        let blind = module(&home, "Store grenier");
        let one = blind.order(Target::Position(30), None, 0).unwrap();
        assert_eq!(
            home.setstate(&[one]),
            json!({
                "app_type": "app_velux",
                "app_version": "791302006",
                "home": {
                    "id": "0123456789abcdef01234567",
                    "timezone": "Europe/Paris",
                    "modules": [
                        { "id": blind.id, "bridge": "aa:bb:cc:dd:ee:01", "target_position": 30 }
                    ],
                },
            })
        );
        let stop = blind.order(Target::Stop, None, 0).unwrap();
        assert_eq!(stop["target_position"], -1);
    }

    #[test]
    fn a_window_order_is_signed_and_its_stop_goes_to_the_gateway() {
        let (home, _) = kix300();
        let window = module(&home, "Mezzanine - Gauche");
        assert!(
            window.order(Target::Position(100), None, 0).is_err(),
            "no key"
        );
        let mut signer =
            Signer::new("AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=", "key-id").unwrap();
        let open = window
            .order(Target::Position(100), Some(&mut signer), 1_700_000_000)
            .unwrap();
        assert_eq!(
            open,
            json!({
                "id": "aa:bb:cc:dd:ee:04",
                "bridge": "aa:bb:cc:dd:ee:01",
                "target_position": 100,
                "timestamp": 1_700_000_000,
                "nonce": 0,
                "sign_key_id": "key-id",
                "hash_target_position": "jz-e9vYUwvJcmH6GrXdmSSaUwzDNOfKaq2w1X-92RZrAE65KaATH3UFo3d-qJbhaW7yR8_ESTmR-6TbmvmrchQ==",
            })
        );
        assert_eq!(
            window.order(Target::Stop, None, 0).unwrap(),
            json!({ "id": "aa:bb:cc:dd:ee:01", "stop_movements": "all" })
        );
    }

    #[test]
    fn a_window_without_battery_has_no_battery_point() {
        let (home, status) = kix300();
        let mains = module(&home, "Mezzanine Droite");
        assert!(
            mains
                .points(&status, false)
                .iter()
                .all(|p| &*p.key != "battery_low")
        );
        let solar = module(&home, "Salon");
        assert!(
            solar
                .points(&status, false)
                .iter()
                .any(|p| &*p.key == "battery_low")
        );
        assert_eq!(
            value(solar, &status, "battery_low"),
            Some(Value::Bool(false))
        );
    }

    #[test]
    fn an_unreachable_module_says_so() {
        let (home, mut status) = kix300();
        let blind = module(&home, "Store grenier");
        for m in status["modules"].as_array_mut().unwrap() {
            if m["id"] == blind.id.as_str() {
                m["reachable"] = false.into();
            }
        }
        assert!(!blind.values(&status).unwrap().0);
        status["modules"] = Json::Array(Vec::new());
        assert!(blind.values(&status).is_none(), "missing from the status");
    }
}
