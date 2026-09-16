//! Physical key layouts: where each key sits in a keyboard's key matrix.

/// Columns in the AULA key matrix; a key's index is `COLUMNS * row + column`.
pub const COLUMNS: u8 = 22;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    pub index: u8,
    /// The W3C `KeyboardEvent.code` name, a stable id for config files.
    pub code: &'static str,
    /// Short label for tables.
    pub label: &'static str,
    /// Where the key is drawn, in the layout's own units (AULA's driver uses pixels).
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl Key {
    pub fn row(&self) -> u8 {
        self.index / COLUMNS
    }

    pub fn column(&self) -> u8 {
        self.index % COLUMNS
    }
}

/// Finds a key by its code (`KeyW`) or label (`W`), ignoring case.
pub fn find<'a>(layout: &'a [Key], name: &str) -> Option<&'a Key> {
    layout
        .iter()
        .find(|k| k.code.eq_ignore_ascii_case(name) || k.label.eq_ignore_ascii_case(name))
}

/// AULA WIN 60 HE, ANSI. Matrix indexes read from AULA's web driver for firmware V3.13.06
/// (research/captures/device-win60-state.json); positions from its layout for product
/// SI2825KZHEARGB (`/config/keys/SI2825KZHEARGB.json`).
pub static WIN60_HE: &[Key] = &[
    Key {
        index: 22,
        code: "Escape",
        label: "Esc",
        x: 10,
        y: 15,
        width: 35,
        height: 35,
    },
    Key {
        index: 23,
        code: "Digit1",
        label: "1",
        x: 48,
        y: 15,
        width: 35,
        height: 35,
    },
    Key {
        index: 24,
        code: "Digit2",
        label: "2",
        x: 86,
        y: 15,
        width: 35,
        height: 35,
    },
    Key {
        index: 25,
        code: "Digit3",
        label: "3",
        x: 124,
        y: 15,
        width: 35,
        height: 35,
    },
    Key {
        index: 26,
        code: "Digit4",
        label: "4",
        x: 162,
        y: 15,
        width: 35,
        height: 35,
    },
    Key {
        index: 27,
        code: "Digit5",
        label: "5",
        x: 200,
        y: 15,
        width: 35,
        height: 35,
    },
    Key {
        index: 28,
        code: "Digit6",
        label: "6",
        x: 238,
        y: 15,
        width: 35,
        height: 35,
    },
    Key {
        index: 29,
        code: "Digit7",
        label: "7",
        x: 276,
        y: 15,
        width: 35,
        height: 35,
    },
    Key {
        index: 30,
        code: "Digit8",
        label: "8",
        x: 314,
        y: 15,
        width: 35,
        height: 35,
    },
    Key {
        index: 31,
        code: "Digit9",
        label: "9",
        x: 352,
        y: 15,
        width: 35,
        height: 35,
    },
    Key {
        index: 32,
        code: "Digit0",
        label: "0",
        x: 390,
        y: 15,
        width: 35,
        height: 35,
    },
    Key {
        index: 33,
        code: "Minus",
        label: "-",
        x: 428,
        y: 15,
        width: 35,
        height: 35,
    },
    Key {
        index: 34,
        code: "Equal",
        label: "=",
        x: 466,
        y: 15,
        width: 35,
        height: 35,
    },
    Key {
        index: 36,
        code: "Backspace",
        label: "Bksp",
        x: 504,
        y: 15,
        width: 73,
        height: 35,
    },
    Key {
        index: 44,
        code: "Tab",
        label: "Tab",
        x: 10,
        y: 53,
        width: 54,
        height: 35,
    },
    Key {
        index: 45,
        code: "KeyQ",
        label: "Q",
        x: 67,
        y: 53,
        width: 35,
        height: 35,
    },
    Key {
        index: 46,
        code: "KeyW",
        label: "W",
        x: 105,
        y: 53,
        width: 35,
        height: 35,
    },
    Key {
        index: 47,
        code: "KeyE",
        label: "E",
        x: 143,
        y: 53,
        width: 35,
        height: 35,
    },
    Key {
        index: 48,
        code: "KeyR",
        label: "R",
        x: 181,
        y: 53,
        width: 35,
        height: 35,
    },
    Key {
        index: 49,
        code: "KeyT",
        label: "T",
        x: 219,
        y: 53,
        width: 35,
        height: 35,
    },
    Key {
        index: 50,
        code: "KeyY",
        label: "Y",
        x: 257,
        y: 53,
        width: 35,
        height: 35,
    },
    Key {
        index: 51,
        code: "KeyU",
        label: "U",
        x: 295,
        y: 53,
        width: 35,
        height: 35,
    },
    Key {
        index: 52,
        code: "KeyI",
        label: "I",
        x: 333,
        y: 53,
        width: 35,
        height: 35,
    },
    Key {
        index: 53,
        code: "KeyO",
        label: "O",
        x: 371,
        y: 53,
        width: 35,
        height: 35,
    },
    Key {
        index: 54,
        code: "KeyP",
        label: "P",
        x: 409,
        y: 53,
        width: 35,
        height: 35,
    },
    Key {
        index: 55,
        code: "BracketLeft",
        label: "[",
        x: 447,
        y: 53,
        width: 35,
        height: 35,
    },
    Key {
        index: 56,
        code: "BracketRight",
        label: "]",
        x: 485,
        y: 53,
        width: 35,
        height: 35,
    },
    Key {
        index: 58,
        code: "Backslash",
        label: "\\",
        x: 523,
        y: 53,
        width: 54,
        height: 35,
    },
    Key {
        index: 66,
        code: "CapsLock",
        label: "Caps",
        x: 10,
        y: 91,
        width: 63,
        height: 35,
    },
    Key {
        index: 68,
        code: "KeyA",
        label: "A",
        x: 76,
        y: 91,
        width: 35,
        height: 35,
    },
    Key {
        index: 69,
        code: "KeyS",
        label: "S",
        x: 114,
        y: 91,
        width: 35,
        height: 35,
    },
    Key {
        index: 70,
        code: "KeyD",
        label: "D",
        x: 152,
        y: 91,
        width: 35,
        height: 35,
    },
    Key {
        index: 71,
        code: "KeyF",
        label: "F",
        x: 190,
        y: 91,
        width: 35,
        height: 35,
    },
    Key {
        index: 72,
        code: "KeyG",
        label: "G",
        x: 228,
        y: 91,
        width: 35,
        height: 35,
    },
    Key {
        index: 73,
        code: "KeyH",
        label: "H",
        x: 266,
        y: 91,
        width: 35,
        height: 35,
    },
    Key {
        index: 74,
        code: "KeyJ",
        label: "J",
        x: 304,
        y: 91,
        width: 35,
        height: 35,
    },
    Key {
        index: 75,
        code: "KeyK",
        label: "K",
        x: 342,
        y: 91,
        width: 35,
        height: 35,
    },
    Key {
        index: 76,
        code: "KeyL",
        label: "L",
        x: 380,
        y: 91,
        width: 35,
        height: 35,
    },
    Key {
        index: 77,
        code: "Semicolon",
        label: ";",
        x: 418,
        y: 91,
        width: 35,
        height: 35,
    },
    Key {
        index: 78,
        code: "Quote",
        label: "'",
        x: 456,
        y: 91,
        width: 35,
        height: 35,
    },
    Key {
        index: 80,
        code: "Enter",
        label: "Enter",
        x: 494,
        y: 91,
        width: 83,
        height: 35,
    },
    Key {
        index: 88,
        code: "ShiftLeft",
        label: "LShift",
        x: 10,
        y: 129,
        width: 83,
        height: 35,
    },
    Key {
        index: 90,
        code: "KeyZ",
        label: "Z",
        x: 96,
        y: 129,
        width: 35,
        height: 35,
    },
    Key {
        index: 91,
        code: "KeyX",
        label: "X",
        x: 134,
        y: 129,
        width: 35,
        height: 35,
    },
    Key {
        index: 92,
        code: "KeyC",
        label: "C",
        x: 172,
        y: 129,
        width: 35,
        height: 35,
    },
    Key {
        index: 93,
        code: "KeyV",
        label: "V",
        x: 210,
        y: 129,
        width: 35,
        height: 35,
    },
    Key {
        index: 94,
        code: "KeyB",
        label: "B",
        x: 248,
        y: 129,
        width: 35,
        height: 35,
    },
    Key {
        index: 95,
        code: "KeyN",
        label: "N",
        x: 286,
        y: 129,
        width: 35,
        height: 35,
    },
    Key {
        index: 96,
        code: "KeyM",
        label: "M",
        x: 324,
        y: 129,
        width: 35,
        height: 35,
    },
    Key {
        index: 97,
        code: "Comma",
        label: ",",
        x: 362,
        y: 129,
        width: 35,
        height: 35,
    },
    Key {
        index: 98,
        code: "Period",
        label: ".",
        x: 400,
        y: 129,
        width: 35,
        height: 35,
    },
    Key {
        index: 99,
        code: "Slash",
        label: "/",
        x: 438,
        y: 129,
        width: 35,
        height: 35,
    },
    Key {
        index: 100,
        code: "ShiftRight",
        label: "RShift",
        x: 476,
        y: 129,
        width: 101,
        height: 35,
    },
    Key {
        index: 110,
        code: "ControlLeft",
        label: "LCtrl",
        x: 10,
        y: 167,
        width: 47,
        height: 35,
    },
    Key {
        index: 111,
        code: "MetaLeft",
        label: "LSuper",
        x: 60,
        y: 167,
        width: 47,
        height: 35,
    },
    Key {
        index: 112,
        code: "AltLeft",
        label: "LAlt",
        x: 110,
        y: 167,
        width: 47,
        height: 35,
    },
    Key {
        index: 116,
        code: "Space",
        label: "Space",
        x: 160,
        y: 167,
        width: 217,
        height: 35,
    },
    Key {
        index: 119,
        code: "AltRight",
        label: "RAlt",
        x: 380,
        y: 167,
        width: 47,
        height: 35,
    },
    Key {
        index: 120,
        code: "ContextMenu",
        label: "Menu",
        x: 430,
        y: 167,
        width: 47,
        height: 35,
    },
    Key {
        index: 121,
        code: "ControlRight",
        label: "RCtrl",
        x: 480,
        y: 167,
        width: 47,
        height: 35,
    },
    Key {
        index: 122,
        code: "KeyFn",
        label: "Fn",
        x: 530,
        y: 167,
        width: 47,
        height: 35,
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn win60_has_61_unique_keys() {
        assert_eq!(WIN60_HE.len(), 61);
        let indexes: HashSet<_> = WIN60_HE.iter().map(|k| k.index).collect();
        let codes: HashSet<_> = WIN60_HE.iter().map(|k| k.code).collect();
        let labels: HashSet<_> = WIN60_HE
            .iter()
            .map(|k| k.label.to_ascii_lowercase())
            .collect();
        assert_eq!((indexes.len(), codes.len(), labels.len()), (61, 61, 61));
    }

    #[test]
    fn w_is_row_2_column_2() {
        // The captured set-trigger frame for W selects column 2, bit 2.
        let w = find(WIN60_HE, "w").unwrap();
        assert_eq!((w.index, w.row(), w.column()), (46, 2, 2));
        assert_eq!(find(WIN60_HE, "KeyW"), Some(w));
    }

    #[test]
    fn positions_fit_the_board_without_overlapping() {
        for (i, a) in WIN60_HE.iter().enumerate() {
            assert!(
                a.x + a.width <= 587 && a.y + a.height <= 217,
                "{} is off the board",
                a.label
            );
            for b in &WIN60_HE[i + 1..] {
                let apart = a.x + a.width <= b.x
                    || b.x + b.width <= a.x
                    || a.y + a.height <= b.y
                    || b.y + b.height <= a.y;
                assert!(apart, "{} overlaps {}", a.label, b.label);
            }
        }
    }

    #[test]
    fn rows_run_top_to_bottom() {
        assert_eq!(find(WIN60_HE, "Esc").unwrap().row(), 1);
        assert_eq!(find(WIN60_HE, "Space").unwrap().row(), 5);
    }
}
