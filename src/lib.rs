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
    max_pw_ms: f64,
    min_pw_ms: f64,
    device: String,
    address: u8,
    output_frequency_hz: u16,
    prescale: u8,
    output_type: OutputDriver,
    inner: Option<Pca9685Impl<I2cdev>>,
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

    pub fn new(config: &Config) -> Pca9685 {
        info!("Opening I2C device: {}", config.device);
        let dev = I2cdev::new(&config.device).unwrap();

        let mut pca = Pca9685::init(
            config,
            Some(Pca9685Impl::new(dev, Address::from(config.address)).unwrap()),
        );

        match &mut pca.inner {
            Some(pca_impl) => {
                pca_impl.set_prescale(pca.prescale).unwrap();
                pca_impl.set_output_driver(pca.output_type).unwrap();
                pca_impl.enable().unwrap();
            }
            None => {}
        }

        return pca;
    }

    pub fn mock(config: &Config) -> Pca9685 {
        return Pca9685::init(&config, None);
    }

    pub fn init(config: &Config, inner: Option<Pca9685Impl<I2cdev>>) -> Pca9685 {
        let cycle_duration_ms = 1000.0 / config.output_frequency_hz as f64;
        let duration_per_count_ms = cycle_duration_ms / Pca9685::RESOLUTION as f64;

        debug!(
            "Max PW: {:0.6}ms ... each count is {:0.6}ms",
            cycle_duration_ms, duration_per_count_ms
        );

        Pca9685 {
            max_pw_ms: cycle_duration_ms,
            min_pw_ms: duration_per_count_ms,
            device: config.device.clone(),
            address: config.address,
            output_frequency_hz: config.output_frequency_hz,
            prescale: Pca9685::calculate_prescale(config.output_frequency_hz),
            output_type: if config.open_drain {
                OutputDriver::OpenDrain
            } else {
                OutputDriver::TotemPole
            },
            inner: inner,
        }
    }

    pub fn output_frequency_hz(&self) -> u16 {
        return self.output_frequency_hz;
    }

    pub fn device(&self) -> &String {
        return &self.device;
    }

    pub fn address(&self) -> u8 {
        return self.address;
    }

    pub fn set_pw_ms(&mut self, channel: Channel, pw_ms: f64) -> Result<u16, Pca9685Error> {
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

        match &mut self.inner {
            Some(pca_impl) => pca_impl.set_channel_on_off(channel, 0, pwm_counts).unwrap(),
            None => {}
        }
        return Ok(pwm_counts);
    }

    fn calculate_prescale(output_frequency_hz: u16) -> u8 {
        // Per PCA 9685 Datasheet, 7.3.5 PWM frequency PRE_SCALE:
        //    prescale_value = round(internal_osc/(4096 * output_frequency_hz)) - 1
        let value =
            Pca9685::INTERNAL_OSC_HZ / (Pca9685::RESOLUTION as f64 * output_frequency_hz as f64);
        let value = value.round() as u8 - 1;
        debug!(
            "Output frequency: {}Hz (pre_scale: {})",
            output_frequency_hz, value
        );

        return value;
    }
}

#[cfg(test)]
mod tests {
    use crate::{Config, Pca9685, Pca9685Error};
    use pwm_pca9685::{Channel, OutputDriver};

    fn create_mock(output_frequency_hz: u16) -> (Config, Pca9685) {
        let config = Config {
            device: "/dev/foo".to_owned(),
            address: 0x40,
            output_frequency_hz: output_frequency_hz,
            open_drain: false,
        };

        let pca = Pca9685::mock(&config);

        return (config, pca);
    }

    #[test]
    fn init() {
        let test_output_frequency_hz = 200;

        let (config, pca) = create_mock(test_output_frequency_hz);

        let expected_max_pw_ms = 1000.0 / test_output_frequency_hz as f64;
        let expected_min_pw_ms = expected_max_pw_ms / 4096.0;
        let expected_prescale = 30; // per PCA9685 documented example using 200Hz

        assert_eq!(pca.max_pw_ms, expected_max_pw_ms);
        assert_eq!(pca.min_pw_ms, expected_min_pw_ms);
        assert_eq!(pca.device, config.device);
        assert_eq!(pca.address, config.address);
        assert_eq!(pca.output_frequency_hz, config.output_frequency_hz);
        assert_eq!(pca.prescale, expected_prescale);
        assert_eq!(pca.output_type, OutputDriver::TotemPole);
    }

    #[test]
    fn set_pw_ms() -> Result<(), Pca9685Error> {
        let test_output_frequency_hz = 200;

        let (_, mut pca) = create_mock(test_output_frequency_hz);

        // Test at min/max of range
        assert_eq!(pca.set_pw_ms(Channel::C0, 0.0)?, 0);
        assert_eq!(pca.set_pw_ms(Channel::C0, pca.max_pw_ms)?, 4096);

        // Test at percentages of range
        for pct in [0.25, 0.5, 0.75] {
            let test_pw_ms = pca.max_pw_ms * pct;
            let expected_counts = (4096.0 * pct) as u16;
            assert_eq!(pca.set_pw_ms(Channel::C0, test_pw_ms)?, expected_counts);
        }

        // Test a specific value, using formula
        for test_pw_ms in [1.0, 1.5, 2.0] {
            // Hz to to millis, so to speak
            let expected_count = 1000.0 / test_output_frequency_hz as f64;

            // Duration of each count, in millis
            let expected_count = expected_count / 4096.0;

            // Number of counts required for given test_pw_ms
            let expected_count = (test_pw_ms / expected_count) as u16;

            assert_eq!(pca.set_pw_ms(Channel::C0, test_pw_ms)?, expected_count);
        }

        return Ok(());
    }

    #[test]
    #[should_panic(expected = "cannot be negative")]
    fn set_pw_ms_neg() {
        let test_output_frequency_hz = 200;

        let (_, mut pca) = create_mock(test_output_frequency_hz);

        pca.set_pw_ms(Channel::C0, -1.0).unwrap();
    }

    #[test]
    #[should_panic(expected = "exceeds maximum")]
    fn set_pw_ms_too_large() {
        let test_output_frequency_hz = 200;

        let (_, mut pca) = create_mock(test_output_frequency_hz);

        pca.set_pw_ms(Channel::C0, pca.max_pw_ms + 1.0).unwrap();
    }
}
