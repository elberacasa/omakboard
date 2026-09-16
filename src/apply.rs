//! Changing settings: work out what differs, write only that, then read it back.

use serde::Deserialize;
use thiserror::Error;

use crate::hidraw::Transport;
use crate::layout::{self, Key};
use crate::proto::aula_ff1b::{
    self as aula, Identity, Lighting, LightingModes, Read, Trigger, TriggerMode, TriggerRange,
    WriteError,
};
use crate::session::{Session, SessionError};
use crate::snapshot::KeyTrigger;

#[derive(Debug, Error)]
pub enum ApplyError {
    #[error(transparent)]
    Session(#[from] SessionError),
    #[error(transparent)]
    Decode(#[from] aula::DecodeError),
    #[error(transparent)]
    Invalid(#[from] WriteError),
    #[error("{0} did not read back as written")]
    Mismatch(String),
}

/// One key's trigger before and after a change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyChange {
    pub key: Key,
    pub from: Trigger,
    pub to: Trigger,
}

/// A change to apply to each selected key's trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerEdit {
    /// Actuation depth, in travel steps; mode and sensitivities stay.
    Actuation(u16),
    /// Turn rapid trigger on. One sensitivity sets both; press and release apart select the
    /// separate mode. Unset values keep what the key has, or the driver's 0.50 mm default.
    RapidOn {
        press: Option<u16>,
        release: Option<u16>,
    },
    /// Back to a fixed actuation point. Sensitivities are kept, as the web driver does.
    RapidOff,
}

/// The web driver's default rapid trigger sensitivity: 0.50 mm.
pub fn default_sensitivity(range: &TriggerRange) -> u16 {
    500 / range.unit_um.max(1)
}

pub fn edit_trigger(t: Trigger, edit: TriggerEdit, range: &TriggerRange) -> Trigger {
    match edit {
        TriggerEdit::Actuation(travel) => Trigger { travel, ..t },
        TriggerEdit::RapidOff => Trigger {
            mode: TriggerMode::Fixed,
            ..t
        },
        TriggerEdit::RapidOn { press, release } => {
            let (old_press, old_release) = if t.mode.is_rapid() {
                (t.press, t.release)
            } else {
                (default_sensitivity(range), default_sensitivity(range))
            };
            let separate = match (press, release) {
                (Some(p), Some(r)) => p != r,
                (None, None) => t.mode == TriggerMode::RapidTriggerSeparate,
                _ => true,
            };
            let press = press.unwrap_or(old_press);
            // Linked rapid trigger uses one sensitivity for both directions.
            let release = release.unwrap_or(if separate { old_release } else { press });
            let mode = if separate {
                TriggerMode::RapidTriggerSeparate
            } else {
                TriggerMode::RapidTrigger
            };
            Trigger {
                mode,
                travel: t.travel,
                press,
                release,
            }
        }
    }
}

/// The changes that `target` makes to `current`, skipping keys it leaves as they are.
pub fn trigger_changes(
    current: &[KeyTrigger],
    layout: &[Key],
    mut target: impl FnMut(&KeyTrigger) -> Option<Trigger>,
) -> Vec<KeyChange> {
    current
        .iter()
        .filter_map(|k| {
            let to = target(k)?;
            let key = *layout.iter().find(|l| l.index == k.index)?;
            (to != k.trigger).then_some(KeyChange {
                key,
                from: k.trigger,
                to,
            })
        })
        .collect()
}

/// Groups changes that share a target trigger, so each group is one frame.
pub fn group(changes: &[KeyChange]) -> Vec<(Trigger, Vec<Key>)> {
    let mut groups: Vec<(Trigger, Vec<Key>)> = Vec::new();
    for c in changes {
        match groups.iter_mut().find(|(t, _)| *t == c.to) {
            Some((_, keys)) => keys.push(c.key),
            None => groups.push((c.to, vec![c.key])),
        }
    }
    groups
}

/// The frames `apply_triggers` would send, validated, without sending them.
pub fn trigger_frames(
    changes: &[KeyChange],
    range: &TriggerRange,
) -> Result<Vec<[u8; aula::PAYLOAD_LEN]>, ApplyError> {
    group(changes)
        .iter()
        .map(|(t, keys)| {
            aula::validate_trigger(t, range)?;
            Ok(aula::trigger_frame(keys, t)?)
        })
        .collect()
}

/// How long to keep reading back before calling a write a mismatch. Every WIN 60 HE write has
/// read back on the first try; the retries cover a slower keyboard, not a known delay.
const READ_BACK_DELAYS_MS: [u64; 3] = [0, 40, 150];

/// Writes the changes, then reads every changed key back.
pub fn apply_triggers<T: Transport>(
    session: &mut Session<T>,
    changes: &[KeyChange],
    range: &TriggerRange,
) -> Result<(), ApplyError> {
    // Validate every frame before sending the first.
    let frames = trigger_frames(changes, range)?;
    for frame in &frames {
        session.write(frame, Some(aula::is_trigger_ack))?;
    }
    let mut pending: Vec<&KeyChange> = changes.iter().collect();
    let mut wrong: Vec<(&KeyChange, Trigger)> = Vec::new();
    for delay in READ_BACK_DELAYS_MS {
        std::thread::sleep(std::time::Duration::from_millis(delay));
        wrong.clear();
        for c in &pending {
            let reply = session.read_one(Read::Trigger {
                row: c.key.row(),
                column: c.key.column(),
            })?;
            let got = aula::parse_trigger(&reply)?;
            if got != c.to {
                wrong.push((c, got));
            }
        }
        if wrong.is_empty() {
            return Ok(());
        }
        pending = wrong.iter().map(|(c, _)| *c).collect();
    }
    let detail: Vec<String> = wrong
        .iter()
        .map(|(c, got)| {
            format!(
                "{} ({})",
                c.key.label,
                trigger_differences(&c.to, got).join(", ")
            )
        })
        .collect();
    Err(ApplyError::Mismatch(format!(
        "the trigger of {}",
        detail.join("; ")
    )))
}

fn trigger_differences(wanted: &Trigger, got: &Trigger) -> Vec<String> {
    let mut out = Vec::new();
    if wanted.mode != got.mode {
        out.push(format!("mode {:?} -> {:?}", wanted.mode, got.mode));
    }
    for (name, a, b) in [
        ("travel", wanted.travel, got.travel),
        ("press", wanted.press, got.press),
        ("release", wanted.release, got.release),
    ] {
        if a != b {
            out.push(format!("{name} {a} -> {b}"));
        }
    }
    out
}

/// Writes lighting and reads it back; returns what the keyboard has now. `from` is what it had.
///
/// The firmware keeps some values in its own form (see [`aula::stored_lighting`]), so that form
/// is what is written and expected back. A mode change first leaves music rhythm, as AULA's
/// web driver does.
pub fn apply_lighting<T: Transport>(
    session: &mut Session<T>,
    modes: &LightingModes,
    from: &Lighting,
    to: &Lighting,
) -> Result<Lighting, ApplyError> {
    let expected = aula::stored_lighting(to);
    aula::validate_lighting(&expected, modes, from.mode)?;
    if expected.mode != from.mode {
        session.write(
            &aula::leave_music_rhythm_frame(),
            Some(aula::is_lighting_helper_ack),
        )?;
    }
    session.write(&aula::lighting_frame(&expected), None)?;
    let mut back = *from;
    for delay in READ_BACK_DELAYS_MS {
        // Lighting writes get no reply; even the first read waits a moment.
        std::thread::sleep(std::time::Duration::from_millis(delay.max(20)));
        back = aula::parse_lighting(&session.read_one(Read::Lighting)?)?;
        if back == expected {
            return Ok(back);
        }
    }
    Err(ApplyError::Mismatch(format!(
        "lighting ({})",
        aula::lighting_differences(&expected, &back).join(", ")
    )))
}

/// Parses a length in millimetres ("1.2", "1.20mm", "0.05") into travel steps.
pub fn parse_mm(text: &str, range: &TriggerRange) -> Result<u16, String> {
    let s = text.trim().trim_end_matches("mm").trim();
    let (whole, frac) = s.split_once('.').unwrap_or((s, ""));
    let bad = || format!("{text:?} is not a length in mm, such as 1.20");
    if whole.is_empty() && frac.is_empty()
        || frac.len() > 3
        || !whole
            .chars()
            .chain(frac.chars())
            .all(|c| c.is_ascii_digit())
    {
        return Err(bad());
    }
    let whole: u32 = if whole.is_empty() {
        0
    } else {
        whole.parse().map_err(|_| bad())?
    };
    let frac: u32 = format!("{frac:0<3}").parse().map_err(|_| bad())?;
    let um = whole
        .checked_mul(1000)
        .and_then(|w| w.checked_add(frac))
        .ok_or_else(bad)?;
    let unit = u32::from(range.unit_um.max(1));
    if um % unit != 0 {
        return Err(format!(
            "{text} mm is not a multiple of this keyboard's {:.3} mm step",
            f64::from(unit) / 1000.0
        ));
    }
    u16::try_from(um / unit).map_err(|_| bad())
}

/// Resolves key names (`W`, `KeyW`, `space`) against a layout.
pub fn select_keys(layout: &[Key], names: &[String]) -> Result<Vec<Key>, String> {
    names
        .iter()
        .map(|n| {
            layout::find(layout, n).copied().ok_or_else(|| {
                format!("no key named {n:?}; names are labels like W, Esc, LShift, Space")
            })
        })
        .collect()
}

/// A backup file, read back for `restore`.
#[derive(Debug, Deserialize)]
pub struct SavedBackup {
    pub schema: u32,
    pub model: String,
    pub info: SavedInfo,
    pub triggers: Vec<SavedTrigger>,
}

#[derive(Debug, Deserialize)]
pub struct SavedInfo {
    pub identity: Identity,
    pub lighting: Lighting,
}

#[derive(Debug, Deserialize)]
pub struct SavedTrigger {
    pub code: String,
    pub trigger: Trigger,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::WIN60_HE;
    use crate::proto::aula_ff1b::tests::*;
    use crate::session::tests::FakeKeyboard;
    use crate::sim::SimKeyboard;

    fn range() -> TriggerRange {
        aula::parse_trigger_range(&frame(RANGE), false).unwrap()
    }

    const FIXED: Trigger = Trigger {
        mode: TriggerMode::Fixed,
        travel: 170,
        press: 1,
        release: 1,
    };

    fn current() -> Vec<KeyTrigger> {
        WIN60_HE
            .iter()
            .map(|k| KeyTrigger {
                code: k.code,
                label: k.label,
                index: k.index,
                trigger: FIXED,
            })
            .collect()
    }

    #[test]
    fn millimetres() {
        let r = range();
        assert_eq!(parse_mm("1.2", &r), Ok(120));
        assert_eq!(parse_mm("1.20mm", &r), Ok(120));
        assert_eq!(parse_mm(".5", &r), Ok(50));
        assert_eq!(parse_mm("3.4", &r), Ok(340));
        assert_eq!(parse_mm("2", &r), Ok(200));
        assert!(parse_mm("1.234", &r).unwrap_err().contains("0.010 mm step"));
        assert!(parse_mm("1,2", &r).is_err());
        assert!(parse_mm("-1", &r).is_err());
        assert!(parse_mm("", &r).is_err());
        assert!(parse_mm("1.2345", &r).is_err());
    }

    #[test]
    fn rapid_trigger_edits() {
        let r = range();
        let on = |press, release| edit_trigger(FIXED, TriggerEdit::RapidOn { press, release }, &r);
        // From fixed, with nothing given: the driver's 0.50 mm default, linked.
        assert_eq!(
            on(None, None),
            Trigger {
                mode: TriggerMode::RapidTrigger,
                travel: 170,
                press: 50,
                release: 50
            }
        );
        // One direction alone is a separate setting; the other keeps the default.
        assert_eq!(
            on(Some(30), None),
            Trigger {
                mode: TriggerMode::RapidTriggerSeparate,
                travel: 170,
                press: 30,
                release: 50
            }
        );
        // `--sensitivity` passes both.
        assert_eq!(on(Some(30), Some(30)).mode, TriggerMode::RapidTrigger);
        assert_eq!(
            on(Some(30), Some(60)),
            Trigger {
                mode: TriggerMode::RapidTriggerSeparate,
                travel: 170,
                press: 30,
                release: 60
            }
        );
        // A key already on separate rapid trigger keeps its values unless told otherwise.
        let separate = Trigger {
            mode: TriggerMode::RapidTriggerSeparate,
            travel: 120,
            press: 20,
            release: 40,
        };
        assert_eq!(
            edit_trigger(
                separate,
                TriggerEdit::RapidOn {
                    press: None,
                    release: None
                },
                &r
            ),
            separate
        );
        assert_eq!(
            edit_trigger(
                separate,
                TriggerEdit::RapidOn {
                    press: None,
                    release: Some(10)
                },
                &r
            ),
            Trigger {
                release: 10,
                ..separate
            }
        );
        assert_eq!(
            edit_trigger(separate, TriggerEdit::RapidOff, &r),
            Trigger {
                mode: TriggerMode::Fixed,
                ..separate
            }
        );
        assert_eq!(
            edit_trigger(separate, TriggerEdit::Actuation(200), &r),
            Trigger {
                travel: 200,
                ..separate
            }
        );
    }

    #[test]
    fn only_changed_keys_are_written_grouped_by_target() {
        let mut now = current();
        now[2].trigger.travel = 120; // key "2" already at 1.20
        let wanted: Vec<&str> = vec!["W", "A", "S", "D", "2"];
        let changes = trigger_changes(&now, WIN60_HE, |k| {
            wanted
                .contains(&k.label)
                .then(|| edit_trigger(k.trigger, TriggerEdit::Actuation(120), &range()))
        });
        assert_eq!(
            changes.iter().map(|c| c.key.label).collect::<Vec<_>>(),
            ["W", "A", "S", "D"]
        );
        let groups = group(&changes);
        assert_eq!(groups.len(), 1);
        assert_eq!(trigger_frames(&changes, &range()).unwrap().len(), 1);
    }

    #[test]
    fn invalid_changes_send_nothing() {
        let changes = trigger_changes(&current(), WIN60_HE, |k| {
            (k.label == "W").then_some(Trigger {
                travel: 400,
                ..k.trigger
            })
        });
        let mut s = Session::new(FakeKeyboard::new(Vec::new()));
        assert!(matches!(
            apply_triggers(&mut s, &changes, &range()),
            Err(ApplyError::Invalid(WriteError::Travel { .. }))
        ));
        assert!(s.trace.is_empty());
    }

    #[test]
    fn writes_are_read_back() {
        let changes = trigger_changes(&current(), WIN60_HE, |k| {
            ["W", "Space"].contains(&k.label).then(|| {
                edit_trigger(
                    k.trigger,
                    TriggerEdit::RapidOn {
                        press: Some(30),
                        release: Some(30),
                    },
                    &range(),
                )
            })
        });
        let mut s = Session::new(SimKeyboard::default());
        apply_triggers(&mut s, &changes, &range()).unwrap();

        let mut s = Session::new(SimKeyboard {
            lie: true,
            ..SimKeyboard::default()
        });
        let err = apply_triggers(&mut s, &changes, &range()).unwrap_err();
        assert_eq!(
            err.to_string(),
            "the trigger of W (travel 170 -> 171); Space (travel 170 -> 171) did not read back as written"
        );
    }

    #[test]
    fn lighting_is_written_and_read_back() {
        let modes = aula::parse_lighting_modes(&frame(MODES)).unwrap();
        let mut s = Session::new(SimKeyboard::default());
        let now = aula::parse_lighting(&s.read_one(Read::Lighting).unwrap()).unwrap();
        let wanted = Lighting {
            mode: 2,
            brightness: 1,
            ..now
        };
        assert_eq!(
            apply_lighting(&mut s, &modes, &now, &wanted).unwrap(),
            wanted
        );
        assert_eq!(
            aula::parse_lighting(&s.read_one(Read::Lighting).unwrap()).unwrap(),
            wanted
        );
    }

    #[test]
    fn effects_with_their_own_direction_read_back() {
        // The bug this guards: Aurora keeps direction 4 when sent 0, which read back as a mismatch.
        let modes = aula::parse_lighting_modes(&frame(MODES)).unwrap();
        let mut s = Session::new(SimKeyboard::default());
        let base = aula::parse_lighting(&s.read_one(Read::Lighting).unwrap()).unwrap();
        for mode in [4, 7, 12, 15] {
            let current = aula::parse_lighting(&s.read_one(Read::Lighting).unwrap()).unwrap();
            let back = apply_lighting(
                &mut s,
                &modes,
                &current,
                &Lighting {
                    mode,
                    direction: 0,
                    ..base
                },
            )
            .unwrap();
            assert_eq!(back.mode, mode);
            assert_ne!(back.direction, 0, "mode {mode} keeps its own direction");
        }
        // Rainbow on Static is kept off, and expected off.
        let current = aula::parse_lighting(&s.read_one(Read::Lighting).unwrap()).unwrap();
        let back = apply_lighting(
            &mut s,
            &modes,
            &current,
            &Lighting {
                mode: 0,
                full_color: true,
                ..base
            },
        )
        .unwrap();
        assert!(!back.full_color);
    }

    #[test]
    fn a_mode_change_leaves_music_rhythm_first() {
        let modes = aula::parse_lighting_modes(&frame(MODES)).unwrap();
        let mut s = Session::new(SimKeyboard::default());
        let sent_17 = |s: &Session<SimKeyboard>| {
            s.trace
                .iter()
                .filter(|(d, p)| *d == crate::session::Direction::Out && p[0] == 0x17)
                .count()
        };
        let now = aula::parse_lighting(&s.read_one(Read::Lighting).unwrap()).unwrap();
        let dim = apply_lighting(
            &mut s,
            &modes,
            &now,
            &Lighting {
                brightness: 1,
                ..now
            },
        )
        .unwrap();
        assert_eq!(sent_17(&s), 0, "no mode change, no 17");
        apply_lighting(&mut s, &modes, &dim, &Lighting { mode: 8, ..dim }).unwrap();
        assert_eq!(sent_17(&s), 1);
    }

    #[test]
    fn mismatches_name_what_differs() {
        let modes = aula::parse_lighting_modes(&frame(MODES)).unwrap();
        let mut s = Session::new(SimKeyboard {
            lie_about_lighting: true,
            ..SimKeyboard::default()
        });
        let now = aula::parse_lighting(&s.read_one(Read::Lighting).unwrap()).unwrap();
        let err = apply_lighting(
            &mut s,
            &modes,
            &now,
            &Lighting {
                brightness: 4,
                speed: 1,
                ..now
            },
        )
        .unwrap_err();
        assert_eq!(
            err.to_string(),
            "lighting (brightness 4 -> 3) did not read back as written"
        );
    }

    #[test]
    fn keys_by_name() {
        let keys = select_keys(WIN60_HE, &["w".into(), "KeyA".into(), "Space".into()]).unwrap();
        assert_eq!(
            keys.iter().map(|k| k.index).collect::<Vec<_>>(),
            [46, 68, 116]
        );
        assert!(select_keys(WIN60_HE, &["Hyper".into()]).is_err());
    }
}
