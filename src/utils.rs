use pwm_pca9685::Channel;
use serde::de::{self, Visitor};
use serde::{Deserializer, Serializer};
use std::fmt;

use crate::Pca9685Error;

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
                value, limits.min_on_count, limits.max_on_count
            ),
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
