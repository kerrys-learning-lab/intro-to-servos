use pwm_pca9685::Channel;
use serde::de::{self, Visitor};
use serde::{Deserializer, Serializer};
use std::{fmt, fs};

use crate::{
    ChannelConfig, ChannelCountLimits, ChannelLimits, ChannelPulseWidthLimits, Config,
    Pca9685Error, Pca9685Result, PcaClockConfig, PCA_PWM_RESOLUTION,
};

impl Config {
    pub fn load_from_file(path: &String) -> Config {
        let config = fs::read_to_string(path).unwrap();

        serde_yaml::from_str(&config).unwrap()
    }
}

impl ChannelConfig {
    pub fn limits(&self) -> (u16, u16) {
        match self.custom_limits {
            Some(limits) => limits.count_limits(),
            None => (0, PCA_PWM_RESOLUTION),
        }
    }
}

impl PcaClockConfig {
    pub fn pw_to_count(&self, pw_ms: f64) -> Result<u16, Pca9685Error> {
        if pw_ms < 0.0 || pw_ms > self.max_pw_ms {
            return Err(Pca9685Error::PulseWidthRangeError(pw_ms, self.max_pw_ms));
        }

        Ok((pw_ms / self.single_pw_duration_ms) as u16)
    }
}

impl ChannelLimits {
    pub fn from_count_limits(min_on_count: u16, max_on_count: u16) -> Self {
        Self {
            count_limits: Some(ChannelCountLimits {
                min_on_count: min_on_count,
                max_on_count: max_on_count,
            }),
            pw_limits: None,
        }
    }

    pub(crate) fn from_pw_limits(
        min_on_pw_ms: f64,
        max_on_pw_ms: f64,
        clock_config: PcaClockConfig,
    ) -> Self {
        Self {
            count_limits: Some(ChannelCountLimits {
                min_on_count: clock_config.pw_to_count(min_on_pw_ms).unwrap(),
                max_on_count: clock_config.pw_to_count(max_on_pw_ms).unwrap(),
            }),
            pw_limits: Some(ChannelPulseWidthLimits {
                min_on_ms: min_on_pw_ms,
                max_on_ms: max_on_pw_ms,
            }),
        }
    }

    /// Returns true if `value` is within [`min_on_count`, `max_on_count`]
    pub fn is_valid(&self, value: u16) -> bool {
        self.count_limits.unwrap().is_valid(value)
    }

    pub fn count_limits(&self) -> (u16, u16) {
        // count_limits should always be valid, because pw_limits are converted
        // to count_limits
        (
            self.count_limits.unwrap().min_on_count,
            self.count_limits.unwrap().max_on_count,
        )
    }

    pub fn pct_to_count(&self, pct: f64) -> Pca9685Result<u16> {
        if pct < 0.0 || pct > 1.0 {
            return Err(Pca9685Error::PercentOfRangeError(pct));
        }

        let (min_on_count, max_on_count) = self.count_limits();
        let pwm_range_width = max_on_count - min_on_count;
        let scaled_pwm_pct = pwm_range_width as f64 * pct;

        Ok(scaled_pwm_pct as u16 + min_on_count)
    }
}

impl fmt::Debug for ChannelLimits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (min_on_count, max_on_count) = self.count_limits();

        match &self.pw_limits {
            Some(pw_limits) => {
                write!(
                    f,
                    "[{}ms, {}ms) ( [{}, {}) )",
                    pw_limits.min_on_ms, pw_limits.max_on_ms, min_on_count, max_on_count
                )
            }
            None => match self.count_limits {
                Some(_) => write!(f, "[{}, {})", min_on_count, max_on_count),
                None => Ok(()),
            },
        }
    }
}

impl Default for ChannelCountLimits {
    fn default() -> Self {
        Self {
            min_on_count: 0,
            max_on_count: PCA_PWM_RESOLUTION,
        }
    }
}

impl ChannelCountLimits {
    /// Returns true if `value` is within [`min_on_count`, `max_on_count`]
    pub fn is_valid(&self, value: u16) -> bool {
        value >= self.min_on_count && value <= self.max_on_count
    }
}

impl Default for ChannelLimits {
    fn default() -> Self {
        Self {
            count_limits: Some(Default::default()),
            pw_limits: None,
        }
    }
}

impl fmt::Debug for Pca9685Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &*self {
            Pca9685Error::NoSuchChannelError(channel) => write!(
                f,
                "Invalid channel: {}.  Valid channels are [0,16).",
                channel
            ),
            Pca9685Error::PulseWidthRangeError(value, max_pw_ms) => write!(
                f,
                "Pulse width value ({}ms) must be within the limits [0, {}].",
                value, max_pw_ms
            ),
            Pca9685Error::CustomLimitsError(value, limits) => write!(
                f,
                "Value ({}) must be within the limits [{}, {}].",
                value,
                limits.count_limits().0,
                limits.count_limits().1
            ),
            Pca9685Error::InvalidConfiguration(msg) => write!(f, "Invalid configuration: {}", msg),
            Pca9685Error::PercentOfRangeError(value) => write!(
                f,
                "Percentage value ({:0.4}) must be within the limits [0.0, 1.0]",
                value
            ),
            Pca9685Error::Pca9685DriverError(error) => {
                write!(
                    f,
                    "An error occurred with the underlying PCA9685 driver: {:?}",
                    error
                )
            }
        }
    }
}

impl fmt::Display for Pca9685Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub fn serialize_channel<S>(channel: &Channel, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_u8(*channel as u8)
}

struct ChannelVisitor;
impl<'de> Visitor<'de> for ChannelVisitor {
    type Value = Channel;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an integer between 0 and 15, inclusive")
    }

    fn visit_u8<E>(self, value: u8) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Channel::try_from(value).unwrap())
    }

    fn visit_u16<E>(self, value: u16) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.visit_u8(value as u8)
    }

    fn visit_u32<E>(self, value: u32) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.visit_u8(value as u8)
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.visit_u8(value as u8)
    }
}

pub fn deserialize_channel<'de, D>(deserializer: D) -> Result<Channel, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_u8(ChannelVisitor)
}

pub mod built_info {
    // The file has been placed there by the build script.
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}
