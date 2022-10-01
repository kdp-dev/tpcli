# tpcli (Teams Presence CLI)

<p align="center">
  <img alt="tpcli demo gif" src="img/tpcli_demo.gif">
</p>
<hr>
<p align="center">
  <img alt="Language: Rust" src="https://img.shields.io/badge/language-Rust-orange">
  <img alt="Platforms: macOS, Windows, and Linux" src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue">
  <img alt="GitHub release downloads" src="https://img.shields.io/github/downloads/kdp-dev/tpcli/v1.0.0/total">
  <img alt="Build binaries job status" src="https://github.com/kdp-dev/tpcli/actions/workflows/ci.yaml/badge.svg">
  <a href="https://twitter.com/kdp_dev"><img alt="Follow us on Twitter" src="https://img.shields.io/twitter/follow/kdp_dev?style=social"></a><br>
  Quickly manage your Microsoft Teams presence from the command line
</p>

## Features

- Control both your Teams status and message with one simple command.
- Specify a precise expiration time or duration on your status.
- Leave the expiration blank, and `tpcli` will wait for you to clear your status on-demand, by pressing the enter key.

## Pre-requisites

You must be logged into Teams on your computer, either in Google Chrome or the Teams app.

- `tpcli` uses auth tokens stored in Chrome/Electron cookies to authenticate itself.

## Installation

### macOS

M1:

```bash
sudo curl -sSL 'https://github.com/kdp-dev/tpcli/releases/download/v1.0.0/tpcli-aarch64-apple-darwin.tgz' | sudo tar xzv -C /usr/local/bin
```

Intel:

```bash
sudo curl -sSL 'https://github.com/kdp-dev/tpcli/releases/download/v1.0.0/tpcli-x86_64-apple-darwin.tgz' | sudo tar xzv -C /usr/local/bin
```

### Windows

Run from an Administrator powershell prompt:

```powershell
Invoke-WebRequest -Uri "https://github.com/kdp-dev/tpcli/releases/download/v1.0.0/tpcli-x86_64-pc-windows-msvc.zip" -OutFile "$env:temp\tpcli.zip"
Expand-Archive -Path "$env:temp\tpcli.zip" -DestinationPath C:\Windows
```

### Linux

x86_64:

```bash
sudo curl -sSL 'https://github.com/kdp-dev/tpcli/releases/download/v1.0.0/tpcli-x86_64-unknown-linux-musl.tgz' | sudo tar xzv -C /usr/local/bin
```

aarch64:

```bash
sudo curl -sSL 'https://github.com/kdp-dev/tpcli/releases/download/v1.0.0/tpcli-aarch64-unknown-linux-musl.tgz' | sudo tar xzv -C /usr/local/bin
```

## Examples

```bash
# Display message "Lunch break" with status `away`. Wait for user input to clear.
tpcli -m 'Lunch break' away

# Set pinned status and message, clearing after 1 hour.
# Get auth token for personal Teams account (live.com) from Chrome cookies
tpcli --account live --app chrome --in 1hr --pin -m 'Important meeting' do_not_disturb
```

## Usage

```
tpcli (Teams Presence CLI) 1.0.0
Easily control your Microsoft Teams presence with this CLI program

USAGE:
    tpcli [FLAGS] [OPTIONS] <status>

FLAGS:
    -h, --help       Prints help information
    -p, --pin        Display my status message when people go to send me a message
    -V, --version    Prints version information

OPTIONS:
        --account <account-type>    Type of Teams account you have: microsoft.com or live.com (personal account)
                                    [default: ms]  [possible values: live, ms]
        --app <application-type>    Application to get authentication token from (Google Chrome or Microsoft Teams app)
                                    [default: teams]  [possible values: chrome, teams]
        --at <expiration-time>      Reset status and message at this time
    -m, --message <message>         Teams status message to display
        --in <time-duration>        Reset status and message after this amount of time (e.g. 10m)

ARGS:
    <status>    Teams status [possible values: available, busy, do_not_disturb, be_right_back, away, offline]
```

## Copyright

Copyright (c) 2022 KDP Software. All Rights Reserved.

## Follow us on social media

<p align="center">
  <a href="https://twitter.com/KDP_dev">Twitter</a>
  | <a href="https://www.youtube.com/channel/UCOKUOMU1cSvcgnyga8atl-g">YouTube</a>
  | <a href="https://www.instagram.com/kdp_software/">Instagram</a>
</p>
