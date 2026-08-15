# simyo-esim — Usage Guide

A local CLI that fetches your Simyo eSIM activation code (LPA) directly from the
official API (`https://appapi.simyo.nl`). No third-party server sits between you
and Simyo: your password only leaves your machine for Simyo itself, is never
written to disk, and never appears in logs.

## Contents

- [Getting the binary](#getting-the-binary)
- [Quick start (all platforms)](#quick-start-all-platforms)
- [Linux](#linux)
- [macOS](#macos)
- [Windows](#windows)
- [Full eSTK / device-swap workflow](#full-estk--device-swap-workflow)
- [Commands](#commands)
- [Credentials & environment variables](#credentials--environment-variables)
- [Security notes](#security-notes)

---

## Getting the binary

### Option A: download a prebuilt release

Push a tag (`v0.1.0`) or trigger the `release` workflow manually — GitHub
Actions builds and attaches these artifacts to the release:

| File | Platform |
|------|----------|
| `simyo-esim-x86_64-unknown-linux-gnu` | Linux x86_64 (glibc) |
| `simyo-esim-aarch64-unknown-linux-gnu` | Linux ARM64 (glibc) |
| `simyo-esim-aarch64-apple-darwin` | macOS Apple Silicon (M1/M2/M3/M4) |
| `simyo-esim-x86_64-apple-darwin` | macOS Intel |
| `simyo-esim-x86_64-pc-windows-msvc.exe` | Windows x64 |

You can also download artifacts from any workflow run (Actions → run →
Artifacts) without creating a release.

### Option B: build from source (any platform)

```sh
# install Rust first: https://rustup.rs
git clone <repo-url>
cd simyo-esim-cli
cargo build --release
# binary at: target/release/simyo-esim (target/release/simyo-esim.exe on Windows)
```

---

## Quick start (all platforms)

```sh
simyo-esim get --phone 06xxxxxxxx --new-device --qr
```

Prompts (all hidden input, nothing is echoed or logged):

1. `Simyo password:` — your MijnSimyo password
2. `OTP code:` — only if your account has login MFA enabled (default for new accounts)
3. `Email validation code:` — the 6-digit code Simyo emails you after the
   device-change order is placed

Output:

```text
activationCode : 1$smdp.io$59-XXXXXX-XXXXXX
status         : ...
phoneNumber    : 06xxxxxxxx
iccid          : ...
LPA            : LPA:1$smdp.io$59-XXXXXX-XXXXXX
<QR code>
```

---

## Linux

```sh
# x86_64
chmod +x simyo-esim-x86_64-unknown-linux-gnu
./simyo-esim-x86_64-unknown-linux-gnu get --phone 06xxxxxxxx --new-device --qr

# ARM64 (Raspberry Pi, ARM servers, ...)
chmod +x simyo-esim-aarch64-unknown-linux-gnu
./simyo-esim-aarch64-unknown-linux-gnu get --phone 06xxxxxxxx
```

Notes:

- Requires glibc ≥ 2.31 (Ubuntu 20.04+ / Debian 11+ / Fedora 33+).
- Terminal QR: any modern terminal with Unicode support works
  (GNOME Terminal, Konsole, iTerm2, kitty, alacritty).
- For ARM64 devices, install the matching `aarch64` binary.

---

## macOS

```sh
# Apple Silicon
chmod +x simyo-esim-aarch64-apple-darwin
./simyo-esim-aarch64-apple-darwin get --phone 06xxxxxxxx --new-device --qr

# Intel
chmod +x simyo-esim-x86_64-apple-darwin
./simyo-esim-x86_64-apple-darwin get --phone 06xxxxxxxx
```

First-run Gatekeeper workaround (unsigned binary):

```sh
xattr -d com.apple.quarantine simyo-esim-aarch64-apple-darwin
```

or right-click → Open → Open in Finder once.

Notes:

- Terminal QR renders correctly in iTerm2 and Terminal.app.
- `rpassword` (hidden password input) works natively on macOS.

---

## Windows

```powershell
# PowerShell (Windows Terminal recommended)
.\simyo-esim-x86_64-pc-windows-msvc.exe get --phone 06xxxxxxxx --new-device --qr
```

Notes:

- **Use Windows Terminal** for `--qr` — legacy `cmd.exe`/PowerShell console
  garbles the Unicode half-blocks. The plain `LPA:` string works everywhere.
- SmartScreen may warn on first run ("unknown publisher") — click
  *More info → Run anyway*.
- Hidden password prompt works in all Windows consoles.
- If you prefer to run without downloading a binary, install Rust and use
  `cargo run --release -- get ...` from the project directory.

---

## Full eSTK / device-swap workflow

For a phone without native eSIM support (eSTK.me / 5ber card):

```sh
# 1. Create the eSIM profile order and fetch the activation code
simyo-esim get --phone 06xxxxxxxx --new-device --qr
#    → password, MFA OTP (if enabled), email validation code
#    → prints activationCode + LPA + QR

# 2. Open the eSTK app on the phone:
#    scan the QR (or type the LPA string) → profile downloads onto the card

# 3. ONLY AFTER the eSTK app shows the profile installed, confirm with Simyo:
simyo-esim login --phone 06xxxxxxxx     # prints a fresh session token
simyo-esim confirm --token <token>
```

Do **not** run step 3 before the card has actually downloaded the profile.

Notes:

- Plain `get` (without `--new-device`) only works if an order already exists
  (e.g. you started the swap in the official app). If it reports
  "no activationCode", rerun with `--new-device`.
- Each `--new-device` order **replaces** the previous eSIM profile — only run
  it when you actually want to move the line onto a new (e)SIM.
- Re-generate the QR anytime, offline:
  `simyo-esim lpa --code '1$smdp.io$59-XXXXXX-XXXXXX' --qr`

---

## Commands

```text
simyo-esim get       full flow: login → (MFA) → (device change) → fetch eSIM → LPA
simyo-esim login     login only; prints the session token on stdout
simyo-esim simcard   query /settings/simcard order status
simyo-esim lpa       build an LPA string / QR from an activation code
simyo-esim confirm   confirm eSIM installation
```

Useful flags (see `simyo-esim get --help` for all):

| Flag | Meaning |
|------|---------|
| `--phone 06xxxxxxxx` | NL phone number |
| `--password <pw>` | password on the command line (avoid; use the prompt) |
| `--token <t>` | reuse an existing session token, skips login |
| `--otp <6digits>` | MFA OTP (otherwise prompted) |
| `--code <6digits>` | email validation code (otherwise prompted) |
| `--new-device` | create a new eSIM profile order (device swap) |
| `--qr` | render the LPA QR code in the terminal |
| `--confirm` | confirm install right after fetching |

---

## Credentials & environment variables

| Variable | Used for |
|----------|----------|
| `SIMYO_PASSWORD` | password (falls back to interactive hidden prompt) |
| `SIMYO_PHONE` | phone number (falls back to prompt) |
| `SIMYO_SESSION_TOKEN` | session token (alternative to `--token`) |
| `RUST_LOG` | log level, e.g. `RUST_LOG=debug` (`-v` also works) |

Precedence: command-line flag → environment variable → interactive prompt.

```sh
# Fully non-interactive example (Linux/macOS):
SIMYO_PASSWORD='...' simyo-esim get --phone 0612863740 --new-device --otp 123456 --code 654321
```

---

## Security notes

- All traffic is direct HTTPS (`rustls`) to `appapi.simyo.nl`; certificate
  validation is enforced.
- The password exists only in memory during the login request. It is never
  logged, never persisted, and never sent anywhere but Simyo.
- Session tokens are redacted in logs (`abcd...wxyz`).
- A fresh random `X-Device-ID` is generated per run; nothing is stored on disk
  between runs.
- Session tokens printed by `login` are secrets: don't paste them into
  chat/issues.
- Prebuilt binaries are built from this repository by GitHub Actions; if you
  prefer full control, build from source (`cargo build --release`) — the only
  supported way to guarantee binary ↔ source correspondence.
