# exc

A fast, lightweight menu launcher for your terminal — a searchable,
keyboard-driven command picker configured entirely from a TOML file.

Use it as a general-purpose "everything menu" for the commands you run all
day (SSH sessions, VPN toggles, quick diagnostics), or drop a project-local
config next to a repo to give it its own menu of tasks — deployments, git
housekeeping, Docker cleanup, whatever that project needs. Commands can
prompt for parameters (with optional masked/secret input), so the same
command template covers a whole family of targets instead of one entry per
host.

## Install

```sh
cargo install exc-launcher   # published crate name; installs the `exc` binary
```

Or build from a clone of this repo:

```sh
cargo build --release
cp target/release/exc ~/.local/bin/exc   # or anywhere on your PATH
```

`cargo install` only puts the `exc` binary on your `$PATH` — it doesn't
install the man page. See [Man page](#man-page) below to add one.

## Quick start

Run `exc` with no arguments. If no config file exists yet at the default
path, it offers to write a starter one for you right there:

```
$ exc
No config file found at ~/.config/exc/config.toml.
Create a starter config there now? [Y/n]
```

Say yes and it drops a small starter config (a few example commands across
a couple of profiles) and launches straight into the picker. You can also
skip the prompt and do it explicitly:

```sh
exc init             # write a starter config with a few example commands
exc validate          # lint the config for mistakes
exc validate --strict --format json   # for scripting
```

The `config.toml` at the root of this repo is a larger, self-contained
example — system/git/docker/network/secrets/deploy profiles showing off
params, secret (masked) params, and multi-statement commands — meant to be
copied and edited into your own config:

```sh
mkdir -p ~/.config/exc
cp config.toml ~/.config/exc/config.toml
```

It is **not** written there automatically — `exc init` always writes the
small generic starter, never this one.

There's also `launcher.sh` at the repo root: a minimal, dependency-free
bash `select`-menu implementation of the same idea, using the same example
commands. It's the shape of script `exc` grew out of, kept here in case you'd
rather copy a plain shell script than build/install a binary.

## Config

Default location: `$XDG_CONFIG_HOME/exc/config.toml`, falling back to
`~/.config/exc/config.toml`. Override with `--config <path>`.

### Schema

```toml
[meta]
title = "me@my-box"     # optional; header shown in the sysinfo box (default: user@host)
theme = "default"        # built-in base palette: default | dark | mono
# theme_file = "themes/dracula.toml"  # OR: an external palette file (ignored if [theme] below is set)

# optional custom palette, inline — takes precedence over theme_file.
# Any color you omit falls back to the `theme` base palette above.
[theme]
accent = "#ff8800"       # #rrggbb hex, or a named color (see Themes below)
border = "darkgrey"
# text, muted, selected_bg, selected_fg also available

[[profiles]]
name = "network"          # used for --profile and exc <name> resolution scoping
label = "Network"         # optional; display name (defaults to `name`)
description = "..."       # optional; shown by `exc list`

  [[profiles.commands]]
  name = "cert-check-online"   # must be globally unique across all profiles
  description = "Check the TLS certificate presented by a remote domain"
  command = "openssl s_client -connect {{domain}}:443"

    [[profiles.commands.params]]
    name = "domain"        # matches a {{domain}} placeholder in `command`
    prompt = "Domain to check"
    default = ""             # shown inline, used when you press Enter on an empty prompt
    secret = false            # true masks input (rpassword) for things like tokens
```

Commands run via `sh -c "<expanded command>"` with stdio fully inherited, so
`ssh`, `fzf`, interactive `sudo` prompts, etc. all work as expected. Multi-
statement commands joined with `;` run every statement regardless of earlier
failures — handy for chained cleanup/prune commands where you want every
step attempted.

One caveat: a command like `source some/venv/activate` only affects the
short-lived `sh -c` child process that runs it, not your interactive shell —
the same limitation any non-sourced shell script has.

## Usage

```sh
exc                        # interactive picker
exc <name>                 # run a command directly (shorthand for `run`)
exc run <name>              # same, explicit form; substring match if no exact name
exc list [--profile P] [--plain]
exc sysinfo
exc validate [--strict] [--format text|json]
exc init [--force]
exc man                     # print a roff(7) man page to stdout, see Man page below

# global flags
exc --config <path> --profile <name> --theme default|dark|mono --no-color
```

## Interactive picker keybindings

Items are numbered and laid out **column-major**: top-to-bottom within a
column, then across to the next column (like `ls` multi-column output) —
not left-to-right, top-to-bottom.

| Key | Action |
|---|---|
| Type letters/symbols | Live filter (regex, falls back to substring on invalid regex) |
| Type digits only | Jump straight to that numbered item (e.g. `12` selects item `[12]`) — matches the numbers shown by `exc list` and an unfiltered grid |
| Backspace | Delete last input character |
| Ctrl-U | Clear the input (filter text or digits) |
| ↑ ↓ ← → | Move selection across the grid |
| Ctrl-J / Ctrl-K / Ctrl-H / Ctrl-L | Vim-style aliases for ↓ / ↑ / ← / → |
| Enter | Run the selected command (prompts for params first, if any) |
| Tab / Shift-Tab | Next / previous profile |
| Esc / Ctrl-C | Quit without running anything |

The picker uses `hjkl` as Ctrl-chords rather than bare letters, since bare
letters are live filter input (the same trade-off tools like `fzf` make). A
number-only query is a dedicated "jump to ID" mode rather than a text search
— the `#` prompt glyph (instead of `/`) shows when you're in it.

## System info panel

`exc sysinfo` (and the picker's header box) show three tiers of fields, each
with a different cost budget:

- **Always shown, computed synchronously at startup**: OS, host, kernel,
  uptime, memory, disk, process count, CPU model/core count, load average
  (macOS/Linux) or CPU% (Windows), swap, local IP, battery, shell/terminal,
  local time, top processes, GPU name.
- **Background-refreshed**: public IP, pending package updates, network
  throughput. These are fetched off a background thread every ~30s (package
  updates every ~30min, since that check itself can take a few seconds) and
  patched into the panel once available — they never delay the picker
  opening. `exc sysinfo` instead does a single bounded fetch (1.5s) and
  simply omits these fields if that doesn't resolve in time, rather than
  hanging the command.

Every field beyond the original 7 is optional and simply omitted if it
can't be determined on your platform (e.g. no battery present, no supported
package manager found) — there's no forced parity across macOS/Linux/
Windows. A couple of fields are genuinely platform-specific by design: load
average has no Windows equivalent (CPU% is shown instead there), and swap/
battery/GPU-name lookups each use a different native API per OS
(`sysctl`/IOKit on macOS, `/proc`+`/sys` on Linux, kernel32 FFI + a
PowerShell CIM query for GPU name on Windows — never `wmic`, which Windows
11 removed in 2026).

## Themes

Three built-in base palettes — `default`, `dark`, `mono` — selected via
`[meta] theme` in the config or overridden with `--theme`. `--no-color` / the
`NO_COLOR` env var forces `mono` regardless of everything below.

You can also define your own colors, two ways:

- **Inline**, as a `[theme]` table in `config.toml` (see the Schema section
  above).
- **In a separate file**, via `[meta] theme_file = "path/to/palette.toml"` —
  a standalone TOML file whose root has the same fields as `[theme]` (no
  wrapper table needed). Relative paths resolve against the config file's
  own directory; `~/` expands to your home directory.

Either form accepts six optional fields — `accent`, `border`, `text`,
`muted`, `selected_bg`, `selected_fg` — each either `"#rrggbb"` hex or a
named color (`black`, `darkgrey`/`darkgray`, `red`, `darkred`, `green`,
`darkgreen`, `yellow`, `darkyellow`, `blue`, `darkblue`, `magenta`,
`darkmagenta`, `cyan`, `darkcyan`, `white`, `grey`/`gray`). Any field you
don't set falls back to the corresponding color from the `[meta] theme` base
palette. `selected_bg`/`selected_fg` control the highlighted item in the
picker grid — kept separate from `text` on purpose, since the highlight
background is usually a light/saturated color and needs its own contrasting
foreground.

Precedence, most to least specific: `--theme` CLI flag (always a full
built-in palette) → inline `[theme]` table → `[meta] theme_file` → `[meta]
theme` → built-in `default`. `exc validate` checks color names/hex, that a
`theme_file` path actually exists and parses, and warns if both an inline
`[theme]` table and `theme_file` are set (the inline table silently wins).

### Bundled palettes

The `themes/` folder ships ten ready-to-use palettes modeled on common
terminal color schemes (the kind you'd find in an iTerm2 profile list):
`solarized-dark`, `solarized-light`, `dracula`, `nord`, `gruvbox-dark`,
`one-dark`, `monokai`, `tomorrow-night`, `homebrew`, and `ayu-dark`. Point
`[meta] theme_file` at one to use it as-is, or copy it into your own config
as a starting point for a custom palette:

```toml
[meta]
theme_file = "themes/dracula.toml"
```

## Man page

`exc man` renders a roff(7) man page straight from the same clap definitions
that back `--help`, so it can't drift out of sync. Install it once, wherever
your system looks for section-1 pages:

```sh
exc man | sudo tee /usr/local/share/man/man1/exc.1 > /dev/null
man exc
```

Or view it without installing anything:

```sh
exc man | man -l -
```

`cargo install` doesn't run this for you — man pages aren't something crates
carry metadata for, so this is a manual (or packaging-script) step.

## License

[MIT](LICENSE)
