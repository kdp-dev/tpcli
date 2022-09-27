use base64::{decode, encode};
use chrono::{DateTime, Duration, Local, SecondsFormat, Utc};
use clap::{crate_version, App, Arg};
use colored::*;
use db_key::Key;
use fs_extra::dir::{copy as copy_dir, CopyOptions};
use futures;
use humantime::parse_duration;
use hyper::{client::HttpConnector, Body, Client, Method, Request};
use hyper_tls::HttpsConnector;
use leveldb::{
    database::Database,
    iterator::Iterable,
    options::{Options, ReadOptions},
};
use rusqlite::{Connection, Result};
use serde::{ser::SerializeStruct, Deserialize, Serialize};
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

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct SkypeToken {
    skype_token: String,
    expiration: u64,
}

struct Jwt {
    token: String,
}

impl Jwt {
    fn exp(&self) -> u64 {
        let payload = self.token.split('.').nth(1).unwrap();
        let decoded_payload = decode(payload).unwrap();
        let decoded_payload = str::from_utf8(&decoded_payload).unwrap();
        let payload_object: serde_json::Value = serde_json::from_str(decoded_payload).unwrap();
        let exp = payload_object["exp"].as_u64().unwrap();
        exp
        // let now = SystemTime::now()
        //     .duration_since(SystemTime::UNIX_EPOCH)
        //     .unwrap()
        //     .as_secs();
        // let exp = exp - now;
        // println!("Token expires in {} seconds", exp);
    }
}

// enum Browser {
//     Chrome,
//     Teams,
// }

// enum PathType {
//     Leveldb,
//     Sqlite,
// }

fn teams_sqlite_path(partition: bool) -> PathBuf {
    let path = if cfg!(target_os = "macos") {
        let home = PathBuf::from(env::var("HOME").unwrap_or(String::from("~")));
        home.join("Library")
            .join("Application Support")
            .join("Microsoft")
            .join("Teams")
    } else if cfg!(target_os = "windows") {
        let app_data = PathBuf::from(env::var("APPDATA").expect("APPDATA env var not found"));
        app_data.join("Microsoft").join("Teams")
    } else {
        panic!("Unsupported platform")
    };

    let path = if partition {
        path.join("Partitions").join("msa")
    } else {
        path
    };

    path.join("Cookies")
}

fn chrome_leveldb_path() -> PathBuf {
    if cfg!(target_os = "macos") {
        let home = PathBuf::from(env::var("HOME").unwrap_or(String::from("~")));
        home.join("Library")
            .join("Application Support")
            .join("Google")
            .join("Chrome")
            .join("Default")
            .join("Local Storage")
            .join("leveldb")
    } else if cfg!(target_os = "windows") {
        let app_data = PathBuf::from(env::var("APPDATA").expect("APPDATA env var not found"));
        panic!("Haven't implemented chrome leveldb path for windows yet")
        // app_data
        //     .join("Microsoft")
        //     .join("Teams")
        //     .join("Local Storage")
        //     .join("leveldb")
    } else {
        panic!("Unsupported platform")
    }
}

#[derive(Debug)]
enum Error {
    PresenceTokenNotFound,
}

fn get_leveldb_tokens() -> (Option<PresenceToken>, Option<SkypeToken>) {
    let leveldb_path = chrome_leveldb_path();
    let temp_db_dir = tempdir().unwrap();
    let options = CopyOptions::new();
    copy_dir(leveldb_path, &temp_db_dir.path(), &options)
        .expect("Error copying leveldb to temp dir");

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
    // let mut tokens: Vec<SkypeToken> = database
    //     .iter(iter_read_opts)
    //     .map(|(k, v)| {
    //         // println!("{:?}", k);
    //         (String::from_utf8(k.key).unwrap_or(String::from("")), v)
    //     })
    //     .filter(|(k, _)| {
    //         println!("{:?}", k);
    //         k.contains("auth.skype.token")
    //         // k.starts_with("_https://teams.microsoft.com\u{0}\u{1}ts.")
    //         //     && k.ends_with(".cache.token.https://presence.teams.microsoft.com/")
    //     })
    //     .map(|(_, v)| -> SkypeToken {
    //         println!("{}", String::from_utf8(v.clone()).unwrap());
    //         serde_json::from_slice(&v[1..]).expect("Failed to parse presence token info")
    //     })
    //     .filter(|token_info| token_info.expiration > cur_epoch)
    //     .collect();

    let mut skype_tokens: Vec<SkypeToken> = Vec::default();
    let mut presence_tokens: Vec<PresenceToken> = Vec::default();
    for (key, value) in database.iter(iter_read_opts) {
        let key = String::from_utf8(key.key).unwrap_or(String::from(""));
        // let value2 = String::from_utf8(value.clone()).unwrap_or(String::from(""));
        // println!("{}: {}", key, value2);
        if key.ends_with("auth.skype.token") {
            let new_skype_token: SkypeToken =
                serde_json::from_slice(&value[1..]).expect("Failed to parse skype token info");
            // println!("Skype token hit: {:?}", &new_skype_token);
            if new_skype_token.expiration > cur_epoch {
                skype_tokens.push(new_skype_token)
            }
        } else if key.ends_with(".cache.token.https://presence.teams.microsoft.com/") {
            let new_presence_token: PresenceToken =
                serde_json::from_slice(&value[1..]).expect("Failed to parse presence token info");
            if new_presence_token.expiration > cur_epoch {
                presence_tokens.push(new_presence_token)
            }
        }
    }

    skype_tokens.sort_by_key(|token| Reverse(token.expiration));
    presence_tokens.sort_by_key(|token| Reverse(token.expiration));

    // println!("{:?}", skype_tokens);
    // println!("{:?}", presence_tokens);

    (
        presence_tokens.into_iter().next(),
        skype_tokens.into_iter().next(),
    )

    // if tokens.iter().count() >= 1 {
    //     // Ok(tokens.remove(0))
    //     Err(Error::PresenceTokenNotFound)
    // } else {
    //     Err(Error::PresenceTokenNotFound)
    // }
    // Err(Error::PresenceTokenNotFound)
}

fn get_sqlite_tokens() -> Jwt {
    let sqlite_path = teams_sqlite_path(true);
    let conn = Connection::open(sqlite_path).unwrap();
    let mut stmt = conn
        .prepare("select value from cookies where name = 'skypetoken_asm'")
        .unwrap();
    let mut tokens: Vec<Jwt> = stmt
        .query_map([], |row| Ok(row.get_unwrap(0)))
        .unwrap()
        .map(|res| Jwt {
            token: res.unwrap(),
        })
        .collect();

    tokens.sort_by_key(|token| Reverse(token.exp()));
    assert!(tokens.len() >= 1, "No tokens found in MS Teams cookie db");

    tokens.remove(0)
}

fn decode_urlenc(s: String) -> String {
    urlencoding::decode(&s).unwrap().into_owned()
}

fn get_auth_sqlite_tokens() -> Jwt {
    let sqlite_path = teams_sqlite_path(false);
    let conn = Connection::open(sqlite_path).unwrap();
    let mut stmt = conn
        .prepare("select value from cookies where name = 'authtoken'")
        .unwrap();
    let mut tokens: Vec<Jwt> = stmt
        .query_map([], |row| Ok(row.get_unwrap(0)))
        .unwrap()
        .map(|res| {
            let raw_token_info = res.unwrap();
            let token_info = decode_urlenc(raw_token_info);
            let bearer_pair = token_info.split("&").next().unwrap();
            Jwt {
                token: bearer_pair.split("=").last().unwrap().to_string(),
            }
        })
        .collect();

    tokens.sort_by_key(|token| Reverse(token.exp()));
    assert!(tokens.len() >= 1, "No tokens found in MS Teams cookie db");

    tokens.remove(0)
}

#[derive(Debug, Serialize)]
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

impl Presence {
    fn to_string_colored(&self) -> ColoredString {
        match self {
            Presence::Available => "available".green(),
            Presence::Busy => "busy".red(),
            Presence::DoNotDisturb => "do_not_disturb".red(),
            Presence::BeRightBack => "be_right_back".yellow(),
            Presence::Away => "away".yellow(),
            Presence::Offline => "offline".white(),
            Presence::Reset => "reset".clear(),
        }
    }
}

struct Availability<'a> {
    availability: &'a Presence,
    activity: Option<String>,
    desired_expiration_time: Option<DateTime<Utc>>,
}

impl Serialize for Availability<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("Availability", 3)?;
        state.serialize_field("availability", &self.availability)?;
        if let Some(activity) = &self.activity {
            state.serialize_field("activity", &activity)?;
        }
        if let Some(expiration) = &self.desired_expiration_time {
            state.serialize_field(
                "desiredExpirationTime",
                &expiration.to_rfc3339_opts(SecondsFormat::Millis, true),
            )?;
        }
        state.end()
    }
}

#[derive(Clone, Copy)]
enum AccountType {
    Microsoft,
    Live,
}

async fn set_availability(
    client: &Client<HttpsConnector<HttpConnector>>,
    token: &str,
    account_type: AccountType,
    presence: &Presence,
    expiration: Option<DateTime<Utc>>,
) -> Result<(), hyper::http::Error> {
    let availability = Availability {
        availability: presence,
        activity: match presence {
            &Presence::Offline => Some("OffWork".to_string()),
            _ => None,
        },
        desired_expiration_time: expiration,
    };

    let request_body = match presence {
        &Presence::Reset => "".to_string(),
        _ => serde_json::to_string(&availability).unwrap(),
    };

    let mut builder = Request::builder()
        .method(Method::PUT)
        .uri(format!(
            "https://presence.teams.{}.com/v1/me/forceavailability/",
            match account_type {
                AccountType::Microsoft => "microsoft",
                AccountType::Live => "live",
            }
        ))
        .header("x-ms-client-consumer-type", "teams4life");

    match account_type {
        AccountType::Microsoft => {
            builder = builder.header("Authorization", format!("Bearer {}", token));
        }
        AccountType::Live => {
            builder = builder.header("x-skypetoken", token);
        }
    }

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
    account_type: AccountType,
    message: Option<&str>,
    pin: bool,
    expiration: Option<DateTime<Utc>>,
) -> Result<(), hyper::http::Error> {
    let mut builder = Request::builder()
        .method(Method::PUT)
        .uri(format!(
            "https://presence.teams.{}.com/v1/me/publishnote",
            match account_type {
                AccountType::Microsoft => "microsoft",
                AccountType::Live => "live",
            }
        ))
        .header("x-ms-client-consumer-type", "teams4life")
        .header("Content-Type", "application/json");

    match account_type {
        AccountType::Microsoft => {
            builder = builder.header("Authorization", format!("Bearer {}", token));
        }
        AccountType::Live => {
            builder = builder.header("x-skypetoken", token);
        }
    }

    let request = builder.body(Body::from(format!(
        "{{\"message\":\"{}\",\"expiry\":\"{}\"}}",
        match message {
            Some(message) => format!(
                "{}{}",
                message,
                if pin { "<pinnednote></pinnednote>" } else { "" }
            ),
            None => "".to_string(),
        },
        match expiration {
            Some(expiration) => expiration.to_rfc3339_opts(SecondsFormat::Millis, true),
            None => "9999-12-31T05:00:00.000Z".to_string(),
        }
    )))?;

    let resp = client.request(request).await.unwrap();
    assert_eq!(resp.status(), 200);
    Ok::<(), hyper::http::Error>(())
}

#[derive(Clone, Copy)]
enum InstanceType {
    TeamsApp,
    Chrome,
}

async fn set_both(
    instance_type: InstanceType,
    account_type: AccountType,
    presence: &Presence,
    expiration: Option<DateTime<Utc>>,
    message: Option<&str>,
    pin: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let token: String;
    match instance_type {
        InstanceType::TeamsApp => {
            match account_type {
                AccountType::Microsoft => {
                    // let (presence_token, _) = get_leveldb_tokens();
                    let auth_token = get_auth_sqlite_tokens();
                    token = auth_token.token;
                    // panic!("non-live account Teams app not supported yet");
                }
                AccountType::Live => {
                    let skype_token = get_sqlite_tokens();
                    token = skype_token.token;
                }
            }
        }
        InstanceType::Chrome => {
            let (presence_token, skype_token) = get_leveldb_tokens();
            match account_type {
                AccountType::Microsoft => {
                    token = presence_token.expect("Missing presence token").token;
                }
                AccountType::Live => {
                    token = skype_token.expect("Missing skype token").skype_token;
                }
            }
        }
    }

    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);

    let _ = futures::try_join!(
        set_availability(&client, &token, account_type, &presence, expiration),
        set_message(&client, &token, account_type, message, pin, expiration)
    )?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // for person in person_iter {
    //     println!("Found person {:?}", person.unwrap());
    // }

    // std::process::exit(0);
    let matches = App::new("tpcli (Teams Presence CLI)")
        .version(crate_version!())
        .about("Easily control your Microsoft Teams presence with this CLI program")
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
            Arg::with_name("application-type")
                // .short("m")
                .long("--app")
                .possible_values(&[
                    "chrome",
                    "teams",
                ])
                .default_value("teams")
                .takes_value(true)
                .help("Application to get authentication token from (Google Chrome or Microsoft Teams app)"),
        )
        .arg(
            Arg::with_name("account-type")
                // .short("m")
                .long("--account")
                .possible_values(&[
                    "live",
                    "ms",
                ])
                .default_value("ms")
                .takes_value(true)
                .help("Type of Teams account you have: microsoft.com or live.com (personal account)"),
        )
        .arg(
            Arg::with_name("message")
                .short("m")
                .long("message")
                .takes_value(true)
                .help("Teams status message to display"),
        )
        .arg(
            Arg::with_name("pin")
                .short("p")
                .long("pin")
                .required(false)
                .takes_value(false)
                .requires("message")
                .help("Display my status message when people go to send me a message"),
        )
        .arg(
            Arg::with_name("time-duration")
                // .short("i")
                .long("--in")
                .takes_value(true)
                .help("Reset status and message after this amount of time (e.g. 10m)"),
        )
        .arg(
            Arg::with_name("expiration-time")
                // .short("a")
                .long("--at")
                .takes_value(true)
                .conflicts_with("time-duration")
                .help("Reset status and message at this time"),
        )
        .get_matches();

    let expiration_date_time: Option<DateTime<Utc>> = match matches.value_of("time-duration") {
        Some(duration) => {
            let now = Utc::now();
            let parsed_duration =
                parse_duration(duration).expect("Failed to parse `--in` arg duration");
            Some(now + Duration::from_std(parsed_duration).unwrap())
        }
        None => match matches.value_of("expiration-time") {
            Some(date_time_str) => {
                // DateTime::parse_from_str("8/5/1994 8:00 AM +00:00", "%m/%d/%Y %H:%M %p %:z")?;
                Some(DateTime::from(
                    DateTime::parse_from_str(date_time_str, "%m/%d/%Y %H:%M %p %:z")
                        .expect("Failed to parse `--at` date and time"),
                ))
            }
            None => None,
        },
    };

    let account_type = match matches.value_of("account-type").unwrap() {
        "live" => AccountType::Live,
        "ms" => AccountType::Microsoft,
        _ => panic!("Invalid account type"),
    };
    let instance_type = match matches.value_of("application-type").unwrap() {
        "chrome" => InstanceType::Chrome,
        "teams" => InstanceType::TeamsApp,
        _ => panic!("Invalid application type"),
    };
    let presence_to_set = Presence::from_str(matches.value_of("status").unwrap()).unwrap();

    // let default_path = get_teams_db_path();

    set_both(
        instance_type,
        account_type,
        &presence_to_set,
        expiration_date_time,
        matches.value_of("message"),
        matches.is_present("pin"),
    )
    .await?;

    print!(
        "Your status is {}{}{}.",
        &presence_to_set.to_string_colored(),
        match matches.value_of("message") {
            Some(v) => format!(" with message \"{}\"", v.cyan()),
            None => "".to_string(),
        },
        match expiration_date_time {
            Some(expiration) => format!(
                ", expiring at {}",
                DateTime::<Local>::from(expiration)
                    .format("%m/%d/%Y %I:%M %p")
                    .to_string()
                    .purple()
            ),
            None => "".to_string(),
        }
    );

    if expiration_date_time.is_some() {
        println!();
        return Ok(());
    }

    print!(" Press {} to clear: ", "enter".green());

    let _ = stdout().flush();
    let mut s = String::new();
    stdin().read_line(&mut s)?;

    // let (presence_token, skype_token) = get_leveldb_tokens(&default_path);
    set_both(
        instance_type,
        account_type,
        &Presence::Reset,
        None,
        None,
        false,
    )
    .await?;

    println!("Your status has been reset.");

    Ok(())
}
