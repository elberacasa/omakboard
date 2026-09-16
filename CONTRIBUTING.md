# Contributing

Omakboard writes to people's keyboards, so the bar is correctness first: nothing about a
keyboard is assumed, and every claim about one is checked on one.

## Development loop

```sh
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
node --test "plugin/tests/**/*.test.js"
```

CI runs the same checks. Tests never need a keyboard: `src/sim.rs` simulates a WIN 60 HE with
replies captured from the real one, applying trigger and lighting writes to its own state.

Trying the plugin on your desktop:

```sh
cargo build --release && install -Dm755 target/release/omakboard ~/.local/bin/omakboard
D=~/.config/omarchy/plugins/io.github.elberacasa.omakboard
mkdir -p "$D" && cp -r manifest.json install.sh plugin "$D"/
omarchy-shell shell rescanPlugins
```

Copy the files rather than symlinking them; Omarchy's plugin validator rejects symlinks. QML
reloads when saved, but `Model.js` and new files do not: restart the shell (`omarchy restart shell`)
after changing `Model.js` or adding a file, because Qt caches `.pragma library` scripts and each
plugin folder's file list.

## Layout

| Path | What |
|---|---|
| `src/descriptor.rs`, `src/hidraw.rs`, `src/discover.rs` | finding the configuration interface by its HID descriptor |
| `src/proto/` | protocol backends; `aula_ff1b.rs` builds frames and decodes replies, with no I/O |
| `src/session.rs`, `src/lock.rs` | one request at a time, one process at a time |
| `src/snapshot.rs`, `src/apply.rs` | reading everything; validated, backed-up, read-back writes |
| `src/serve.rs` | the overlay's JSON-lines server |
| `src/registry.rs`, `src/layout.rs` | supported models and their key layouts |
| `plugin/` | Omarchy overlay, bar widget, and their pure JS model (`Model.js`) |
| `notes/` | the protocol and hardware, with where each fact came from |
| `research/tools/webhid-tap.js` | records what a vendor's web driver sends |

## Rules for keyboard code

- **No guessing.** Every frame layout in `notes/protocol-aula-ff1b.md` says where it came from:
  captured on the wire, read from the vendor's driver, or verified by Omakboard. A layout read
  only from the driver gets a capture before Omakboard writes it.
- **Check before sending.** Validate every value against the limits the keyboard reports before
  the first frame goes out; a request that fails validation sends nothing.
- **Back up, write only differences, read back.** New write paths follow `src/apply.rs`.
- **Never send the refused frames** in `aula_ff1b::refused`, and never add a way to flash firmware.
- **Verified means checked on that model.** Mark a model `Verified` only after each capability it
  claims was written and read back on the model itself.

## Adding a keyboard

You do not need to own a keyboard to add it, but someone who owns it has to check it.

1. Open a *Keyboard report* issue with `omakboard devices --json` and `omakboard info`.
2. If the keyboard speaks a protocol Omakboard knows, add it to `src/registry.rs` with its
   product name and layout, as `Untested`. AULA's web driver publishes both
   (`/config/device.json`, `/config/keys/<product>.json`).
3. If it does not, capture its web driver: open the driver in Chromium, paste
   `research/tools/webhid-tap.js` into the console before connecting, mark each action with
   `omakboard.mark("…")`, change one setting at a time, and attach the log.
4. Once an owner has changed and read back each capability on the keyboard, it becomes `Verified`.

## Logo

`docs/images/omakboard.svg` and `plugin/PixelMark.qml` draw the same sprite. Change the sprite in
both, then regenerate the SVG with `scripts/pixel-mark.py > docs/images/omakboard.svg`; CI checks
they match.
