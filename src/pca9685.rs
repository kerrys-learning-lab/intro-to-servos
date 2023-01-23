use linux_embedded_hal::I2cdev;
use log::{debug, info};
use pwm_pca9685::{Address, Channel, OutputDriver, Pca9685 as Pca9685Impl};
use serde::Deserialize;
use std::fmt;

#[derive(Debug, Deserialize)]
pub struct Config {
    /// Path to I2C device file (e.g, /dev/i2c-1)
    device: String,

    /// Address of PCA9685 (e.g, 0x40)
    address: u8,

    /// PWM output frequency
    output_frequency_hz: u16,

    /// Open drain (if not set, use Totem pole)
    #[serde(default)]
    open_drain: bool,
}

pub struct Pca9685 {
    pub max_pw_ms: f64,

    pub min_pw_ms: f64,

    inner: Pca9685Impl<I2cdev>,
}

#[derive(Debug, Clone)]
pub struct Pca9685Error {
    operation: String,
    msg: String,
}

impl fmt::Display for Pca9685Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Unable to complete operation: {} ({}).",
            self.operation, self.msg
        )
    }
}

impl Pca9685 {
    const INTERNAL_OSC_HZ: f64 = 25.0 * 1000.0 * 1000.0; // 25 MHz
    const RESOLUTION: i32 = 4096;

    pub fn new(config: Config) -> Pca9685 {
        info!("Opening I2C device: {}", config.device);
        let dev = I2cdev::new(config.device).unwrap();

        let cycle_duration_ms = 1000.0 / config.output_frequency_hz as f64;
        let duration_per_count_ms = cycle_duration_ms / Pca9685::RESOLUTION as f64;

        debug!(
            "Max PW: {:0.6}ms ... each count is {:0.6}ms",
            cycle_duration_ms, duration_per_count_ms
        );

        let mut pca = Pca9685 {
            max_pw_ms: cycle_duration_ms,
            min_pw_ms: duration_per_count_ms,
            inner: Pca9685Impl::new(dev, Address::from(config.address)).unwrap(),
        };

        pca.set_prescale(config.output_frequency_hz);

        pca.set_output_driver(config.open_drain);

        pca.inner.enable().unwrap();

        return pca;
    }

    pub fn set_pw_ms(&mut self, channel: Channel, pw_ms: f64) -> Result<(), Pca9685Error> {
        if pw_ms < 0.0 {
            return Err(Pca9685Error {
                operation: "set_pw_ms".to_owned(),
                msg: format!("Desired pulse width ({}ms) cannot be negative.", pw_ms).to_owned(),
            });
        } else if pw_ms > self.max_pw_ms {
            return Err(Pca9685Error {
                operation: "set_pw_ms".to_owned(),
                msg: format!(
                    "Desired pulse width ({}ms) exceeds maximum ({}ms).  Check output_frequency.",
                    pw_ms, self.max_pw_ms
                )
                .to_owned(),
            });
        }

        let pwm_counts = (pw_ms / self.min_pw_ms) as u16;

        debug!(
            "Setting channel {:?} to {} counts ({:0.6}ms)",
            channel,
            pwm_counts,
            pwm_counts as f64 * self.min_pw_ms
        );

        self.inner
            .set_channel_on_off(channel, 0, pwm_counts)
            .unwrap();

        return Ok(());
    }

    fn set_prescale(&mut self, output_frequency_hz: u16) {
        // Per PCA 9685 Datasheet, 7.3.5 PWM frequency PRE_SCALE:
        //    prescale_value = round(internal_osc/(4096 * output_frequency_hz)) - 1
        let value =
            Pca9685::INTERNAL_OSC_HZ / (Pca9685::RESOLUTION as f64 * output_frequency_hz as f64);
        let value = value.round() as u8 - 1;
        debug!(
            "Output frequency: {}Hz (pre_scale: {})",
            output_frequency_hz, value
        );

        self.inner.set_prescale(value).unwrap();
    }

    fn set_output_driver(&mut self, open_drain: bool) {
        let output_driver = if open_drain {
            OutputDriver::OpenDrain
        } else {
            OutputDriver::TotemPole
        };

        self.inner.set_output_driver(output_driver).unwrap();
    }
}
