# tpcli (Teams Presence CLI)

<p align="center">
  <img alt="tpcli demo gif" src="img/tpcli_demo.gif">
</p>
<hr>
<p align="center">
  <img alt="Build binaries job status" src="https://github.com/kdp-dev/tpcli/actions/workflows/ci.yaml/badge.svg">
  <img alt="Language: Rust" src="https://img.shields.io/badge/language-Rust-orange"><br>
  Quickly manage your Microsoft Teams presence from the CLI
</p>

## Features

- Control both you Teams status and message with one simple command.
- Specify a precise expiration time or duration on your status.
- Leave the expiration blank, and `tpcli` will wait for you to clear your status, on-demand, by pressing the enter key.

## Pre-requisites

You must be logged into Teams on your computer, either in Google Chrome or the Teams app.

- `tpcli` uses auth tokens stored in Chrome/Electron cookies to authenticate itself.

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
