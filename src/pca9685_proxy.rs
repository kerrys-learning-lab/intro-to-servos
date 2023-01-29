use crate::{Config, Pca9685Proxy, PCA_PWM_RESOLUTION};
use linux_embedded_hal::i2cdev::linux::LinuxI2CError;
use linux_embedded_hal::I2cdev;
use pwm_pca9685::{Address, Channel, Error, OutputDriver, Pca9685 as Pca9685Impl};

const INTERNAL_OSC_HZ: f64 = 25.0 * 1000.0 * 1000.0; // 25 MHz

pub(super) struct Pca9685ProxyImpl {
    max_pw_ms: f64,
    single_count_duration_ms: f64,
    device: String,
    address: u8,
    output_frequency_hz: u16,
    prescale: u8,
    output_type: OutputDriver,
    inner: Option<Pca9685Impl<I2cdev>>,
}

impl Pca9685Proxy for Pca9685ProxyImpl {
    fn max_pw_ms(&self) -> f64 {
        return self.max_pw_ms;
    }

    fn single_count_duration_ms(&self) -> f64 {
        return self.single_count_duration_ms;
    }

    fn output_frequency_hz(&self) -> u16 {
        return self.output_frequency_hz;
    }

    fn device(&self) -> String {
        return self.device.clone();
    }

    fn address(&self) -> u8 {
        return self.address;
    }

    fn prescale(&self) -> u8 {
        return self.prescale;
    }

    fn output_type(&self) -> OutputDriver {
        return self.output_type;
    }

    fn set_channel_off_count(
        &mut self,
        channel: Channel,
        off: u16,
    ) -> Result<(), Error<LinuxI2CError>> {
        match &mut self.inner {
            Some(inner) => {
                log::info!("Calling set_channel_on_off({:?}, 0, {})", channel, off);
                inner.set_channel_on_off(channel, 0, off)
            }
            None => Ok(()),
        }
    }

    fn set_channel_full_on(&mut self, channel: Channel) -> Result<(), Error<LinuxI2CError>> {
        match &mut self.inner {
            Some(inner) => inner.set_channel_full_on(channel, 0),
            None => Ok(()),
        }
    }

    fn set_channel_full_off(&mut self, channel: Channel) -> Result<(), Error<LinuxI2CError>> {
        match &mut self.inner {
            Some(inner) => inner.set_channel_full_off(channel),
            None => Ok(()),
        }
    }
}

impl Pca9685ProxyImpl {
    pub(super) fn new(config: &Config) -> Box<dyn Pca9685Proxy> {
        let dev = I2cdev::new(&config.device)
            .unwrap_or_else(|_| panic!("Unable to load I2C device file: {}", config.device));

        let mut pca = Pca9685ProxyImpl::init(
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

        return Box::new(pca);
    }

    pub(super) fn mock(config: &Config) -> Box<dyn Pca9685Proxy> {
        return Box::new(Pca9685ProxyImpl::init(&config, None));
    }

    fn init(config: &Config, inner: Option<Pca9685Impl<I2cdev>>) -> Pca9685ProxyImpl {
        let cycle_duration_ms = 1000.0 / config.output_frequency_hz as f64;
        let single_count_duration_ms = cycle_duration_ms / PCA_PWM_RESOLUTION as f64;

        Pca9685ProxyImpl {
            max_pw_ms: cycle_duration_ms,
            single_count_duration_ms,
            device: config.device.clone(),
            address: config.address,
            output_frequency_hz: config.output_frequency_hz,
            prescale: Pca9685ProxyImpl::calculate_prescale(config.output_frequency_hz),
            output_type: if config.open_drain {
                OutputDriver::OpenDrain
            } else {
                OutputDriver::TotemPole
            },
            inner: inner,
        }
    }

    fn calculate_prescale(output_frequency_hz: u16) -> u8 {
        // Per PCA 9685 Datasheet, 7.3.5 PWM frequency PRE_SCALE:
        //    prescale_value = round(internal_osc/(4096 * output_frequency_hz)) - 1
        let value = INTERNAL_OSC_HZ / (PCA_PWM_RESOLUTION as f64 * output_frequency_hz as f64);
        let value = value.round() as u8 - 1;

        return value;
    }
}
