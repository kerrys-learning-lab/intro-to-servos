use clap::Parser;
use env_logger;
use pca9685::{Config, Pca9685};
use pwm_pca9685::Channel;
use std::fs;

/// Simple program to interact with a PCA9685
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Channel
    #[arg(value_parser = clap::value_parser!(u8).range(..16))]
    channel: u8,

    /// Pulse width (ms)
    #[arg()]
    pulse_width_ms: f64,

    /// Path to configuration file
    #[arg(long, default_value = "/etc/pca9685.yaml")]
    config_file_path: String,
}

fn main() {
    env_logger::init();

    let args = Args::parse();

    let config = fs::read_to_string(args.config_file_path).unwrap();
    let config: Config = serde_yaml::from_str(&config).unwrap();
    let pca = Pca9685::new(&config);

    let channel = Channel::try_from(args.channel).unwrap();
    pca.set_pw_ms(channel, args.pulse_width_ms).unwrap();
}
