//! `omakboard serve`: the overlay's connection to the keyboard.
//!
//! One JSON request per line on stdin, one JSON response per line on stdout, over a single
//! keyboard session, so the overlay never waits for a process to start.
//!
//! ```text
//! → {"id": 1, "cmd": "state"}
//! → {"id": 2, "cmd": "apply", "triggers": [{"code": "KeyW", "trigger": {"mode": "rapid", "travel": 120, "press": 30, "release": 30}}]}
//! → {"id": 3, "cmd": "apply", "lighting": {"mode": 2, "brightness": 3, ...}}
//! → {"id": 4, "cmd": "undo"}
//! ← {"id": 2, "ok": true, "result": {"protocol": 1, "keyboard": {…}, "triggers": {…}, …}}
//! ← {"id": 4, "ok": false, "error": "…"}
//! ```
//!
//! Every answer carries the whole state, so the overlay replaces what it shows instead of
//! patching it. Writes keep the CLI's guarantees: everything is read fresh first, all of it
//! is backed up before the session's first write, only differences are sent, and each write
//! is read back. What a write replaced is kept, so `undo` puts it back the same way.
//! The device lock is held for each request, never while idle.

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::apply::{self, KeyChange};
use crate::hidraw::Transport;
use crate::layout::Key;
use crate::lock::DeviceLock;
use crate::proto::aula_ff1b::{self as aula, Lighting, Trigger, TriggerMode};
use crate::registry::{Model, Support};
use crate::session::Session;
use crate::snapshot::{self, Info, KeyTrigger};

/// The request format's version. The plugin asks for a helper update when this is older
/// than it needs.
pub const PROTOCOL: u32 = 1;

/// Opens the keyboard; the real one finds it through sysfs, tests use a simulation.
pub trait Opener {
    type Transport: Transport;
    fn open(&mut self) -> Result<(&'static Model, Self::Transport), String>;
}

#[derive(Debug, Deserialize)]
struct Request {
    id: u64,
    #[serde(flatten)]
    command: Command,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
enum Command {
    State,
    Apply {
        #[serde(default)]
        triggers: Vec<TriggerSet>,
        #[serde(default)]
        lighting: Option<Lighting>,
    },
    Undo,
}

#[derive(Debug, Clone, Deserialize)]
struct TriggerSet {
    code: String,
    trigger: WireTrigger,
}

/// A trigger as the overlay sends and receives it: the mode as a plain word.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
struct WireTrigger {
    mode: WireMode,
    travel: u16,
    press: u16,
    release: u16,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum WireMode {
    Fixed,
    Rapid,
    RapidSeparate,
    Other,
}

impl WireTrigger {
    fn from_trigger(t: Trigger) -> Self {
        let mode = match t.mode {
            TriggerMode::Fixed => WireMode::Fixed,
            TriggerMode::RapidTrigger => WireMode::Rapid,
            TriggerMode::RapidTriggerSeparate => WireMode::RapidSeparate,
            TriggerMode::Other(_) => WireMode::Other,
        };
        Self {
            mode,
            travel: t.travel,
            press: t.press,
            release: t.release,
        }
    }

    fn to_trigger(self) -> Result<Trigger, String> {
        let mode = match self.mode {
            WireMode::Fixed => TriggerMode::Fixed,
            WireMode::Rapid => TriggerMode::RapidTrigger,
            WireMode::RapidSeparate => TriggerMode::RapidTriggerSeparate,
            WireMode::Other => return Err("cannot write an unknown trigger mode".to_owned()),
        };
        Ok(Trigger {
            mode,
            travel: self.travel,
            press: self.press,
            release: self.release,
        })
    }
}

/// What one write replaced.
struct UndoEntry {
    triggers: Vec<(String, Trigger)>,
    lighting: Option<Lighting>,
}

struct Device<T: Transport> {
    model: &'static Model,
    session: Session<T>,
    info: Info,
    triggers: Vec<KeyTrigger>,
}

pub struct Server<O: Opener> {
    opener: O,
    device: Option<Device<O::Transport>>,
    undo: Vec<UndoEntry>,
    backup: Option<PathBuf>,
    /// Where the first write's backup goes; tests point it at a temporary file.
    pub backup_path: fn(&Model) -> Result<PathBuf, String>,
    pub lock_timeout: Duration,
    pub lock_path: PathBuf,
}

impl<O: Opener> Server<O> {
    pub fn new(opener: O) -> Self {
        Self {
            opener,
            device: None,
            undo: Vec::new(),
            backup: None,
            backup_path: |model| snapshot::default_backup_path(&model.usb_id()),
            lock_timeout: Duration::from_secs(2),
            lock_path: crate::lock::default_path(),
        }
    }

    /// Answers requests until stdin closes.
    pub fn run(&mut self, input: impl BufRead, mut output: impl Write) -> std::io::Result<()> {
        for line in input.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let response = self.handle_line(&line);
            writeln!(output, "{response}")?;
            output.flush()?;
        }
        Ok(())
    }

    pub fn handle_line(&mut self, line: &str) -> Value {
        let request: Request = match serde_json::from_str(line) {
            Ok(r) => r,
            Err(e) => {
                let id = serde_json::from_str::<Value>(line)
                    .ok()
                    .and_then(|v| v.get("id").cloned())
                    .unwrap_or(Value::Null);
                return json!({ "id": id, "ok": false, "error": format!("bad request: {e}") });
            }
        };
        match self.handle(request.command) {
            Ok(result) => json!({ "id": request.id, "ok": true, "result": result }),
            Err(error) => json!({ "id": request.id, "ok": false, "error": error }),
        }
    }

    fn handle(&mut self, command: Command) -> Result<Value, String> {
        let _lock = DeviceLock::acquire_at(&self.lock_path, self.lock_timeout)
            .map_err(|e| e.to_string())?;
        let result = match command {
            Command::State => self.refresh().map(|()| Vec::new()),
            Command::Apply { triggers, lighting } => self.apply(triggers, lighting, true),
            Command::Undo => self.undo(),
        };
        match result {
            Ok(changed) => Ok(self.state(changed)),
            Err(e) => {
                // A failed exchange may mean the keyboard went away; open it afresh next time.
                self.device = None;
                Err(e)
            }
        }
    }

    /// Reads everything from the keyboard, opening it first when needed.
    fn refresh(&mut self) -> Result<(), String> {
        let device = match self.device.take() {
            Some(d) => d,
            None => {
                let (model, transport) = self.opener.open()?;
                let session = Session::new(transport);
                Device {
                    model,
                    session,
                    info: placeholder_info(),
                    triggers: Vec::new(),
                }
            }
        };
        let mut device = device;
        device.info =
            snapshot::read_info(&mut device.session, device.model).map_err(|e| e.to_string())?;
        device.triggers = snapshot::read_triggers(&mut device.session, device.model.layout)
            .map_err(|e| e.to_string())?;
        self.device = Some(device);
        Ok(())
    }

    fn apply(
        &mut self,
        sets: Vec<TriggerSet>,
        lighting: Option<Lighting>,
        remember: bool,
    ) -> Result<Vec<String>, String> {
        self.refresh()?;
        let device = self.device.as_mut().expect("refreshed");
        let model = device.model;
        if model.support != Support::Verified {
            return Err(format!("{} is not verified for changes yet", model.name));
        }
        let caps = model.backend.capabilities();
        let range = device.info.trigger_range;

        let mut targets: Vec<(Key, Trigger)> = Vec::new();
        for set in &sets {
            let key = model
                .layout
                .iter()
                .find(|k| k.code == set.code)
                .ok_or_else(|| format!("no key {} on the {}", set.code, model.name))?;
            targets.push((*key, set.trigger.to_trigger()?));
        }
        let changes: Vec<KeyChange> = apply::trigger_changes(&device.triggers, model.layout, |k| {
            targets
                .iter()
                .find(|(key, _)| key.index == k.index)
                .map(|(_, t)| *t)
        });
        // Compared in the form the firmware stores, so asking for what it already keeps sends nothing.
        let lighting_change = lighting
            .map(|l| aula::stored_lighting(&l))
            .filter(|l| *l != device.info.lighting);

        // Check everything before sending anything.
        for c in &changes {
            let rapid_changed = c.from.mode != c.to.mode
                || c.from.press != c.to.press
                || c.from.release != c.to.release;
            if (c.from.travel != c.to.travel && !caps.actuation)
                || (rapid_changed && !caps.rapid_trigger)
            {
                return Err(format!(
                    "Omakboard cannot change that on the {} yet",
                    model.name
                ));
            }
        }
        apply::trigger_frames(&changes, &range).map_err(|e| e.to_string())?;
        if let Some(l) = &lighting_change {
            if !caps.lighting {
                return Err(format!(
                    "Omakboard cannot change lighting on the {} yet",
                    model.name
                ));
            }
            aula::validate_lighting(l, &device.info.lighting_modes, device.info.lighting.mode)
                .map_err(|e| e.to_string())?;
        }
        if changes.is_empty() && lighting_change.is_none() {
            return Ok(Vec::new());
        }

        if self.backup.is_none() {
            let path = (self.backup_path)(model)?;
            snapshot::save_backup(
                &path,
                model,
                &device.info,
                &device.triggers,
                &device.session.trace,
            )?;
            self.backup = Some(path);
        }
        if remember {
            self.undo.push(UndoEntry {
                triggers: changes
                    .iter()
                    .map(|c| (c.key.code.to_owned(), c.from))
                    .collect(),
                lighting: lighting_change.map(|_| device.info.lighting),
            });
        }

        let mut changed: Vec<String> = changes.iter().map(|c| c.key.label.to_owned()).collect();
        if !changes.is_empty() {
            apply::apply_triggers(&mut device.session, &changes, &range)
                .map_err(|e| e.to_string())?;
            for c in &changes {
                if let Some(k) = device.triggers.iter_mut().find(|k| k.index == c.key.index) {
                    k.trigger = c.to;
                }
            }
        }
        if let Some(l) = lighting_change {
            let from = device.info.lighting;
            device.info.lighting =
                apply::apply_lighting(&mut device.session, &device.info.lighting_modes, &from, &l)
                    .map_err(|e| e.to_string())?;
            changed.push("lighting".to_owned());
        }
        Ok(changed)
    }

    fn undo(&mut self) -> Result<Vec<String>, String> {
        let Some(entry) = self.undo.pop() else {
            return Err("nothing to undo".to_owned());
        };
        let sets = entry
            .triggers
            .iter()
            .map(|(code, t)| TriggerSet {
                code: code.clone(),
                trigger: WireTrigger::from_trigger(*t),
            })
            .collect();
        match self.apply(sets, entry.lighting, false) {
            Ok(changed) => Ok(changed),
            Err(e) => {
                // Keep it, so Undo can be tried again.
                self.undo.push(entry);
                Err(e)
            }
        }
    }

    fn state(&self, changed: Vec<String>) -> Value {
        let Some(d) = &self.device else {
            return json!(null);
        };
        let info = &d.info;
        let range = &info.trigger_range;
        let layout = d.model.layout;
        let width = layout.iter().map(|k| k.x + k.width).max().unwrap_or(0)
            + layout.iter().map(|k| k.x).min().unwrap_or(0);
        let height = layout.iter().map(|k| k.y + k.height).max().unwrap_or(0)
            + layout.iter().map(|k| k.y).min().unwrap_or(0);
        let triggers: serde_json::Map<String, Value> = d
            .triggers
            .iter()
            .map(|k| {
                (
                    k.code.to_owned(),
                    json!(WireTrigger::from_trigger(k.trigger)),
                )
            })
            .collect();
        json!({
            "protocol": PROTOCOL,
            "keyboard": {
                "name": d.model.name,
                "usb_id": d.model.usb_id(),
                "product": info.identity.chip(),
                "firmware": info.identity.firmware(),
                "built": info.identity.built,
                "poll_rate_hz": info.poll_rate,
                "support": d.model.support,
                "capabilities": d.model.backend.capabilities(),
            },
            "limits": {
                "travel_min": range.min,
                "travel_max": range.max,
                "unit_um": range.unit_um,
                "sensitivity_max": range.max_sensitivity(),
                "sensitivity_default": apply::default_sensitivity(range),
                "brightness_max": info.lighting_modes.max_brightness,
                "speed_max": info.lighting_modes.max_speed,
            },
            "modes": info.lighting_modes.offered().iter().map(|&id| json!({
                "id": id,
                "name": aula::lighting_mode_name(id).map(str::to_owned).unwrap_or_else(|| format!("Mode {id}")),
            })).collect::<Vec<_>>(),
            "layout": {
                "width": width,
                "height": height,
                "keys": layout.iter().map(|k| json!({
                    "code": k.code, "label": k.label, "index": k.index,
                    "x": k.x, "y": k.y, "width": k.width, "height": k.height,
                })).collect::<Vec<_>>(),
            },
            "triggers": triggers,
            "lighting": info.lighting,
            "undo": self.undo.len(),
            "backup": self.backup.as_ref().map(|p| p.display().to_string()),
            "changed": changed,
        })
    }
}

/// Stands in until the first read replaces it.
fn placeholder_info() -> Info {
    use crate::proto::aula_ff1b::{Identity, LightingModes, Rgb, Status, TriggerRange};
    let black = Rgb { r: 0, g: 0, b: 0 };
    Info {
        identity: Identity {
            raw: String::new(),
            built: None,
        },
        status: Status::default(),
        poll_rate: None,
        lighting: Lighting {
            mode: 0,
            brightness: 0,
            speed: 0,
            foreground: black,
            background: black,
            direction: 0,
            full_color: false,
            power: 0,
        },
        lighting_modes: LightingModes {
            modes: Vec::new(),
            max_speed: 0,
            max_brightness: 0,
        },
        trigger_range: TriggerRange {
            min: 0,
            max: 0,
            unit_um: 10,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::lookup;
    use crate::sim::{FACTORY, SimKeyboard};
    use std::cell::RefCell;
    use std::rc::Rc;

    /// Hands out one shared simulated keyboard, so tests can look inside it afterwards.
    #[derive(Clone)]
    struct SharedSim(Rc<RefCell<SimKeyboard>>);

    impl Transport for SharedSim {
        fn send(&mut self, report: &[u8]) -> Result<(), crate::hidraw::TransportError> {
            self.0.borrow_mut().send(report)
        }
        fn recv(&mut self, t: Duration) -> Result<Option<Vec<u8>>, crate::hidraw::TransportError> {
            self.0.borrow_mut().recv(t)
        }
    }

    struct SimOpener {
        sim: SharedSim,
        opens: usize,
        present: bool,
    }

    impl Opener for &mut SimOpener {
        type Transport = SharedSim;
        fn open(&mut self) -> Result<(&'static Model, SharedSim), String> {
            if !self.present {
                return Err("no supported keyboard is plugged in".to_owned());
            }
            self.opens += 1;
            Ok((lookup(0x2E3C, 0xC365).unwrap(), self.sim.clone()))
        }
    }

    fn opener() -> SimOpener {
        SimOpener {
            sim: SharedSim(Rc::new(RefCell::new(SimKeyboard::default()))),
            opens: 0,
            present: true,
        }
    }

    fn server(o: &mut SimOpener) -> Server<&mut SimOpener> {
        let mut s = Server::new(o);
        s.backup_path = |_| {
            Ok(std::env::temp_dir()
                .join(format!("omakboard-serve-test-{}.json", std::process::id())))
        };
        s.lock_timeout = Duration::from_secs(5);
        s.lock_path = std::env::temp_dir().join(format!(
            "omakboard-serve-lock-{}/device.lock",
            std::process::id()
        ));
        s
    }

    fn call(s: &mut Server<&mut SimOpener>, line: &str) -> Value {
        s.handle_line(line)
    }

    #[test]
    fn state_describes_the_keyboard() {
        let mut o = opener();
        let mut s = server(&mut o);
        let r = call(&mut s, r#"{"id": 7, "cmd": "state"}"#);
        assert_eq!(r["id"], 7);
        assert_eq!(r["ok"], true, "{r}");
        let st = &r["result"];
        assert_eq!(st["protocol"], PROTOCOL);
        assert_eq!(st["keyboard"]["name"], "AULA WIN 60 HE");
        assert_eq!(st["keyboard"]["firmware"], "3.13.06");
        assert_eq!(st["keyboard"]["poll_rate_hz"], 8000);
        assert_eq!(st["limits"]["travel_max"], 340);
        assert_eq!(st["limits"]["sensitivity_max"], 170);
        assert_eq!(st["limits"]["sensitivity_default"], 50);
        assert_eq!(st["layout"]["keys"].as_array().unwrap().len(), 61);
        assert_eq!(
            (
                st["layout"]["width"].clone(),
                st["layout"]["height"].clone()
            ),
            (json!(587), json!(217))
        );
        assert_eq!(st["modes"][0], json!({"id": 2, "name": "Wave"}));
        assert_eq!(
            st["triggers"]["KeyW"],
            json!({"mode": "fixed", "travel": 170, "press": 1, "release": 1})
        );
        assert_eq!(st["lighting"]["brightness"], 4);
        assert_eq!(st["undo"], 0);
    }

    #[test]
    fn apply_writes_only_differences_and_undo_puts_them_back() {
        let mut o = opener();
        let sim = o.sim.clone();
        let mut s = server(&mut o);
        let r = call(
            &mut s,
            r#"{"id": 1, "cmd": "apply", "triggers": [
                {"code": "KeyW", "trigger": {"mode": "rapid", "travel": 120, "press": 30, "release": 30}},
                {"code": "KeyA", "trigger": {"mode": "rapid", "travel": 120, "press": 30, "release": 30}},
                {"code": "KeyQ", "trigger": {"mode": "fixed", "travel": 170, "press": 1, "release": 1}}
            ]}"#,
        );
        assert_eq!(r["ok"], true, "{r}");
        assert_eq!(r["result"]["changed"], json!(["W", "A"]));
        assert_eq!(r["result"]["undo"], 1);
        assert!(r["result"]["backup"].is_string());
        // One frame for both keys; Q already matched, so it was not written.
        assert_eq!(sim.0.borrow().writes(), 1);
        let rapid = Trigger {
            mode: TriggerMode::RapidTrigger,
            travel: 120,
            press: 30,
            release: 30,
        };
        assert_eq!(sim.0.borrow().trigger("A"), rapid);
        assert_eq!(r["result"]["triggers"]["KeyW"]["mode"], "rapid");

        let r = call(&mut s, r#"{"id": 2, "cmd": "undo"}"#);
        assert_eq!(r["ok"], true, "{r}");
        assert_eq!(r["result"]["undo"], 0);
        assert_eq!(sim.0.borrow().trigger("W"), FACTORY);
        assert_eq!(sim.0.borrow().trigger("A"), FACTORY);
        assert_eq!(
            call(&mut s, r#"{"id": 3, "cmd": "undo"}"#)["error"],
            "nothing to undo"
        );
    }

    #[test]
    fn lighting_applies_and_undoes() {
        let mut o = opener();
        let sim = o.sim.clone();
        let mut s = server(&mut o);
        let st = call(&mut s, r#"{"id": 1, "cmd": "state"}"#)["result"].clone();
        let mut lighting = st["lighting"].clone();
        lighting["mode"] = json!(8);
        lighting["foreground"] = json!({"r": 0, "g": 160, "b": 255});
        let r = call(
            &mut s,
            &json!({"id": 2, "cmd": "apply", "lighting": lighting}).to_string(),
        );
        assert_eq!(r["ok"], true, "{r}");
        assert_eq!(r["result"]["changed"], json!(["lighting"]));
        assert_eq!(sim.0.borrow().lighting[0], 8);
        call(&mut s, r#"{"id": 3, "cmd": "undo"}"#);
        assert_eq!(sim.0.borrow().lighting[0], 0);
    }

    #[test]
    fn every_offered_effect_applies_through_the_server() {
        let mut o = opener();
        let mut s = server(&mut o);
        let state = call(&mut s, r#"{"id": 1, "cmd": "state"}"#)["result"].clone();
        let ids: Vec<u64> = state["modes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["id"].as_u64().unwrap())
            .collect();
        assert_eq!(ids.last(), Some(&10), "Custom is offered");
        for (n, id) in ids.iter().enumerate() {
            // The overlay sends the lighting it has, with only the mode changed.
            let mut lighting =
                call(&mut s, r#"{"id": 2, "cmd": "state"}"#)["result"]["lighting"].clone();
            lighting["mode"] = json!(id);
            let r = call(
                &mut s,
                &json!({"id": 10 + n, "cmd": "apply", "lighting": lighting}).to_string(),
            );
            assert_eq!(r["ok"], true, "mode {id}: {r}");
            assert_eq!(r["result"]["lighting"]["mode"], json!(id));
        }
        // Aurora keeps its own direction, and the state says so.
        let mut lighting =
            call(&mut s, r#"{"id": 3, "cmd": "state"}"#)["result"]["lighting"].clone();
        lighting["mode"] = json!(7);
        lighting["direction"] = json!(0);
        let r = call(
            &mut s,
            &json!({"id": 4, "cmd": "apply", "lighting": lighting}).to_string(),
        );
        assert_eq!(r["result"]["lighting"]["direction"], 4);
        // Asking again for Aurora with direction 0 is what the keyboard already keeps: nothing to send.
        let again = call(
            &mut s,
            &json!({"id": 5, "cmd": "apply", "lighting": lighting}).to_string(),
        );
        assert_eq!(again["result"]["changed"], json!([]));
    }

    #[test]
    fn invalid_requests_send_nothing() {
        let mut o = opener();
        let sim = o.sim.clone();
        let mut s = server(&mut o);
        let deep = r#"{"id": 1, "cmd": "apply", "triggers": [{"code": "KeyW", "trigger": {"mode": "fixed", "travel": 400, "press": 1, "release": 1}}]}"#;
        assert_eq!(call(&mut s, deep)["error"], "travel 400 is outside 8-340");
        let unknown = r#"{"id": 2, "cmd": "apply", "triggers": [{"code": "KeyHyper", "trigger": {"mode": "fixed", "travel": 100, "press": 1, "release": 1}}]}"#;
        assert!(
            call(&mut s, unknown)["error"]
                .as_str()
                .unwrap()
                .contains("no key KeyHyper")
        );
        let mut lighting =
            call(&mut s, r#"{"id": 3, "cmd": "state"}"#)["result"]["lighting"].clone();
        lighting["mode"] = json!(100);
        assert!(
            call(
                &mut s,
                &json!({"id": 4, "cmd": "apply", "lighting": lighting}).to_string()
            )["error"]
                .as_str()
                .unwrap()
                .contains("offers")
        );
        assert_eq!(sim.0.borrow().writes(), 0);
        let bad = call(&mut s, r#"{"id": 5, "cmd": "explode"}"#);
        assert_eq!(
            (bad["id"].clone(), bad["ok"].clone()),
            (json!(5), json!(false))
        );
    }

    #[test]
    fn a_missing_keyboard_is_an_error_and_is_retried() {
        let mut o = opener();
        o.present = false;
        {
            let mut s = server(&mut o);
            assert_eq!(
                call(&mut s, r#"{"id": 1, "cmd": "state"}"#)["error"],
                "no supported keyboard is plugged in"
            );
        }
        o.present = true;
        let mut s = server(&mut o);
        assert_eq!(call(&mut s, r#"{"id": 2, "cmd": "state"}"#)["ok"], true);
    }

    #[test]
    fn run_answers_line_by_line() {
        let mut o = opener();
        let mut s = server(&mut o);
        let mut out = Vec::new();
        s.run(
            "{\"id\": 1, \"cmd\": \"state\"}\n\n{\"id\": 2, \"cmd\": \"undo\"}\n".as_bytes(),
            &mut out,
        )
        .unwrap();
        let lines: Vec<Value> = String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(
            (lines[0]["ok"].clone(), lines[1]["ok"].clone()),
            (json!(true), json!(false))
        );
    }
}
