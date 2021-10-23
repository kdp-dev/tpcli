use clap::{crate_version, App, Arg};
use db_key::Key;
use fs_extra::dir::{copy as copy_dir, CopyOptions};
use futures;
use hyper::{client::HttpConnector, Body, Client, Method, Request};
use hyper_tls::HttpsConnector;
use leveldb::{
    database::Database,
    iterator::Iterable,
    options::{Options, ReadOptions},
};
use serde::Deserialize;
use std::{cmp::Reverse, str::FromStr, time::SystemTime};
use std::{
    env, fs,
    io::{stdin, stdout, Write},
    path::{Path, PathBuf},
    str,
};
use tempfile::tempdir;

/// Used for keying leveldb.
#[derive(Debug, PartialEq)]
pub struct BytesKey {
    key: Vec<u8>,
}

impl Key for BytesKey {
    fn from_u8(key: &[u8]) -> Self {
        Self { key: key.to_vec() }
    }

    fn as_slice<T, F: Fn(&[u8]) -> T>(&self, f: F) -> T {
        f(self.key.as_slice())
    }
}

#[derive(Deserialize, Debug)]
struct PresenceToken {
    token: String,
    expiration: u64,
}

fn get_teams_db_path() -> PathBuf {
    if cfg!(target_os = "macos") {
        let home = PathBuf::from(env::var("HOME").unwrap_or(String::from("~")));
        home.join("Library")
            .join("Application Support")
            .join("Google")
            .join("Chrome")
            .join("Default")
            .join("Local Storage")
            .join("leveldb")

        // home.join("Library")
        //     .join("Application Support")
        //     .join("Microsoft")
        //     .join("Teams")
        //     .join("Local Storage")
        //     .join("leveldb")
    } else if cfg!(target_os = "windows") {
        let app_data = PathBuf::from(env::var("APPDATA").expect("APPDATA env var not found"));
        app_data
            .join("Microsoft")
            .join("Teams")
            .join("Local Storage")
            .join("leveldb")
    } else {
        panic!("Unsupported platform")
    }
}

#[derive(Debug)]
enum Error {
    PresenceTokenNotFound,
}

fn get_presence_token(db_path: &Path) -> Result<PresenceToken, Error> {
    let temp_db_dir = tempdir().unwrap();

    let options = CopyOptions::new();
    copy_dir(db_path, &temp_db_dir.path(), &options).expect("Error copying leveldb to temp dir");

    let leveldb_path = temp_db_dir.path().join("leveldb");
    let lock_file = leveldb_path.join("LOCK");
    if lock_file.exists() {
        fs::remove_file(&lock_file).expect("Failed to delete leveldb lock file");
    }

    let options = Options::new();
    let database = Database::<BytesKey>::open(&leveldb_path, options)
        .expect("Failed to open leveldb database");

    let cur_epoch = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let iter_read_opts = ReadOptions::new();
    let mut tokens: Vec<PresenceToken> = database
        .iter(iter_read_opts)
        .map(|(k, v)| (String::from_utf8(k.key).unwrap_or(String::from("")), v))
        .filter(|(k, _)| {
            k.starts_with("_https://teams.microsoft.com\u{0}\u{1}ts.")
                && k.ends_with(".cache.token.https://presence.teams.microsoft.com/")
        })
        .map(|(_, v)| -> PresenceToken {
            serde_json::from_slice(&v[1..]).expect("Failed to parse presence token info")
        })
        .filter(|token_info| token_info.expiration > cur_epoch)
        .collect();

    tokens.sort_by_key(|token| Reverse(token.expiration));

    if tokens.iter().count() >= 1 {
        Ok(tokens.remove(0))
    } else {
        Err(Error::PresenceTokenNotFound)
    }
}

#[derive(Debug)]
enum Presence {
    Available,
    Busy,
    DoNotDisturb,
    BeRightBack,
    Away,
    Offline,
    Reset,
}

impl FromStr for Presence {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "available" => Ok(Presence::Available),
            "busy" => Ok(Presence::Busy),
            "do_not_disturb" => Ok(Presence::DoNotDisturb),
            "be_right_back" => Ok(Presence::BeRightBack),
            "away" => Ok(Presence::Away),
            "offline" => Ok(Presence::Offline),
            "reset" => Ok(Presence::Reset),
            _ => Err("No match"),
        }
    }
}

impl ToString for Presence {
    fn to_string(&self) -> String {
        String::from_str(match self {
            Presence::Available => "available",
            Presence::Busy => "busy",
            Presence::DoNotDisturb => "do_not_disturb",
            Presence::BeRightBack => "be_right_back",
            Presence::Away => "away",
            Presence::Offline => "offline",
            Presence::Reset => "reset",
        })
        .unwrap()
    }
}

async fn set_availability(
    client: &Client<HttpsConnector<HttpConnector>>,
    token: &str,
    presence: &Presence,
) -> Result<(), hyper::http::Error> {
    let request_body = match presence {
        Presence::Available => "{\"availability\":\"Away\"}",
        Presence::Busy => "{\"availability\":\"Busy\"}",
        Presence::DoNotDisturb => "{\"availability\":\"DoNotDisturb\"}",
        Presence::BeRightBack => "{\"availability\":\"BeRightBack\"}",
        Presence::Away => "{\"availability\":\"Away\"}",
        Presence::Offline => "{\"availability\":\"Offline\",\"activity\":\"OffWork\"}",
        Presence::Reset => "",
    };
    let mut builder = Request::builder()
        .method(Method::PUT)
        .uri("https://presence.teams.microsoft.com/v1/me/forceavailability/")
        .header("Authorization", format!("Bearer {}", token));
    if request_body != "" {
        builder = builder.header("Content-Type", "application/json");
    } else {
        builder = builder.header("Content-Length", "0");
    }
    let request = builder.body(Body::from(request_body))?;

    let resp = client.request(request).await.unwrap();
    assert_eq!(resp.status(), 200);
    Ok::<(), hyper::http::Error>(())
}

async fn set_message(
    client: &Client<HttpsConnector<HttpConnector>>,
    token: &str,
    message: Option<&str>,
) -> Result<(), hyper::http::Error> {
    let request = Request::builder()
        .method(Method::PUT)
        .uri("https://presence.teams.microsoft.com/v1/me/publishnote")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(format!(
            "{{\"message\":\"{}\",\"expiry\":\"9999-12-31T05:00:00.000Z\"}}",
            message.unwrap_or("")
        )))?;

    let resp = client.request(request).await.unwrap();
    assert_eq!(resp.status(), 200);
    Ok::<(), hyper::http::Error>(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let matches = App::new("tpm (Teams Presence Management)")
        .version(crate_version!())
        .about("Easily manage your MS Teams presence")
        .arg(
            Arg::with_name("status")
                .possible_values(&[
                    "available",
                    "busy",
                    "do_not_disturb",
                    "be_right_back",
                    "away",
                    "offline",
                ])
                .takes_value(true)
                .required(true)
                .help("Teams status"),
        )
        .arg(
            Arg::with_name("message")
                .short("m")
                .long("message")
                .takes_value(true)
                .help("Teams status message to display"),
        )
        .get_matches();

    let presence_to_set = Presence::from_str(matches.value_of("status").unwrap()).unwrap();

    let default_path = get_teams_db_path();

    let token_info = get_presence_token(&default_path).expect("Failed to get token");

    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);

    let _ = futures::try_join!(
        set_availability(&client, &token_info.token, &presence_to_set),
        set_message(&client, &token_info.token, matches.value_of("message"))
    )?;

    print!(
        "Your status is {}{}. Press enter to clear: ",
        &presence_to_set.to_string(),
        match matches.value_of("message") {
            Some(v) => format!(" with message \"{}\"", v),
            None => "".to_string(),
        }
    );
    let _ = stdout().flush();
    let mut s = String::new();
    stdin().read_line(&mut s)?;

    let token_info = get_presence_token(&default_path).expect("Failed to get token");
    let _ = futures::try_join!(
        set_availability(&client, &token_info.token, &Presence::Reset),
        set_message(&client, &token_info.token, None)
    )?;

    println!("Your status has been reset");

    Ok(())
}
