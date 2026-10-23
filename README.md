# dbi-backend

A Rust USB backend for [DBI](https://github.com/rashevskyv/dbi), a Nintendo Switch
homebrew app for installing titles over USB. This is a reimplementation of the
Python `dbibackend.py`.

It listens for the Switch over USB (VID `0x057E`, PID `0x3000`), answers the DBI0
protocol commands `LIST`, `FILE_RANGE`, and `EXIT`, and streams the requested
byte ranges of your NSP files.

## Build

```bash
cargo build --release
```

## Usage

```bash
# Make a directory's files available and open the TUI
dbi-backend ~/games

# Or pass specific files
dbi-backend game1.nsp game2.nsp
```

Keys: `a` open the path picker, `c` clear the list, `s` start the server, `S`
browse network shares, `/` search the list, `q` quit. While the server is
running, `a`, `c`, and `S` are disabled so the offered list matches the
running server's snapshot. In the picker: `j`/`k` or the arrow keys move,
`Enter` opens a directory or adds a file, `Space` adds the highlighted directory
or file, `Backspace` goes up, and `Esc`/`q` cancels.

## Titles

The file list shows what each offered file is: its title id, the kind of
content it is (`base`, `update` or `dlc`) and its version. The title id and the
version come from the container's own metadata when the container carries it in
the clear; otherwise they are read from the conventional dump name
`Name [0100000000010000][v65536].nsp`. A title id that is not in the name is
classified by its last three digits: `000` is a base game, `800` an update,
anything else downloadable content.

The title's *name* is never read out of the container. Nintendo keeps it in the
content archive's header, which is encrypted with a key only the console holds,
so the backend shows the name the file already carries instead of asking for
console keys. The files offered to the Switch keep their names whatever the
backend knows about them.

## Filters

A directory scan offers only the files it accepts: by default every `.nsp` and
`.nsz` file. Change that from the command line or from the config file:

```bash
# Offer only .xci and .nsp files
dbi-backend --ext xci --ext nsp ~/games

# Offer everything except the patched copies
dbi-backend --exclude '*PATCH*' ~/games
```

```toml
[filters]
extensions = ["nsp", "nsz"]
include = ["*.nsp"]
exclude = ["*PATCH*"]
```

The command line wins over the config file, per setting: `--ext` overrides
`extensions` while the config file's `exclude` still applies. A pattern is a
glob matched against the *file name* (not the path), ignoring case; a bracket
introduces a character class, so `*[PATCH]*` matches any name holding a `P`,
an `A`, a `T`, a `C` or an `H`. The filters apply when a path is added, not to
what the picker lists, and a file named explicitly is always added.

The `/` key searches the offered list: typing narrows the rows the file list
shows, `Enter` keeps the search and closes the input, `Esc` clears it. The
search never changes what the Switch receives — it is a view, not a filter.

## Share browsing

Games can also install straight from an SMB share. Configure the shares in
`~/.config/dbi-backend/config.toml` (create the directory if it is missing):

```toml
[[shares]]
name = "nas"
server = "192.0.2.4" # or host:port; the port defaults to 445
share = "games"
username = "you"
# domain = "WORKGROUP" # optional
# root = "library/switch" # optional: where browsing starts
```

The config never contains a password. Press `S` in the TUI to list the
configured shares, pick one, and type its password. The password is asked at
every share connection, kept in memory only, and never logged — connection
failures name the share, not the secret. A successful connection opens the
same file picker over the share (starting at `root` when set); added files
show as share-qualified paths like `nas:/switch/game.nsp` and stream through
the share when the Switch asks for them. Opening a stale session retries once;
if the connection drops mid-transfer it is reported.

## Linux permissions

Opening the USB device requires access to the `/dev/bus/usb/...` node. Install the
provided udev rule and add your user to the `plugdev` group:

```bash
sudo cp contrib/99-nintendo-switch.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules && sudo udevadm trigger
sudo usermod -aG plugdev "$USER"   # log out and back in
```

## Protocol

Messages use a 16-byte little-endian header: `DBI0` magic, `cmd_type`, `cmd_id`,
`data_size`. See `crates/core/src/protocol.rs`.

## Development

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```
