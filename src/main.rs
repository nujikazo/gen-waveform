use {
    clap::Parser,
    cpal::traits::{DeviceTrait, HostTrait, StreamTrait},
    rand::{rngs::StdRng, Rng, SeedableRng},
    std::f32::consts::PI,
    std::fmt,
    std::fmt::Display,
    std::str::FromStr,
    std::sync::{Arc, Mutex},
    std::time::Duration,
};

#[derive(Parser, Debug)]
#[clap(about, version, author)]
struct Args {
    #[clap(short, long, default_value_t = Waveform::Sine)]
    waveform: Waveform,

    #[clap(short, long, default_value_t = 440)]
    frequency: u32,

    #[clap(short, long, default_value_t = 1)]
    time: u64,

    #[clap(short, long, default_value_t = 0.5)]
    volume: f32,
}

#[derive(Debug, Copy, Clone)]
enum Waveform {
    Sine,
    Sawtooth,
    Triangle,
    Square,
    Noise,
}

impl FromStr for Waveform {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> anyhow::Result<Self, anyhow::Error> {
        match s.to_lowercase().as_str() {
            "sine" | "sin" => Ok(Waveform::Sine),
            "sawtooth" | "saw" => Ok(Waveform::Sawtooth),
            "triangle" | "tri" => Ok(Waveform::Triangle),
            "square" | "squ" => Ok(Waveform::Square),
            "noise" | "noi" => Ok(Waveform::Noise),
            _ => Err(anyhow::anyhow!("Unknown waveform: {}", s)),
        }
    }
}

impl Display for Waveform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Waveform::Sine => "sine",
            Waveform::Sawtooth => "sawtooth",
            Waveform::Triangle => "triangle",
            Waveform::Square => "square",
            Waveform::Noise => "noise",
        };
        write!(f, "{}", s)
    }
}

struct Oscillator {
    waveform: Waveform,
    frequency: f32,
    sample_rate: f32,
    phase: f32,
    volume: f32,
    rng: StdRng,
}

impl Oscillator {
    fn new(waveform: Waveform, frequency: f32, sample_rate: f32, volume: f32) -> Self {
        Self {
            waveform,
            frequency,
            sample_rate,
            phase: 0.0,
            volume: volume.clamp(0.0, 1.0),
            rng: StdRng::from_entropy(),
        }
    }

    fn next_sample(&mut self) -> f32 {
        let sample = match self.waveform {
            Waveform::Sine => self.sine(),
            Waveform::Sawtooth => self.sawtooth(),
            Waveform::Triangle => self.triangle(),
            Waveform::Square => self.square(),
            Waveform::Noise => self.white_noise(),
        };

        let phase_increment = self.frequency / self.sample_rate;
        self.phase += phase_increment;

        while self.phase >= 1.0 {
            self.phase -= 1.0;
        }

        sample * self.volume
    }

    fn sine(&self) -> f32 {
        (2.0 * PI * self.phase).sin()
    }

    fn sawtooth(&self) -> f32 {
        2.0 * self.phase - 1.0
    }

    fn triangle(&self) -> f32 {
        let p = self.phase;
        if p < 0.5 {
            4.0 * p - 1.0
        } else {
            3.0 - 4.0 * p
        }
    }

    fn square(&self) -> f32 {
        if self.phase < 0.5 {
            1.0
        } else {
            -1.0
        }
    }

    fn white_noise(&mut self) -> f32 {
        self.rng.gen_range(-1.0..=1.0)
    }

    #[allow(dead_code)]
    fn set_frequency(&mut self, frequency: f32) {
        self.frequency = frequency;
    }

    #[allow(dead_code)]
    fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
    }
}

fn main() -> Result<(), anyhow::Error> {
    let args = Args::parse();

    if args.volume < 0.0 || args.volume > 1.0 {
        return Err(anyhow::anyhow!("Volume must be between 0.0 and 1.0"));
    }

    let host = cpal::default_host();
    let output_device = host
        .default_output_device()
        .ok_or_else(|| anyhow::anyhow!("No default output device found"))?;

    println!("Output device: {}", output_device.name()?);

    let config = output_device.default_output_config()?;
    println!("Default output config: {:?}", config);
    println!(
        "Playing {} wave at {}Hz for {}s at {:.0}% volume",
        args.waveform,
        args.frequency,
        args.time,
        args.volume * 100.0
    );

    match config.sample_format() {
        cpal::SampleFormat::F32 => run::<f32>(&output_device, &config.into(), args),
        cpal::SampleFormat::I16 => run::<i16>(&output_device, &config.into(), args),
        cpal::SampleFormat::U16 => run::<u16>(&output_device, &config.into(), args),
    }
}

fn run<T>(
    output_device: &cpal::Device,
    config: &cpal::StreamConfig,
    args: Args,
) -> Result<(), anyhow::Error>
where
    T: cpal::Sample,
{
    let channels = config.channels as usize;
    let sample_rate = config.sample_rate.0 as f32;

    let oscillator = Arc::new(Mutex::new(Oscillator::new(
        args.waveform,
        args.frequency as f32,
        sample_rate,
        args.volume,
    )));

    let oscillator_clone = Arc::clone(&oscillator);

    let err_fn = |err| eprintln!("Audio stream error: {}", err);

    let output_data_fn = move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
        let mut osc = oscillator_clone.lock().unwrap();

        for frame in data.chunks_mut(channels) {
            let sample = osc.next_sample();
            let value: T = cpal::Sample::from(&sample);

            for channel_sample in frame.iter_mut() {
                *channel_sample = value;
            }
        }
    };

    let stream = output_device.build_output_stream(config, output_data_fn, err_fn)?;
    stream.play()?;

    std::thread::sleep(Duration::from_secs(args.time));

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oscillator_sine() {
        let mut osc = Oscillator::new(Waveform::Sine, 1.0, 4.0, 1.0);

        assert!((osc.next_sample()).abs() < 0.01);

        assert!((osc.next_sample() - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_oscillator_sawtooth() {
        let mut osc = Oscillator::new(Waveform::Sawtooth, 1.0, 4.0, 1.0);

        assert!((osc.next_sample() + 1.0).abs() < 0.01);
    }

    #[test]
    fn test_phase_wrapping() {
        let mut osc = Oscillator::new(Waveform::Sine, 1000.0, 1000.0, 1.0);

        for _ in 0..2000 {
            osc.next_sample();
        }

        assert!(osc.phase >= 0.0 && osc.phase < 1.0);
    }
}
