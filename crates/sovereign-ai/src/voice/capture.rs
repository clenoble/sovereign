use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::{traits::*, HeapRb};

/// Audio capture from the default input device.
/// Pushes f32 mono samples into a lock-free ring buffer.
pub struct AudioCapture {
    _stream: cpal::Stream,
}

/// Ring buffer consumer type for reading captured audio.
pub type AudioConsumer = ringbuf::HeapCons<f32>;

impl AudioCapture {
    /// Start capturing audio from the default input device.
    /// Returns the capture handle and a consumer for reading samples.
    ///
    /// The ring buffer holds ~30s of 16kHz mono audio (480,000 samples).
    pub fn start(target_sample_rate: u32) -> Result<(Self, AudioConsumer, u32)> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .context("No audio input device available")?;

        if let Ok(desc) = device.description() {
            tracing::info!("Audio input device: {:?}", desc);
        }

        let config = Self::find_config(&device, target_sample_rate)?;
        let actual_rate = config.sample_rate();
        let channels = config.channels() as usize;

        tracing::info!(
            "Audio config: {}Hz, {} channels, {:?}",
            actual_rate,
            channels,
            config.sample_format()
        );

        // ~30 seconds of mono audio at target rate
        let rb = HeapRb::<f32>::new(target_sample_rate as usize * 30);
        let (mut prod, cons) = rb.split();

        let stream = device
            .build_input_stream(
                &config.into(),
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if channels == 1 {
                        prod.push_slice(data);
                    } else {
                        for chunk in data.chunks(channels) {
                            let _ = prod.try_push(chunk[0]);
                        }
                    }
                },
                |err| {
                    tracing::error!("Audio capture error: {err}");
                },
                None,
            )
            .context("Failed to build input stream")?;

        stream.play().context("Failed to start audio capture")?;

        Ok((Self { _stream: stream }, cons, actual_rate))
    }

    fn find_config(
        device: &cpal::Device,
        target_rate: u32,
    ) -> Result<cpal::SupportedStreamConfig> {
        let configs = device
            .supported_input_configs()
            .context("Failed to query input configs")?;

        let mut best: Option<cpal::SupportedStreamConfigRange> = None;
        for cfg in configs {
            if cfg.sample_format() == cpal::SampleFormat::F32
                && cfg.min_sample_rate() <= target_rate
                && cfg.max_sample_rate() >= target_rate
            {
                if cfg.channels() == 1 {
                    return Ok(cfg.with_sample_rate(target_rate));
                }
                best = Some(cfg);
            }
        }

        if let Some(cfg) = best {
            return Ok(cfg.with_sample_rate(target_rate));
        }

        device
            .default_input_config()
            .context("No supported input config")
    }
}
