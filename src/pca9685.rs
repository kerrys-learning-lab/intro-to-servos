use crate::pca9685_proxy::Pca9685ProxyImpl;
use crate::{
    ChannelConfig, ChannelProxy, Config, Pca9685, Pca9685Error, Pca9685Proxy, Pca9685Result,
};
use log;
use pwm_pca9685::{Channel, OutputDriver};
use std::collections::HashMap;
use std::sync::Mutex;

unsafe impl Send for Pca9685 {}
unsafe impl Sync for Pca9685 {}

impl Pca9685 {
    /// Creates a new [Pca9685] utilizing the given [Config].
    pub fn new(config: &Config) -> Pca9685 {
        return Pca9685::init(config, Pca9685ProxyImpl::new(config));
    }

    /// Creates a **mock** [Pca9685] utilizing the given [Config].  Commands
    /// which *should* affect the PCA9685 output (e.g., [Pca9685::set_pwm_count],
    /// [Pca9685::set_pw_ms], and [Pca9685::set_pct]) actually have no effect.
    pub fn mock(config: &Config) -> Pca9685 {
        return Pca9685::init(config, Pca9685ProxyImpl::mock(config));
    }

    fn init(config: &Config, inner: Box<dyn Pca9685Proxy>) -> Pca9685 {
        let pca_count_length_ms = inner.single_count_duration_ms();
        let pca_max_pw_ms = inner.max_pw_ms();

        log::info!(target: "pca9685", "Device:           {}", config.device);
        log::info!(target: "pca9685", "Address:          {:#02x}", config.address);
        log::info!(target: "pca9685", "Output frequency: {}Hz", config.output_frequency_hz);
        log::info!(target: "pca9685", "Max PW:           {:0.4}ms", pca_max_pw_ms);
        log::info!(target: "pca9685", "Each count:       {:0.4}ms", pca_count_length_ms);

        let mut channels = HashMap::new();
        for ch in 0..16 {
            let channel = Channel::try_from(ch).unwrap();
            channels.insert(
                ch,
                ChannelProxy::new(channel, pca_count_length_ms, pca_max_pw_ms),
            );
        }

        Pca9685 {
            inner: Mutex::new(inner),
            channels: Mutex::new(channels),
        }
    }

    /// Returns the maximum pulse width (in milliseconds) given the configured
    /// output frequency of the [Pca9685].
    pub fn max_pw_ms(&self) -> f64 {
        return self.inner.lock().unwrap().max_pw_ms();
    }

    /// Returns the duration (in milliseconds) of a single pulse width count
    /// given the configured output frequency of the [Pca9685].
    pub fn single_count_duration_ms(&self) -> f64 {
        return self.inner.lock().unwrap().single_count_duration_ms();
    }

    /// Returns the configured output frequency (in Hz) of the [Pca9685].
    pub fn output_frequency_hz(&self) -> u16 {
        return self.inner.lock().unwrap().output_frequency_hz();
    }

    /// Returns the configured [Pca9685] device (e.g., `/dev/i2c-1`).
    pub fn device(&self) -> String {
        return self.inner.lock().unwrap().device();
    }

    /// Returns the configured address (e.g., `0x40`) of the [Pca9685].
    pub fn address(&self) -> u8 {
        return self.inner.lock().unwrap().address();
    }

    /// Returns the calculated prescale value given the configured output
    /// frequency of the [Pca9685].
    pub fn prescale(&self) -> u8 {
        return self.inner.lock().unwrap().prescale();
    }

    /// Returns the configured output type (e.g., `OpenDrain` / `TotemPole`) of
    /// the [Pca9685].
    pub fn output_type(&self) -> OutputDriver {
        return self.inner.lock().unwrap().output_type();
    }

    /// Returns the [ChannelConfig] of the requested `channel`.
    pub fn config(&self, channel: Channel) -> Pca9685Result<ChannelConfig> {
        let raw_channel = channel as u8;

        match self.channels.lock().unwrap().get(&raw_channel) {
            Some(ch) => Ok(ch.config()),
            None => Err(Pca9685Error::NoSuchChannelError(raw_channel)),
        }
    }

    /// Configures a channel given a [ChannelConfig].
    pub fn configure_channel(&self, config: ChannelConfig) -> Pca9685Result<ChannelConfig> {
        let raw_channel = config.channel as u8;

        match self.channels.lock().unwrap().get_mut(&raw_channel) {
            Some(ch) => Ok(ch.configure(&config)),
            None => Err(Pca9685Error::NoSuchChannelError(raw_channel)),
        }
    }

    /// Sets `channel` to full/continuous output, returning the resulting
    /// [ChannelConfig] containing the updated `current_count`.
    ///
    /// Ignores any configured ChannelCountLimits, if applicable.
    pub fn full_on(&self, channel: Channel) -> Pca9685Result<ChannelConfig> {
        let mut locked_pca_impl = self.inner.lock().unwrap();

        let raw_channel = channel as u8;

        match self.channels.lock().unwrap().get_mut(&raw_channel) {
            Some(ch) => ch.full_on(&mut locked_pca_impl),
            None => Err(Pca9685Error::NoSuchChannelError(raw_channel)),
        }
    }

    /// Sets `channel` to off (no output), returning the resulting
    /// [ChannelConfig] containing the updated `current_count` as None.
    ///
    /// Ignores any configured ChannelCountLimits, if applicable.
    ///
    /// Error conditions:
    /// * [Pca9685Error::Pca9685DriverError] if the underlying PCA 9685 driver
    /// yields an error
    pub fn full_off(&self, channel: Channel) -> Pca9685Result<ChannelConfig> {
        let mut locked_pca_impl = self.inner.lock().unwrap();

        let raw_channel = channel as u8;

        match self.channels.lock().unwrap().get_mut(&raw_channel) {
            Some(ch) => ch.full_off(&mut locked_pca_impl),
            None => Err(Pca9685Error::NoSuchChannelError(raw_channel)),
        }
    }

    /// Sets the `channel` output to `count` pulse counts, returning the resulting
    /// [ChannelConfig] containing the updated `current_count`.
    ///
    /// Error conditions:
    /// * [Pca9685Error::PulseWidthRangeError] if `count` is not within the
    /// limits of the PCA9685
    /// * [Pca9685Error::CustomLimitsError] if `count` is not within the channel's
    /// configured limits
    /// * [Pca9685Error::Pca9685DriverError] if the underlying PCA 9685 driver
    /// yields an error
    pub fn set_pwm_count(&self, channel: Channel, count: u16) -> Pca9685Result<ChannelConfig> {
        let mut locked_pca_impl = self.inner.lock().unwrap();

        let raw_channel = channel as u8;

        match self.channels.lock().unwrap().get_mut(&raw_channel) {
            Some(ch) => ch.set_pwm_count(count, &mut locked_pca_impl),
            None => Err(Pca9685Error::NoSuchChannelError(raw_channel)),
        }
    }

    /// Sets the `channel` output to `pw_ms` pulse width in milliseconds,
    /// returning the resulting [ChannelConfig] containing the updated
    /// `current_count`.
    ///
    /// Error conditions:
    /// * [Pca9685Error::PulseWidthRangeError] if `pw_ms` is not within the
    /// limits of the PCA9685 (based on the configured output frequency)
    /// * [Pca9685Error::CustomLimitsError] if `pw_ms` is not within the channel's
    /// configured limits
    /// * [Pca9685Error::Pca9685DriverError] if the underlying PCA 9685 driver
    /// yields an error
    pub fn set_pw_ms(&self, channel: Channel, pw_ms: f64) -> Pca9685Result<ChannelConfig> {
        let mut locked_pca_impl = self.inner.lock().unwrap();

        let raw_channel = channel as u8;

        match self.channels.lock().unwrap().get_mut(&raw_channel) {
            Some(ch) => ch.set_pw_ms(pw_ms, &mut locked_pca_impl),
            None => Err(Pca9685Error::NoSuchChannelError(raw_channel)),
        }
    }

    /// Sets the `channel` output to `pct` percent duty cycle (based on the
    /// channel's configured ChannelCountLimits, if applicable),
    /// returning the resulting [ChannelConfig] containing the updated
    /// `current_count`.
    ///
    /// Error conditions:
    /// * [Pca9685Error::PercentOfRangeError] if `pct` is not within [0.0, 1.0]
    /// * [Pca9685Error::Pca9685DriverError] if the underlying PCA 9685 driver
    /// yields an error
    pub fn set_pct(&self, channel: Channel, pct: f64) -> Pca9685Result<ChannelConfig> {
        let mut locked_pca_impl = self.inner.lock().unwrap();

        let raw_channel = channel as u8;

        match self.channels.lock().unwrap().get_mut(&raw_channel) {
            Some(ch) => ch.set_pct(pct, &mut locked_pca_impl),
            None => Err(Pca9685Error::NoSuchChannelError(raw_channel)),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{Config, Pca9685};
    use pwm_pca9685::OutputDriver;

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
        let single_count_duration_ms = expected_max_pw_ms / 4096.0;
        let expected_prescale = 30; // per PCA9685 documented example using 200Hz

        assert_eq!(pca.max_pw_ms(), expected_max_pw_ms);
        assert_eq!(pca.single_count_duration_ms(), single_count_duration_ms);
        assert_eq!(pca.device(), config.device);
        assert_eq!(pca.address(), config.address);
        assert_eq!(pca.output_frequency_hz(), config.output_frequency_hz);
        assert_eq!(pca.prescale(), expected_prescale);
        assert_eq!(pca.output_type(), OutputDriver::TotemPole);
    }
}
