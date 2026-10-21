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
browse network shares, `q` quit. While the server is running, `a`, `c`, and `S`
are disabled so the offered list matches the running server's snapshot. In the
picker: `j`/`k` or the arrow keys move, `Enter` opens a directory or adds a
file, `Space` adds the highlighted directory or file, `Backspace` goes up, and
`Esc`/`q` cancels.

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
