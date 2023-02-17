use clap::Parser;
use log;
use pca9685::{utils, ChannelConfig, Config, Pca9685, Pca9685Error};
use pwm_pca9685::Channel;
use rocket::http::Status;
use rocket::response::status;
use rocket::serde::{json::Json, Deserialize, Serialize};
use rocket::{Build, Rocket, State};
use strum::EnumString;

use pca9685::utils::{deserialize_channel, serialize_channel};

#[derive(Serialize)]
#[serde(crate = "rocket::serde")]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, PartialEq, EnumString, Serialize, Deserialize)]
enum StatusType {
    HEALTHY,
    DEGRADED,
}

#[derive(Serialize)]
#[serde(crate = "rocket::serde")]
struct SoftwareStatus {
    version: String,
}

#[derive(Serialize)]
#[serde(crate = "rocket::serde")]
struct StatusResponse {
    status: StatusType,
    software: SoftwareStatus,
}

#[derive(Debug, PartialEq, EnumString, Serialize, Deserialize)]
enum CommandType {
    FullOn,
    PulseCount,
    PulseWidth,
    Percent,
    FullOff,
}

#[derive(Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
struct ChannelCommand {
    #[serde(
        serialize_with = "serialize_channel",
        deserialize_with = "deserialize_channel"
    )]
    channel: Channel,
    command_type: CommandType,
    value: Option<f64>,
}

// #[derive(Deserialize)]
// #[serde(crate = "rocket::serde")]
// struct ChannelCommands {
//     commands: Vec<PulseWidthCommand>,
// }

/// RESTful interface to PCA9685
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to configuration file
    #[arg(long, default_value = "/etc/pca9685.yaml")]
    config_file_path: String,
}

#[macro_use]
extern crate rocket;

type HttpError = status::Custom<Json<ErrorResponse>>;
type HttpResult<T> = Result<Json<T>, HttpError>;

#[get("/status")]
fn get_status() -> HttpResult<StatusResponse> {
    Ok(Json(StatusResponse {
        status: StatusType::HEALTHY,
        software: SoftwareStatus {
            version: utils::built_info::PKG_VERSION.to_string(),
        },
    }))
}

fn extract_channel(path_channel: u8, body_channel: Channel) -> Result<Channel, HttpError> {
    if path_channel != (body_channel as u8) {
        return Err(status::Custom(
            Status::BadRequest,
            Json(ErrorResponse {
                error: format!(
                    "Request body channel ({:?}) doesn't match resource channel ({:?}).",
                    body_channel, path_channel
                ),
            }),
        ));
    }

    Ok(Channel::try_from(path_channel).unwrap())
}

fn extract_error(error: &Pca9685Error) -> status::Custom<Json<ErrorResponse>> {
    let error_code = match error {
        Pca9685Error::Pca9685DriverError(_) => Status::InternalServerError,
        _ => Status::BadRequest,
    };

    status::Custom(
        error_code,
        Json(ErrorResponse {
            error: error.to_string(),
        }),
    )
}

fn get_channel_config(channel: Channel, pca: &State<Pca9685>) -> HttpResult<ChannelConfig> {
    match pca.config(channel) {
        Ok(config) => match config.custom_limits {
            Some(_) => Ok(Json(config)),
            None => Err(status::Custom(
                Status::NotFound,
                Json(ErrorResponse {
                    error: String::from(format!("Channel {:?} not configured.", channel)),
                }),
            )),
        },
        Err(error) => Err(extract_error(&error)),
    }
}

#[get("/channel/<channel>")]
fn get_channel(channel: u8, pca: &State<Pca9685>) -> HttpResult<ChannelConfig> {
    get_channel_config(Channel::try_from(channel).unwrap(), pca)
}

#[post("/channel", format = "application/json", data = "<command>")]
fn post_channel(command: Json<ChannelConfig>, pca: &State<Pca9685>) -> HttpResult<ChannelConfig> {
    match pca.config(command.channel) {
        Ok(existing_config) => match existing_config.custom_limits {
            Some(_) => {
                return Err(status::Custom(
                    Status::Conflict,
                    Json(ErrorResponse {
                        error: String::from(format!(
                            "Channel {:?} already configured.",
                            command.channel
                        )),
                    }),
                ))
            }
            None => match pca.configure_channel(&command.into_inner()) {
                Ok(new_config) => Ok(Json(new_config)),
                Err(error) => Err(extract_error(&error)),
            },
        },
        Err(_) => {
            return Err(status::Custom(
                Status::NotFound,
                Json(ErrorResponse {
                    error: String::from(format!("Channel {:?} not found.", command.channel)),
                }),
            ))
        }
    }
}

#[put("/channel/<channel>", format = "application/json", data = "<command>")]
fn put_channel(
    channel: u8,
    command: Json<ChannelCommand>,
    pca: &State<Pca9685>,
) -> HttpResult<ChannelConfig> {
    let channel = extract_channel(channel, command.channel)?;

    // Assert channel is configured/exists
    get_channel_config(channel, pca)?;

    let value = match command.command_type {
        CommandType::PulseCount | CommandType::PulseWidth | CommandType::Percent => match command.value {
            Some(value) => value,
            None => {
                return Err(status::Custom(
                    Status::BadRequest,
                    Json(ErrorResponse {
                        error: String::from(
                            "Command body must contain 'value' when command_type is PulseCount | PulseWidth | Percent.",
                        ),
                    }),
                ))
            }
        },
        _ => match command.value {
            Some(_) => {
                return Err(status::Custom(
                    Status::BadRequest,
                    Json(ErrorResponse {
                        error: String::from(
                            "Command body may only contain 'value' when command_type is PulseCount | PulseWidth | Percent.",
                        ),
                    }),
                ))
            },
            None => 0.0
        },
    };

    let command_result = match command.command_type {
        CommandType::FullOn => pca.full_on(channel),
        CommandType::FullOff => pca.full_off(channel),
        CommandType::PulseCount => pca.set_pwm_count(channel, value as u16),
        CommandType::PulseWidth => pca.set_pw_ms(channel, value),
        CommandType::Percent => pca.set_pct(channel, value),
    };

    match command_result {
        Ok(config) => Ok(Json(config)),
        Err(error) => Err(extract_error(&error)),
    }
}

#[delete("/channel/<channel>")]
fn delete_channel(channel: u8, pca: &State<Pca9685>) -> HttpResult<ChannelConfig> {
    let channel = Channel::try_from(channel).unwrap();

    // Assert channel is configured/exists
    get_channel_config(channel, pca)?;

    match pca.configure_channel(&ChannelConfig {
        channel: channel,
        current_count: None,
        custom_limits: None,
    }) {
        Ok(config) => Ok(Json(config)),
        Err(error) => Err(extract_error(&error)),
    }
}

fn rocket(config: &Config, mock: bool) -> Rocket<Build> {
    let pca9685 = if mock {
        log::warn!(target: "server", "Using mock PCA9685 driver.");
        Pca9685::null(&config)
    } else {
        Pca9685::new(&config)
    };

    rocket::build()
        .mount(
            "/",
            routes![
                get_status,
                post_channel,
                put_channel,
                get_channel,
                delete_channel
            ],
        )
        .manage(pca9685)
}

#[rocket::main]
async fn main() -> Result<(), rocket::Error> {
    env_logger::init();

    let args = Args::parse();

    let config: Config = Config::load_from_file(&args.config_file_path);

    // Using conditional compilation..if the architecture is not ARM, use a mock PCA9685
    let force_mock = cfg!(not(any(target_arch = "arm", target_arch = "aarch64")));

    let _rocket = rocket(&config, force_mock).launch().await?;

    Ok(())
}

#[cfg(test)]
mod pca9685_server_test {
    use crate::{ChannelCommand, CommandType};

    use super::rocket;
    use pca9685::{ChannelConfig, ChannelCountLimits, Config, PCA_PWM_RESOLUTION};
    use pwm_pca9685::Channel;
    use rocket::http::{ContentType, Status};
    use rocket::local::blocking::Client;
    use rocket::serde::json;
    use rocket::{Build, Rocket};

    const TEST_CHANNEL_RAW_VALUE: u8 = 0;

    fn create_test_config() -> ChannelConfig {
        ChannelConfig {
            channel: Channel::try_from(TEST_CHANNEL_RAW_VALUE).unwrap(),
            current_count: None,
            custom_limits: Some(ChannelCountLimits {
                min_on_count: 1000,
                max_on_count: 2000,
            }),
        }
    }

    fn create_mock() -> Rocket<Build> {
        let config = Config {
            device: "/dev/foo".to_owned(),
            address: 0x40,
            output_frequency_hz: 200,
            open_drain: false,
            channels: Default::default(),
        };

        rocket(&config, true)
    }

    #[test]
    fn get_status() {
        let client = Client::tracked(create_mock()).expect("valid rocket instance");
        let response = client.get(uri!(super::get_status)).dispatch();
        assert_eq!(response.status(), Status::Ok);
    }

    #[test]
    fn configure_channel() {
        let client = Client::tracked(create_mock()).expect("valid rocket instance");
        let config = create_test_config();

        let response = client
            .post(uri!(super::post_channel()))
            .header(ContentType::JSON)
            .body(json::to_string(&config).unwrap())
            .dispatch();
        assert_eq!(response.status(), Status::Ok);

        let response_config = response.into_json::<ChannelConfig>().unwrap();

        assert_eq!(TEST_CHANNEL_RAW_VALUE, response_config.channel as u8);
        assert_eq!(
            config.custom_limits.unwrap(),
            response_config.custom_limits.unwrap()
        );
    }

    #[test]
    fn configure_channel_conflict() {
        let client = Client::tracked(create_mock()).expect("valid rocket instance");
        let config = create_test_config();

        let initial_response = client
            .post(uri!(super::post_channel()))
            .header(ContentType::JSON)
            .body(json::to_string(&config).unwrap())
            .dispatch();
        assert_eq!(initial_response.status(), Status::Ok);

        let duplicate_response = client
            .post(uri!(super::post_channel()))
            .header(ContentType::JSON)
            .body(json::to_string(&config).unwrap())
            .dispatch();
        assert_eq!(duplicate_response.status(), Status::Conflict);
    }

    #[test]
    fn get_channel() {
        let client = Client::tracked(create_mock()).expect("valid rocket instance");
        let config = create_test_config();

        let post_response = client
            .post(uri!(super::post_channel()))
            .header(ContentType::JSON)
            .body(json::to_string(&config).unwrap())
            .dispatch();
        assert_eq!(post_response.status(), Status::Ok);

        let get_response = client
            .get(uri!(super::get_channel(channel = TEST_CHANNEL_RAW_VALUE)))
            .dispatch();
        assert_eq!(get_response.status(), Status::Ok);

        let response_config = get_response.into_json::<ChannelConfig>().unwrap();

        assert_eq!(TEST_CHANNEL_RAW_VALUE, response_config.channel as u8);
        assert_eq!(
            config.custom_limits.unwrap(),
            response_config.custom_limits.unwrap()
        );
    }

    #[test]
    fn get_channel_not_found() {
        let client = Client::tracked(create_mock()).expect("valid rocket instance");

        let get_response = client
            .get(uri!(super::get_channel(channel = TEST_CHANNEL_RAW_VALUE)))
            .dispatch();
        assert_eq!(get_response.status(), Status::NotFound);
    }

    #[test]
    fn put_channel_full_on() {
        let client = Client::tracked(create_mock()).expect("valid rocket instance");
        let config = create_test_config();
        let command = ChannelCommand {
            channel: Channel::try_from(TEST_CHANNEL_RAW_VALUE).unwrap(),
            command_type: CommandType::FullOn,
            value: None,
        };

        let post_response = client
            .post(uri!(super::post_channel()))
            .header(ContentType::JSON)
            .body(json::to_string(&config).unwrap())
            .dispatch();
        assert_eq!(post_response.status(), Status::Ok);

        let put_response = client
            .put(uri!(super::put_channel(channel = TEST_CHANNEL_RAW_VALUE)))
            .header(ContentType::JSON)
            .body(json::to_string(&command).unwrap())
            .dispatch();
        assert_eq!(put_response.status(), Status::Ok);

        let response_config = put_response.into_json::<ChannelConfig>().unwrap();

        assert_eq!(TEST_CHANNEL_RAW_VALUE, response_config.channel as u8);
        assert_eq!(PCA_PWM_RESOLUTION, response_config.current_count.unwrap());
    }

    #[test]
    fn put_channel_full_on_bad_request() {
        let client = Client::tracked(create_mock()).expect("valid rocket instance");
        let config = create_test_config();
        let command = ChannelCommand {
            channel: Channel::try_from(TEST_CHANNEL_RAW_VALUE).unwrap(),
            command_type: CommandType::FullOn,
            value: Some(3.2),
        };

        let post_response = client
            .post(uri!(super::post_channel()))
            .header(ContentType::JSON)
            .body(json::to_string(&config).unwrap())
            .dispatch();
        assert_eq!(post_response.status(), Status::Ok);

        let put_response = client
            .put(uri!(super::put_channel(channel = TEST_CHANNEL_RAW_VALUE)))
            .header(ContentType::JSON)
            .body(json::to_string(&command).unwrap())
            .dispatch();
        assert_eq!(put_response.status(), Status::BadRequest);
    }

    #[test]
    fn put_channel_full_off() {
        let client = Client::tracked(create_mock()).expect("valid rocket instance");
        let config = create_test_config();
        let command = ChannelCommand {
            channel: Channel::try_from(TEST_CHANNEL_RAW_VALUE).unwrap(),
            command_type: CommandType::FullOff,
            value: None,
        };

        let post_response = client
            .post(uri!(super::post_channel()))
            .header(ContentType::JSON)
            .body(json::to_string(&config).unwrap())
            .dispatch();
        assert_eq!(post_response.status(), Status::Ok);

        let put_response = client
            .put(uri!(super::put_channel(channel = TEST_CHANNEL_RAW_VALUE)))
            .header(ContentType::JSON)
            .body(json::to_string(&command).unwrap())
            .dispatch();
        assert_eq!(put_response.status(), Status::Ok);

        let response_config = put_response.into_json::<ChannelConfig>().unwrap();

        assert_eq!(TEST_CHANNEL_RAW_VALUE, response_config.channel as u8);
        assert!(response_config.current_count.is_none());
    }

    #[test]
    fn put_channel_full_off_bad_request() {
        let client = Client::tracked(create_mock()).expect("valid rocket instance");
        let config = create_test_config();
        let command = ChannelCommand {
            channel: Channel::try_from(TEST_CHANNEL_RAW_VALUE).unwrap(),
            command_type: CommandType::FullOff,
            value: Some(3.2),
        };

        let post_response = client
            .post(uri!(super::post_channel()))
            .header(ContentType::JSON)
            .body(json::to_string(&config).unwrap())
            .dispatch();
        assert_eq!(post_response.status(), Status::Ok);

        let put_response = client
            .put(uri!(super::put_channel(channel = TEST_CHANNEL_RAW_VALUE)))
            .header(ContentType::JSON)
            .body(json::to_string(&command).unwrap())
            .dispatch();
        assert_eq!(put_response.status(), Status::BadRequest);
    }

    #[test]
    fn put_channel_pulse_count() {
        let client = Client::tracked(create_mock()).expect("valid rocket instance");
        let config = create_test_config();
        let command = ChannelCommand {
            channel: Channel::try_from(TEST_CHANNEL_RAW_VALUE).unwrap(),
            command_type: CommandType::PulseCount,
            value: Some(1500.0),
        };

        let post_response = client
            .post(uri!(super::post_channel()))
            .header(ContentType::JSON)
            .body(json::to_string(&config).unwrap())
            .dispatch();
        assert_eq!(post_response.status(), Status::Ok);

        let put_response = client
            .put(uri!(super::put_channel(channel = TEST_CHANNEL_RAW_VALUE)))
            .header(ContentType::JSON)
            .body(json::to_string(&command).unwrap())
            .dispatch();
        assert_eq!(put_response.status(), Status::Ok);

        let response_config = put_response.into_json::<ChannelConfig>().unwrap();

        assert_eq!(TEST_CHANNEL_RAW_VALUE, response_config.channel as u8);
        assert_eq!(1500, response_config.current_count.unwrap());
    }

    #[test]
    fn put_channel_pulse_count_beyond_limits() {
        let client = Client::tracked(create_mock()).expect("valid rocket instance");
        let config = create_test_config();
        let command = ChannelCommand {
            channel: Channel::try_from(TEST_CHANNEL_RAW_VALUE).unwrap(),
            command_type: CommandType::PulseCount,
            value: Some(3000.0),
        };

        let post_response = client
            .post(uri!(super::post_channel()))
            .header(ContentType::JSON)
            .body(json::to_string(&config).unwrap())
            .dispatch();
        assert_eq!(post_response.status(), Status::Ok);

        let put_response = client
            .put(uri!(super::put_channel(channel = TEST_CHANNEL_RAW_VALUE)))
            .header(ContentType::JSON)
            .body(json::to_string(&command).unwrap())
            .dispatch();
        assert_eq!(put_response.status(), Status::BadRequest);
    }

    #[test]
    fn put_channel_pulse_count_bad_request() {
        let client = Client::tracked(create_mock()).expect("valid rocket instance");
        let config = create_test_config();
        let command = ChannelCommand {
            channel: Channel::try_from(TEST_CHANNEL_RAW_VALUE).unwrap(),
            command_type: CommandType::PulseCount,
            value: None,
        };

        let post_response = client
            .post(uri!(super::post_channel()))
            .header(ContentType::JSON)
            .body(json::to_string(&config).unwrap())
            .dispatch();
        assert_eq!(post_response.status(), Status::Ok);

        let put_response = client
            .put(uri!(super::put_channel(channel = TEST_CHANNEL_RAW_VALUE)))
            .header(ContentType::JSON)
            .body(json::to_string(&command).unwrap())
            .dispatch();
        assert_eq!(put_response.status(), Status::BadRequest);
    }

    #[test]
    fn put_channel_pw_ms() {
        let client = Client::tracked(create_mock()).expect("valid rocket instance");
        let config = create_test_config();
        let command = ChannelCommand {
            channel: Channel::try_from(TEST_CHANNEL_RAW_VALUE).unwrap(),
            command_type: CommandType::PulseWidth,
            value: Some(1.831055),
        };

        let post_response = client
            .post(uri!(super::post_channel()))
            .header(ContentType::JSON)
            .body(json::to_string(&config).unwrap())
            .dispatch();
        assert_eq!(post_response.status(), Status::Ok);

        let put_response = client
            .put(uri!(super::put_channel(channel = TEST_CHANNEL_RAW_VALUE)))
            .header(ContentType::JSON)
            .body(json::to_string(&command).unwrap())
            .dispatch();
        assert_eq!(put_response.status(), Status::Ok);

        let response_config = put_response.into_json::<ChannelConfig>().unwrap();

        assert_eq!(TEST_CHANNEL_RAW_VALUE, response_config.channel as u8);
        assert_eq!(1500, response_config.current_count.unwrap());
    }

    #[test]
    fn put_channel_pw_ms_bad_request() {
        let client = Client::tracked(create_mock()).expect("valid rocket instance");
        let config = create_test_config();
        let command = ChannelCommand {
            channel: Channel::try_from(TEST_CHANNEL_RAW_VALUE).unwrap(),
            command_type: CommandType::PulseWidth,
            value: None,
        };

        let post_response = client
            .post(uri!(super::post_channel()))
            .header(ContentType::JSON)
            .body(json::to_string(&config).unwrap())
            .dispatch();
        assert_eq!(post_response.status(), Status::Ok);

        let put_response = client
            .put(uri!(super::put_channel(channel = TEST_CHANNEL_RAW_VALUE)))
            .header(ContentType::JSON)
            .body(json::to_string(&command).unwrap())
            .dispatch();
        assert_eq!(put_response.status(), Status::BadRequest);
    }

    #[test]
    fn put_channel_pct() {
        let client = Client::tracked(create_mock()).expect("valid rocket instance");
        let config = create_test_config();
        let command = ChannelCommand {
            channel: Channel::try_from(TEST_CHANNEL_RAW_VALUE).unwrap(),
            command_type: CommandType::Percent,
            value: Some(0.5),
        };

        let post_response = client
            .post(uri!(super::post_channel()))
            .header(ContentType::JSON)
            .body(json::to_string(&config).unwrap())
            .dispatch();
        assert_eq!(post_response.status(), Status::Ok);

        let put_response = client
            .put(uri!(super::put_channel(channel = TEST_CHANNEL_RAW_VALUE)))
            .header(ContentType::JSON)
            .body(json::to_string(&command).unwrap())
            .dispatch();
        assert_eq!(put_response.status(), Status::Ok);

        let response_config = put_response.into_json::<ChannelConfig>().unwrap();

        assert_eq!(TEST_CHANNEL_RAW_VALUE, response_config.channel as u8);
        assert_eq!(1500, response_config.current_count.unwrap());
    }

    #[test]
    fn put_channel_pct_bad_request() {
        let client = Client::tracked(create_mock()).expect("valid rocket instance");
        let config = create_test_config();
        let command = ChannelCommand {
            channel: Channel::try_from(TEST_CHANNEL_RAW_VALUE).unwrap(),
            command_type: CommandType::Percent,
            value: None,
        };

        let post_response = client
            .post(uri!(super::post_channel()))
            .header(ContentType::JSON)
            .body(json::to_string(&config).unwrap())
            .dispatch();
        assert_eq!(post_response.status(), Status::Ok);

        let put_response = client
            .put(uri!(super::put_channel(channel = TEST_CHANNEL_RAW_VALUE)))
            .header(ContentType::JSON)
            .body(json::to_string(&command).unwrap())
            .dispatch();
        assert_eq!(put_response.status(), Status::BadRequest);
    }

    #[test]
    fn put_channel_not_found() {
        let client = Client::tracked(create_mock()).expect("valid rocket instance");
        let command = ChannelCommand {
            channel: Channel::try_from(TEST_CHANNEL_RAW_VALUE).unwrap(),
            command_type: CommandType::Percent,
            value: None,
        };

        let put_response = client
            .put(uri!(super::put_channel(channel = TEST_CHANNEL_RAW_VALUE)))
            .header(ContentType::JSON)
            .body(json::to_string(&command).unwrap())
            .dispatch();
        assert_eq!(put_response.status(), Status::NotFound);
    }

    #[test]
    fn delete_channel() {
        let client = Client::tracked(create_mock()).expect("valid rocket instance");
        let config = create_test_config();

        let initial_response = client
            .post(uri!(super::post_channel()))
            .header(ContentType::JSON)
            .body(json::to_string(&config).unwrap())
            .dispatch();
        assert_eq!(initial_response.status(), Status::Ok);

        let delete_response = client
            .delete(uri!(super::delete_channel(
                channel = TEST_CHANNEL_RAW_VALUE
            )))
            .dispatch();
        assert_eq!(delete_response.status(), Status::Ok);

        let duplicate_response = client
            .post(uri!(super::post_channel()))
            .header(ContentType::JSON)
            .body(json::to_string(&config).unwrap())
            .dispatch();
        assert_eq!(duplicate_response.status(), Status::Ok);
    }

    #[test]
    fn delete_channel_not_found() {
        let client = Client::tracked(create_mock()).expect("valid rocket instance");

        let delete_response = client
            .delete(uri!(super::delete_channel(
                channel = TEST_CHANNEL_RAW_VALUE
            )))
            .dispatch();
        assert_eq!(delete_response.status(), Status::NotFound);
    }
}
