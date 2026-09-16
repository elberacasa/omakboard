# Security

Omakboard runs as your user and reaches keyboards through a udev rule that grants the logged-in
seat user access to each supported keyboard's vendor configuration interface, and nothing else.

The plugin runs only `omakboard` and this checkout's own `install.sh`. The installer verifies
the release archive's SHA-256 before installing anything, and uses sudo only to install the
udev rule.

Report a vulnerability privately at
https://github.com/elberacasa/omakboard/security/advisories/new, not in a public issue. That
includes anything that could damage a keyboard or change it without being asked.
