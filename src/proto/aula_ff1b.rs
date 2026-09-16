//! AULA's Hall-effect protocol on vendor usage page 0xFF1B.
//!
//! notes/protocol-aula-ff1b.md documents the frames and where each fact came from. Every
//! frame is 63 payload bytes: command, sub-command, a big-endian page number, a payload
//! length, then the payload. This module builds requests and decodes replies; it does no I/O.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{Backend, Capabilities, ConfigInterface};
use crate::layout::Key;

pub const REPORT_ID: u8 = 0x01;
pub const PAYLOAD_LEN: usize = 63;

pub struct AulaFf1b;

impl Backend for AulaFf1b {
    fn id(&self) -> &'static str {
        "aula-ff1b"
    }

    fn config_interface(&self) -> ConfigInterface {
        ConfigInterface {
            usage_page: 0xFF1B,
            usage: 0x91,
            report_id: REPORT_ID,
            payload_len: PAYLOAD_LEN,
        }
    }

    fn capabilities(&self) -> Capabilities {
        // Each switched on after its frames were captured from AULA's driver and checked on a
        // WIN 60 HE. The rest of the protocol is documented but not yet written.
        Capabilities {
            actuation: true,
            rapid_trigger: true,
            lighting: true,
            ..Capabilities::default()
        }
    }
}

/// A request that only reads. Nothing in this enum changes the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Read {
    Status,
    Identity,
    Lighting,
    LightingModes,
    TriggerRange,
    Trigger { row: u8, column: u8 },
    PollRate,
}

impl Read {
    pub fn payload(self) -> [u8; PAYLOAD_LEN] {
        let mut p = [0u8; PAYLOAD_LEN];
        match self {
            Read::Status => p[0] = 0x01,
            Read::Identity => p[0] = 0x0D,
            Read::Lighting => p[..2].copy_from_slice(&[0x07, 0x01]),
            Read::LightingModes => p[0] = 0x0A,
            Read::TriggerRange => p[..6].copy_from_slice(&[0x21, 0, 0, 0, 0x18, 0x04]),
            Read::Trigger { row, column } => {
                p[..8].copy_from_slice(&[0x21, 0, 0, 0, 0x18, 0x05, row, column])
            }
            Read::PollRate => p[..6].copy_from_slice(&[0x21, 0, 0, 0, 0x01, 0x09]),
        }
        p
    }

    /// Whether an input payload answers this request.
    pub fn answered_by(self, reply: &[u8]) -> bool {
        if reply.len() < 6 {
            return false;
        }
        match self {
            Read::Status => reply[0] == 0x01,
            Read::Identity => reply[0] == 0x0D,
            Read::Lighting => reply[0] == 0x07,
            Read::LightingModes => reply[0] == 0x0A,
            Read::TriggerRange => reply[0] == 0x21 && reply[5] == 0x04,
            Read::Trigger { .. } => reply[0] == 0x21 && reply[5] == 0x05,
            Read::PollRate => reply[0] == 0x21 && reply[5] == 0x09,
        }
    }

    /// Replies to collect: identity arrives as two pages.
    pub fn pages(self) -> usize {
        match self {
            Read::Identity => 2,
            _ => 1,
        }
    }
}

/// Why a frame must never be sent. Checked on every outgoing frame, reads included.
pub fn refused(payload: &[u8]) -> Option<&'static str> {
    let at = |i: usize| payload.get(i).copied().unwrap_or(0);
    match (at(0), at(1)) {
        (0x14, 0x00) if at(5) != 0 => Some("factory reset"),
        (0x21, _) if at(4) == 0x18 && at(5) == 0x06 => Some("trigger reset"),
        (0x21, _) if at(4) == 0x18 && at(5) == 0x07 => Some("switch calibration"),
        _ => None,
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DecodeError {
    #[error("the keyboard sent no {0} reply")]
    Missing(&'static str),
    #[error("the {what} reply is malformed: {detail}")]
    Malformed { what: &'static str, detail: String },
}

fn malformed(what: &'static str, detail: impl Into<String>) -> DecodeError {
    DecodeError::Malformed {
        what,
        detail: detail.into(),
    }
}

/// The page number in bytes 2-3.
pub fn page(reply: &[u8]) -> u16 {
    u16::from_be_bytes([
        reply.get(2).copied().unwrap_or(0),
        reply.get(3).copied().unwrap_or(0),
    ])
}

/// Payload bytes, bounded by the length in byte 4.
fn body(reply: &[u8]) -> &[u8] {
    let len = reply.get(4).copied().unwrap_or(0) as usize;
    reply
        .get(5..)
        .map(|rest| &rest[..len.min(rest.len())])
        .unwrap_or(&[])
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    /// The comma-separated identity string, e.g. `W669,34,KB,SI,SI2825KZHEARGB,V3.13.06`.
    pub raw: String,
    /// Firmware build time, e.g. `Sep 22 2025,14:11:51`.
    pub built: Option<String>,
}

impl Identity {
    fn field(&self, i: usize) -> Option<&str> {
        self.raw.split(',').nth(i).filter(|s| !s.is_empty())
    }

    /// AULA's platform code: `W669`.
    pub fn platform(&self) -> Option<&str> {
        self.field(0)
    }

    pub fn chip(&self) -> Option<&str> {
        self.field(4)
    }

    /// `3.13.06`
    pub fn firmware(&self) -> Option<&str> {
        self.field(5).map(|v| v.trim_start_matches(['V', 'v']))
    }
}

/// Printable ASCII up to the first NUL.
fn ascii(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take_while(|&&b| b != 0)
        .filter(|b| b.is_ascii_graphic() || **b == b' ')
        .map(|&b| b as char)
        .collect()
}

pub fn parse_identity(replies: &[Vec<u8>]) -> Result<Identity, DecodeError> {
    let text = |p: u16| {
        replies
            .iter()
            .find(|r| page(r) == p)
            .map(|r| ascii(body(r)))
    };
    let raw = text(0).ok_or(DecodeError::Missing("identity"))?;
    if !raw.contains(',') {
        return Err(malformed("identity", format!("unexpected text {raw:?}")));
    }
    Ok(Identity {
        raw,
        built: text(1).filter(|s| !s.is_empty()),
    })
}

/// Feature flags from the status reply.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Status {
    pub socd: bool,
    pub calibration: bool,
    pub custom_lighting: bool,
    pub switch_types: bool,
    pub poll_rate: bool,
    pub dead_band: bool,
    pub music_rhythm: bool,
    /// Travel in 0.001 mm steps instead of 0.01 mm.
    pub high_precision: bool,
    pub gamepad: bool,
}

pub fn parse_status(reply: &[u8]) -> Result<Status, DecodeError> {
    if reply.len() < 19 || reply[0] != 0x01 {
        return Err(malformed("status", format!("{} bytes", reply.len())));
    }
    let (flags, more) = (reply[17], reply[18]);
    let bit = |byte: u8, n: u8| byte & (1 << n) != 0;
    Ok(Status {
        socd: reply[14] == 1,
        calibration: bit(flags, 0),
        custom_lighting: bit(flags, 1),
        switch_types: bit(flags, 2),
        poll_rate: bit(flags, 3),
        dead_band: bit(flags, 4),
        music_rhythm: bit(flags, 5),
        high_precision: bit(more, 0),
        gamepad: bit(more, 2),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl std::fmt::Display for Rgb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lighting {
    pub mode: u8,
    pub brightness: u8,
    pub speed: u8,
    pub foreground: Rgb,
    pub background: Rgb,
    pub direction: u8,
    pub full_color: bool,
    /// Raw. Reads 0 with the lights on (captured); its meaning is not confirmed.
    pub power: u8,
}

impl Lighting {
    pub fn mode_name(&self) -> Option<&'static str> {
        lighting_mode_name(self.mode)
    }
}

/// Lighting mode names, as AULA's web driver labels them in English.
pub fn lighting_mode_name(mode: u8) -> Option<&'static str> {
    Some(match mode {
        0 => "Static",
        1 => "Breath",
        2 => "Wave",
        3 => "Neon",
        4 => "Radar",
        6 => "Reactive",
        7 => "Aurora",
        8 => "Ripple",
        9 => "Twinkle",
        10 => "Custom",
        11 => "Cross",
        12 => "Speed Respond",
        14 => "Auto Ripple",
        15 => "Striation",
        16 => "Fireworks",
        100 => "Musical Rhythm",
        _ => return None,
    })
}

pub fn parse_lighting(reply: &[u8]) -> Result<Lighting, DecodeError> {
    let b = body(reply);
    if reply.first() != Some(&0x07) || b.len() < 12 {
        return Err(malformed("lighting", format!("{} payload bytes", b.len())));
    }
    let rgb = |i: usize| Rgb {
        r: b[i],
        g: b[i + 1],
        b: b[i + 2],
    };
    Ok(Lighting {
        mode: b[0],
        brightness: b[1],
        speed: b[2],
        foreground: rgb(3),
        background: rgb(6),
        direction: if b[9] >= 6 { 0 } else { b[9] },
        full_color: b[10] != 0,
        power: b[11],
    })
}

/// Travel limits. Travel values everywhere are in steps of `unit_um` micrometres.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TriggerRange {
    pub min: u16,
    pub max: u16,
    pub unit_um: u16,
}

impl TriggerRange {
    pub fn mm(&self, steps: u16) -> f64 {
        f64::from(steps) * f64::from(self.unit_um) / 1000.0
    }
}

pub fn parse_trigger_range(
    reply: &[u8],
    high_precision: bool,
) -> Result<TriggerRange, DecodeError> {
    if reply.len() < 11 || reply[0] != 0x21 || reply[5] != 0x04 {
        return Err(malformed("trigger range", format!("{} bytes", reply.len())));
    }
    let max = u16::from_be_bytes([reply[9], reply[6]]);
    let unit_raw = if reply[7] == 0 {
        10
    } else {
        u16::from(reply[7])
    };
    let unit_um = if high_precision {
        unit_raw
    } else {
        unit_raw * 10
    };
    // AULA firmware that reports no minimum: the web driver's fallbacks per unit.
    let min = match (reply[10], unit_um) {
        (0, 40) => 2,
        (0, 80) | (0, 100) => 1,
        (m, _) => u16::from(m),
    };
    if max == 0 || min > max {
        return Err(malformed("trigger range", format!("min {min}, max {max}")));
    }
    Ok(TriggerRange { min, max, unit_um })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "kind", content = "raw")]
pub enum TriggerMode {
    /// Actuates at a fixed depth.
    Fixed,
    /// Rapid trigger with one sensitivity for press and release.
    RapidTrigger,
    /// Rapid trigger with separate press and release sensitivities.
    RapidTriggerSeparate,
    Other(u8),
}

impl TriggerMode {
    pub fn from_byte(b: u8) -> Self {
        match b {
            0x00 => TriggerMode::Fixed,
            0x0C => TriggerMode::RapidTrigger,
            0x0D => TriggerMode::RapidTriggerSeparate,
            other => TriggerMode::Other(other),
        }
    }

    pub fn is_rapid(self) -> bool {
        matches!(
            self,
            TriggerMode::RapidTrigger | TriggerMode::RapidTriggerSeparate
        )
    }
}

/// One key's trigger, in travel steps (see [`TriggerRange::unit_um`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trigger {
    pub mode: TriggerMode,
    pub travel: u16,
    /// Rapid trigger press sensitivity.
    pub press: u16,
    /// Rapid trigger release sensitivity.
    pub release: u16,
}

pub fn parse_trigger(reply: &[u8]) -> Result<Trigger, DecodeError> {
    if reply.len() < 15 || reply[0] != 0x21 || reply[5] != 0x05 {
        return Err(malformed("trigger", format!("{} bytes", reply.len())));
    }
    Ok(Trigger {
        mode: TriggerMode::from_byte(reply[6]),
        travel: u16::from_be_bytes([reply[11], reply[7]]),
        press: u16::from_be_bytes([reply[13], reply[9]]),
        release: u16::from_be_bytes([reply[14], reply[10]]),
    })
}

/// Polling rate in Hz: the keyboard reports 1, 2, 4 or 8 kHz.
pub fn parse_poll_rate(reply: &[u8]) -> Result<u32, DecodeError> {
    if reply.len() < 7 || reply[0] != 0x21 || reply[5] != 0x09 {
        return Err(malformed("polling rate", format!("{} bytes", reply.len())));
    }
    match reply[6] {
        code @ (1 | 2 | 4 | 8) => Ok(u32::from(code) * 1000),
        other => Err(malformed("polling rate", format!("code {other}"))),
    }
}

/// The lighting modes a keyboard advertises, and its brightness and speed limits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LightingModes {
    pub modes: Vec<u8>,
    pub max_speed: u8,
    pub max_brightness: u8,
}

pub fn parse_lighting_modes(reply: &[u8]) -> Result<LightingModes, DecodeError> {
    let b = body(reply);
    // Mode ids, a two-byte marker, then max speed and max brightness.
    if reply.first() != Some(&0x0A) || b.len() < 4 {
        return Err(malformed(
            "lighting modes",
            format!("{} payload bytes", b.len()),
        ));
    }
    let n = b.len();
    Ok(LightingModes {
        modes: b[..n - 4].to_vec(),
        max_speed: b[n - 2],
        max_brightness: b[n - 1],
    })
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum WriteError {
    #[error("travel {value} is outside {min}-{max}")]
    Travel { value: u16, min: u16, max: u16 },
    #[error("rapid trigger sensitivity {value} is outside 1-{max}")]
    Sensitivity { value: u16, max: u16 },
    #[error("trigger mode {0:#04x} is not one Omakboard has verified")]
    UnknownMode(u8),
    #[error("no keys selected")]
    NoKeys,
    #[error("lighting mode {0} is not one this keyboard advertises")]
    LightingMode(u8),
    #[error("brightness {value} is above this keyboard's {max}")]
    Brightness { value: u8, max: u8 },
    #[error("speed {value} is above this keyboard's {max}")]
    Speed { value: u8, max: u8 },
}

impl TriggerMode {
    fn byte(self) -> Result<u8, WriteError> {
        match self {
            TriggerMode::Fixed => Ok(0x00),
            TriggerMode::RapidTrigger => Ok(0x0C),
            TriggerMode::RapidTriggerSeparate => Ok(0x0D),
            TriggerMode::Other(b) => Err(WriteError::UnknownMode(b)),
        }
    }
}

impl TriggerRange {
    /// The web driver's rapid trigger slider runs from 1 step to half the maximum travel.
    pub fn max_sensitivity(&self) -> u16 {
        self.max.div_ceil(2)
    }
}

/// Checks a trigger against the keyboard's limits before it is framed.
pub fn validate_trigger(t: &Trigger, range: &TriggerRange) -> Result<(), WriteError> {
    t.mode.byte()?;
    if t.travel < range.min || t.travel > range.max {
        return Err(WriteError::Travel {
            value: t.travel,
            min: range.min,
            max: range.max,
        });
    }
    let max = range.max_sensitivity();
    for value in [t.press, t.release] {
        if value == 0 || value > max {
            return Err(WriteError::Sensitivity { value, max });
        }
    }
    Ok(())
}

/// One set-trigger frame: every key in `keys` gets the same trigger.
pub fn trigger_frame(keys: &[Key], t: &Trigger) -> Result<[u8; PAYLOAD_LEN], WriteError> {
    if keys.is_empty() {
        return Err(WriteError::NoKeys);
    }
    let mut p = [0u8; PAYLOAD_LEN];
    p[..5].copy_from_slice(&[0x21, 0, 0, 0, 0x18]);
    p[5] = t.mode.byte()?;
    // One byte per matrix column, one bit per row.
    for key in keys {
        p[6 + usize::from(key.column())] |= 1 << key.row();
    }
    let [travel_hi, travel_lo] = t.travel.to_be_bytes();
    let [press_hi, press_lo] = t.press.to_be_bytes();
    let [release_hi, release_lo] = t.release.to_be_bytes();
    p[28..36].copy_from_slice(&[
        travel_lo, travel_lo, press_lo, release_lo, travel_hi, travel_hi, press_hi, release_hi,
    ]);
    Ok(p)
}

/// The keyboard's acknowledgement of a set-trigger frame.
pub fn is_trigger_ack(reply: &[u8]) -> bool {
    reply.len() > 5 && reply[0] == 0x21 && reply[4] == 0x01 && reply[5] == 0x00
}

pub fn validate_lighting(l: &Lighting, modes: &LightingModes) -> Result<(), WriteError> {
    if !modes.modes.contains(&l.mode) {
        return Err(WriteError::LightingMode(l.mode));
    }
    if l.brightness > modes.max_brightness {
        return Err(WriteError::Brightness {
            value: l.brightness,
            max: modes.max_brightness,
        });
    }
    if l.speed > modes.max_speed {
        return Err(WriteError::Speed {
            value: l.speed,
            max: modes.max_speed,
        });
    }
    Ok(())
}

pub fn lighting_frame(l: &Lighting) -> [u8; PAYLOAD_LEN] {
    let mut p = [0u8; PAYLOAD_LEN];
    p[..5].copy_from_slice(&[0x07, 0, 0, 0, 0x0E]);
    p[5..17].copy_from_slice(&[
        l.mode,
        l.brightness,
        l.speed,
        l.foreground.r,
        l.foreground.g,
        l.foreground.b,
        l.background.r,
        l.background.g,
        l.background.b,
        l.direction,
        u8::from(l.full_color),
        l.power,
    ]);
    p
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A reply from its hex bytes, zero-padded to a full payload.
    pub fn frame(hex: &str) -> Vec<u8> {
        let mut bytes: Vec<u8> = hex
            .split_whitespace()
            .map(|b| u8::from_str_radix(b, 16).unwrap())
            .collect();
        bytes.resize(PAYLOAD_LEN, 0);
        bytes
    }

    // Replies captured from the WIN 60 HE, firmware V3.13.06 (research/captures/00-connect-baseline.tsv).
    pub const STATUS: &str = "01 00 00 00 11 00 00 00 00 00 00 00 00 00 01 00 00 3f 00 01 01";
    pub const IDENTITY_0: &str = "0d 00 00 00 32 57 36 36 39 2c 33 34 2c 4b 42 2c 53 49 2c 53 49 32 38 32 35 4b 5a 48 45 41 52 47 42 2c 56 33 2e 31 33 2e 30 36";
    pub const IDENTITY_1: &str = "0d 00 00 01 32 53 65 70 20 32 32 20 32 30 32 35 2c 31 34 3a 31 31 3a 35 31 00 00 00 00 53 65 70 20";
    pub const LIGHTING: &str = "07 00 00 00 0c 00 04 03 ff";
    pub const RANGE: &str = "21 00 00 00 06 04 54 01 01 01 08";
    pub const TRIGGER_ESC: &str = "21 00 00 00 04 05 00 aa aa 01 01";
    pub const POLL_RATE: &str = "21 00 00 00 02 09 08";

    #[test]
    fn requests_match_the_captured_frames() {
        let hex = |r: Read| {
            let p = r.payload();
            let end = p.iter().rposition(|&b| b != 0).map_or(1, |i| i + 1);
            p[..end]
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(" ")
        };
        assert_eq!(hex(Read::Status), "01");
        assert_eq!(hex(Read::Identity), "0d");
        assert_eq!(hex(Read::Lighting), "07 01");
        assert_eq!(hex(Read::TriggerRange), "21 00 00 00 18 04");
        assert_eq!(
            hex(Read::Trigger { row: 1, column: 2 }),
            "21 00 00 00 18 05 01 02"
        );
        assert_eq!(hex(Read::PollRate), "21 00 00 00 01 09");
    }

    #[test]
    fn no_read_is_ever_refused() {
        for r in [
            Read::Status,
            Read::Identity,
            Read::Lighting,
            Read::TriggerRange,
            Read::PollRate,
            Read::LightingModes,
        ]
        .into_iter()
        .chain((0..6).flat_map(|row| (0..22).map(move |column| Read::Trigger { row, column })))
        {
            assert_eq!(refused(&r.payload()), None, "{r:?}");
        }
    }

    #[test]
    fn destructive_frames_are_refused() {
        assert_eq!(refused(&frame("14 00 00 00 01 01")), Some("factory reset"));
        assert_eq!(refused(&frame("21 00 00 00 18 06")), Some("trigger reset"));
        assert_eq!(
            refused(&frame("21 00 00 00 18 07 02 02")),
            Some("switch calibration")
        );
        // The performance-flags write shares command 14 sub 00 but leaves byte 5 zero.
        assert_eq!(
            refused(&frame("14 00 00 00 01 00 00 00 00 00 00 00 01")),
            None
        );
    }

    #[test]
    fn replies_are_matched_to_their_requests() {
        assert!(Read::TriggerRange.answered_by(&frame(RANGE)));
        assert!(!Read::Trigger { row: 1, column: 0 }.answered_by(&frame(RANGE)));
        assert!(Read::Trigger { row: 1, column: 0 }.answered_by(&frame(TRIGGER_ESC)));
        assert!(!Read::PollRate.answered_by(&frame(TRIGGER_ESC)));
        assert!(!Read::Identity.answered_by(&frame(STATUS)));
    }

    #[test]
    fn identity() {
        let id = parse_identity(&[frame(IDENTITY_1), frame(IDENTITY_0)]).unwrap();
        assert_eq!(id.raw, "W669,34,KB,SI,SI2825KZHEARGB,V3.13.06");
        assert_eq!(id.platform(), Some("W669"));
        assert_eq!(id.chip(), Some("SI2825KZHEARGB"));
        assert_eq!(id.firmware(), Some("3.13.06"));
        // The build time stops at the NUL; the bytes after it are stale buffer.
        assert_eq!(id.built.as_deref(), Some("Sep 22 2025,14:11:51"));
        assert_eq!(
            parse_identity(&[frame(IDENTITY_1)]),
            Err(DecodeError::Missing("identity"))
        );
    }

    #[test]
    fn status_flags() {
        let s = parse_status(&frame(STATUS)).unwrap();
        assert!(
            s.socd
                && s.calibration
                && s.custom_lighting
                && s.switch_types
                && s.poll_rate
                && s.dead_band
        );
        assert!(s.music_rhythm);
        assert!(!s.high_precision && !s.gamepad);
    }

    #[test]
    fn lighting() {
        let l = parse_lighting(&frame(LIGHTING)).unwrap();
        assert_eq!((l.mode, l.brightness, l.speed), (0, 4, 3));
        assert_eq!(l.foreground, Rgb { r: 255, g: 0, b: 0 });
        assert_eq!(l.foreground.to_string(), "#ff0000");
        assert_eq!(l.mode_name(), Some("Static"));
        assert!(parse_lighting(&frame("07 00 00 00 02 00 04")).is_err());
    }

    #[test]
    fn trigger_range() {
        let r = parse_trigger_range(&frame(RANGE), false).unwrap();
        assert_eq!(
            r,
            TriggerRange {
                min: 8,
                max: 340,
                unit_um: 10
            }
        );
        assert_eq!((r.mm(r.min), r.mm(r.max)), (0.08, 3.4));
        assert_eq!(parse_trigger_range(&frame(RANGE), true).unwrap().unit_um, 1);
    }

    #[test]
    fn triggers() {
        let t = parse_trigger(&frame(TRIGGER_ESC)).unwrap();
        assert_eq!(
            t,
            Trigger {
                mode: TriggerMode::Fixed,
                travel: 170,
                press: 1,
                release: 1
            }
        );
        // High bytes sit apart from the low bytes: 0x0154 = 340.
        let deep = parse_trigger(&frame("21 00 00 00 04 05 0c 54 54 32 32 01 01 00 00")).unwrap();
        assert_eq!(
            (deep.mode, deep.travel, deep.press),
            (TriggerMode::RapidTrigger, 340, 50)
        );
    }

    #[test]
    fn poll_rate() {
        assert_eq!(parse_poll_rate(&frame(POLL_RATE)), Ok(8000));
        assert!(parse_poll_rate(&frame("21 00 00 00 02 09 03")).is_err());
    }

    fn hex(p: &[u8]) -> String {
        let end = p.iter().rposition(|&b| b != 0).map_or(1, |i| i + 1);
        p[..end]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    const W: &str = "21 00 00 00 18 {mode} 00 00 04 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 {tail}";

    fn w_key() -> Vec<Key> {
        vec![*crate::layout::find(crate::layout::WIN60_HE, "W").unwrap()]
    }

    #[test]
    fn trigger_frames_match_the_captured_writes() {
        let cases = [
            (
                Trigger {
                    mode: TriggerMode::Fixed,
                    travel: 120,
                    press: 1,
                    release: 1,
                },
                "00",
                "78 78 01 01",
            ),
            (
                Trigger {
                    mode: TriggerMode::RapidTrigger,
                    travel: 120,
                    press: 50,
                    release: 50,
                },
                "0c",
                "78 78 32 32",
            ),
            (
                Trigger {
                    mode: TriggerMode::RapidTriggerSeparate,
                    travel: 120,
                    press: 50,
                    release: 50,
                },
                "0d",
                "78 78 32 32",
            ),
            (
                Trigger {
                    mode: TriggerMode::Fixed,
                    travel: 170,
                    press: 50,
                    release: 50,
                },
                "00",
                "aa aa 32 32",
            ),
        ];
        for (trigger, mode, tail) in cases {
            let expected = W.replace("{mode}", mode).replace("{tail}", tail);
            assert_eq!(
                hex(&trigger_frame(&w_key(), &trigger).unwrap()),
                expected,
                "{trigger:?}"
            );
        }
    }

    #[test]
    fn trigger_frames_carry_high_bytes_and_many_keys() {
        let layout = crate::layout::WIN60_HE;
        let keys: Vec<Key> = ["Esc", "Q", "A", "Space"]
            .iter()
            .map(|n| *crate::layout::find(layout, n).unwrap())
            .collect();
        let t = Trigger {
            mode: TriggerMode::RapidTriggerSeparate,
            travel: 340,
            press: 170,
            release: 3,
        };
        let p = trigger_frame(&keys, &t).unwrap();
        // Esc row 1 col 0; Q row 2 col 1; A row 3 col 2; Space row 5 col 6.
        assert_eq!(&p[6..13], &[0b10, 0b100, 0b1000, 0, 0, 0, 0b100000]);
        assert_eq!(
            &p[28..36],
            &[0x54, 0x54, 0xAA, 0x03, 0x01, 0x01, 0x00, 0x00]
        );
        assert_eq!(refused(&p), None);
        assert_eq!(trigger_frame(&[], &t), Err(WriteError::NoKeys));
    }

    #[test]
    fn trigger_limits() {
        let range = parse_trigger_range(&frame(RANGE), false).unwrap();
        let ok = Trigger {
            mode: TriggerMode::Fixed,
            travel: 170,
            press: 1,
            release: 1,
        };
        assert_eq!(validate_trigger(&ok, &range), Ok(()));
        assert_eq!(range.max_sensitivity(), 170);
        let low = Trigger { travel: 7, ..ok };
        assert_eq!(
            validate_trigger(&low, &range),
            Err(WriteError::Travel {
                value: 7,
                min: 8,
                max: 340
            })
        );
        let deep = Trigger { travel: 341, ..ok };
        assert!(validate_trigger(&deep, &range).is_err());
        let sensitive = Trigger {
            mode: TriggerMode::RapidTrigger,
            press: 171,
            ..ok
        };
        assert_eq!(
            validate_trigger(&sensitive, &range),
            Err(WriteError::Sensitivity {
                value: 171,
                max: 170
            })
        );
        let zero = Trigger { release: 0, ..ok };
        assert!(validate_trigger(&zero, &range).is_err());
        let unknown = Trigger {
            mode: TriggerMode::Other(5),
            ..ok
        };
        assert_eq!(
            validate_trigger(&unknown, &range),
            Err(WriteError::UnknownMode(5))
        );
    }

    #[test]
    fn trigger_ack() {
        assert!(is_trigger_ack(&frame("21 00 00 00 01")));
        assert!(!is_trigger_ack(&frame(POLL_RATE)));
        assert!(!is_trigger_ack(&frame(TRIGGER_ESC)));
    }

    pub const MODES: &str = "0a 00 00 00 12 02 01 03 09 04 00 0e 0f 06 07 0b 0c 10 08 a5 5a 04 04";

    #[test]
    fn lighting_modes() {
        let m = parse_lighting_modes(&frame(MODES)).unwrap();
        assert_eq!(m.modes, vec![2, 1, 3, 9, 4, 0, 14, 15, 6, 7, 11, 12, 16, 8]);
        assert_eq!((m.max_speed, m.max_brightness), (4, 4));
        // Every mode the WIN 60 HE advertises has a name.
        assert!(m.modes.iter().all(|&id| lighting_mode_name(id).is_some()));
    }

    #[test]
    fn lighting_frames_match_the_captured_writes() {
        let static_red = parse_lighting(&frame(LIGHTING)).unwrap();
        assert_eq!(
            hex(&lighting_frame(&static_red)),
            "07 00 00 00 0e 00 04 03 ff"
        );
        let green = Lighting {
            foreground: Rgb {
                r: 255,
                g: 0x34,
                b: 0,
            },
            ..static_red
        };
        assert_eq!(
            hex(&lighting_frame(&green)),
            "07 00 00 00 0e 00 04 03 ff 34"
        );
        let dim = Lighting {
            brightness: 2,
            ..green
        };
        assert_eq!(hex(&lighting_frame(&dim)), "07 00 00 00 0e 00 02 03 ff 34");
        let breath = Lighting {
            mode: 1,
            foreground: Rgb { r: 255, g: 0, b: 0 },
            ..dim
        };
        assert_eq!(hex(&lighting_frame(&breath)), "07 00 00 00 0e 01 02 03 ff");
    }

    #[test]
    fn lighting_limits() {
        let modes = parse_lighting_modes(&frame(MODES)).unwrap();
        let l = parse_lighting(&frame(LIGHTING)).unwrap();
        assert_eq!(validate_lighting(&l, &modes), Ok(()));
        assert_eq!(
            validate_lighting(&Lighting { mode: 10, ..l }, &modes),
            Err(WriteError::LightingMode(10))
        );
        assert!(validate_lighting(&Lighting { brightness: 5, ..l }, &modes).is_err());
        assert!(validate_lighting(&Lighting { speed: 5, ..l }, &modes).is_err());
    }
}
