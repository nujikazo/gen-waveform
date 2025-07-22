use crate::oscillator::{AudioParams, Oscillator};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SizedSample};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub struct AudioEngine {
    params: Arc<Mutex<AudioParams>>,
    sample_buffer: Arc<Mutex<Vec<f32>>>,
    should_quit: Arc<AtomicBool>,
}

impl AudioEngine {
    pub fn new(
        params: Arc<Mutex<AudioParams>>,
        sample_buffer: Arc<Mutex<Vec<f32>>>,
        should_quit: Arc<AtomicBool>,
    ) -> Self {
        Self {
            params,
            sample_buffer,
            should_quit,
        }
    }

    pub fn start(self) -> Result<thread::JoinHandle<Result<(), anyhow::Error>>, anyhow::Error> {
        // Initialize audio
        let host = cpal::default_host();
        let output_device = host
            .default_output_device()
            .ok_or_else(|| anyhow::anyhow!("No default output device found"))?;

        println!("Output device: {}", output_device.name()?);

        let config = output_device.default_output_config()?;
        println!("Default output config: {:?}", config);

        let thread_handle = thread::spawn(move || match config.sample_format() {
            cpal::SampleFormat::F32 => run::<f32>(
                &output_device,
                &config.into(),
                self.params,
                self.sample_buffer,
                self.should_quit,
            ),
            cpal::SampleFormat::I16 => run::<i16>(
                &output_device,
                &config.into(),
                self.params,
                self.sample_buffer,
                self.should_quit,
            ),
            cpal::SampleFormat::U16 => run::<u16>(
                &output_device,
                &config.into(),
                self.params,
                self.sample_buffer,
                self.should_quit,
            ),
            _ => Err(anyhow::anyhow!("Unsupported sample format")),
        });

        Ok(thread_handle)
    }
}

fn run<T>(
    output_device: &cpal::Device,
    config: &cpal::StreamConfig,
    params: Arc<Mutex<AudioParams>>,
    sample_buffer: Arc<Mutex<Vec<f32>>>,
    should_quit: Arc<AtomicBool>,
) -> Result<(), anyhow::Error>
where
    T: Sample + SizedSample + FromSample<f32>,
{
    let channels = config.channels as usize;
    let sample_rate = config.sample_rate.0 as f32;

    let mut oscillator = Oscillator::new(params, sample_rate, sample_buffer);
    oscillator.set_interpolation(true);
    oscillator.set_band_limited(true);

    let err_fn = |err| eprintln!("Audio stream error: {}", err);

    let output_data_fn = move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
        for frame in data.chunks_mut(channels) {
            let sample = oscillator.next_sample();
            let value = T::from_sample(sample);

            for channel_sample in frame.iter_mut() {
                *channel_sample = value;
            }
        }
    };

    let stream = output_device.build_output_stream(config, output_data_fn, err_fn, None)?;
    stream.play()?;

    // Keep stream alive until quit signal
    while !should_quit.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_millis(100));
    }

    Ok(())
}
