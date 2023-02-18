use crate::utils::{deserialize_channel, serialize_channel};
use linux_embedded_hal::i2cdev::linux::LinuxI2CError;
use pwm_pca9685::Channel;
use pwm_pca9685::OutputDriver;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;

mod channelproxy;
pub mod pca9685;
mod pca9685_proxy;
pub mod utils;

/// The PCA9685 has 4096 steps (12-bit PWM) of resolution
pub const PCA_PWM_RESOLUTION: u16 = 4096;

#[derive(Debug, Deserialize)]
/// An immutable YAML-based configuration of a [Pca9685] device.
pub struct Config {
    /// Path to I2C device file (e.g, /dev/i2c-1)
    pub device: String,

    /// Address of PCA9685 (e.g, 0x40)
    pub address: u8,

    /// PWM output frequency
    pub output_frequency_hz: u16,

    /// Open drain (if not set, use Totem pole)
    #[serde(default)]
    pub open_drain: bool,

    #[serde(default)]
    pub channels: Vec<ChannelConfig>,
}

#[derive(Deserialize, Serialize, PartialEq, Clone, Copy)]
pub struct ChannelLimits {
    pub count_limits: Option<ChannelCountLimits>,
    pub pw_limits: Option<ChannelMsLimits>,
}

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone, Copy)]
/// Constrains the limits of a Channel to values other than the default [0, 4095].
///
/// For example, a servo may be constrained to [1000, 3000] which then affects
/// the behavior of subsequent calls to [Pca9685::set_pwm_count],
/// [Pca9685::set_pw_ms], and [Pca9685::set_pct]
pub struct ChannelCountLimits {
    pub min_on_count: u16,
    pub max_on_count: u16,
}

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone, Copy)]
pub struct ChannelMsLimits {
    pub min_on_ms: f64,
    pub max_on_ms: f64,
}

#[derive(Deserialize, Serialize, Debug)]
/// Represents the desired and/or actual configuration of a Channel.
///
/// As an input, sets the `ChannelCountLimits` on the associated Channel (in
/// which case `current_count` is not used).
///
/// As an output, describes the current PWM count (`current_count`) and
/// configured limits (`custom_limits`), if any.
pub struct ChannelConfig {
    #[serde(
        serialize_with = "serialize_channel",
        deserialize_with = "deserialize_channel"
    )]
    pub channel: Channel,
    pub current_count: Option<u16>,
    pub custom_limits: Option<ChannelLimits>,
}

#[derive(PartialEq, Debug, Clone, Copy)]
struct PcaClockConfig {
    max_pw_ms: f64,
    single_pw_duration_ms: f64,
}

struct ChannelProxy {
    name: String,
    config: ChannelConfig,
    clock_config: PcaClockConfig,
}

trait Pca9685Proxy {
    fn max_pw_ms(&self) -> f64;

    fn single_count_duration_ms(&self) -> f64;

    fn output_frequency_hz(&self) -> u16;

    fn device(&self) -> String;

    fn address(&self) -> u8;

    fn prescale(&self) -> u8;

    fn output_type(&self) -> OutputDriver;

    fn set_channel_off_count(
        &mut self,
        channel: Channel,
        off: u16,
    ) -> Result<(), pwm_pca9685::Error<LinuxI2CError>>;

    fn set_channel_full_on(
        &mut self,
        channel: Channel,
    ) -> Result<(), pwm_pca9685::Error<LinuxI2CError>>;

    fn set_channel_full_off(
        &mut self,
        channel: Channel,
    ) -> Result<(), pwm_pca9685::Error<LinuxI2CError>>;
}

/// Provides access to a PCA9685 controller, with the ability to customize the
/// range of each Channel, and set each Channel's value using raw counts,
/// pulse width in milliseconds, or percent of max pulse width.
pub struct Pca9685 {
    inner: Mutex<Box<dyn Pca9685Proxy>>,
    channels: Mutex<HashMap<u8, ChannelProxy>>,
}

/// Represents the possible errors that may occur when commanding the [Pca9685].
pub enum Pca9685Error {
    NoSuchChannelError(u8),
    PulseWidthRangeError(f64, f64),
    CustomLimitsError(u16, ChannelLimits),
    InvalidConfiguration(String),
    PercentOfRangeError(f64),
    Pca9685DriverError(pwm_pca9685::Error<LinuxI2CError>),
}

/// Customized [Result], where the error type is [Pca9685Error]
pub type Pca9685Result<T> = Result<T, Pca9685Error>;
