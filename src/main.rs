use clap::Parser;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::fmt;
use std::fmt::Display;
use std::str::FromStr;

#[derive(Parser, Debug)]
#[clap(about, version, author)]
struct Args {
    #[clap(short, long, default_value_t = Waveform::SINE)]
    waveform: Waveform,

    #[clap(short, long, default_value_t = 440)]
    frequency: u32,

    #[clap(short, long, default_value_t = 1)]
    time: u64,
}

#[derive(Debug, Copy, Clone)]
enum Waveform {
    SINE,
    SQUARE,
    TRIANGLE,
    SAWTOOTH,
    NOISE,
}

impl FromStr for Waveform {
    type Err = String;

    fn from_str(s: &str) -> anyhow::Result<Self, Self::Err> {
        match s {
            "sine" => Ok(Waveform::SINE),
            "sin" => Ok(Waveform::SINE),
            "square" => Ok(Waveform::SQUARE),
            "squ" => Ok(Waveform::SQUARE),
            "triangle" => Ok(Waveform::TRIANGLE),
            "tri" => Ok(Waveform::TRIANGLE),
            "sawtooth" => Ok(Waveform::SAWTOOTH),
            "saw" => Ok(Waveform::SAWTOOTH),
            "noise" => Ok(Waveform::NOISE),
            _ => Err(String::from("")),
        }
    }
}

impl Display for Waveform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match &self {
            Waveform::SINE => "sine",
            Waveform::SAWTOOTH => "sawtooth",
            Waveform::SQUARE => "square",
            Waveform::TRIANGLE => "triangle",
            Waveform::NOISE => "noise",
        };

        write!(f, "{}", s)
    }
}

fn main() -> anyhow::Result<()> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .expect("failed to find a default output device");
    println!("Output device: {}", device.name()?);

    let config = device.default_output_config()?;
    println!("Default output config: {:?}", config);

    let args = Args::parse();

    match config.sample_format() {
        cpal::SampleFormat::F32 => run::<f32>(&device, &config.into(), args),
        cpal::SampleFormat::I16 => run::<i16>(&device, &config.into(), args),
        cpal::SampleFormat::U16 => run::<u16>(&device, &config.into(), args),
    }
}

fn run<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    args: Args,
) -> Result<(), anyhow::Error>
where
    T: cpal::Sample,
{
    let sample_rate = config.sample_rate.0 as f32;
    let channels = config.channels as usize;
    let sample_clock = 0f32;
    let frequency = args.frequency as f32;

    let mut gen_fn: Box<dyn FnMut() -> f32 + Send> = match args.waveform {
        Waveform::SINE => gen_sine(sample_clock, sample_rate, frequency),
        Waveform::SAWTOOTH => gen_sawtooth(sample_clock, sample_rate, frequency),
        Waveform::SQUARE => gen_square(sample_clock, sample_rate, frequency),
        Waveform::TRIANGLE => gen_triangle(sample_clock, sample_rate, frequency),
        Waveform::NOISE => gen_white_noise(sample_clock, sample_rate),
    };

    let output_data_fn =
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| write_data(data, channels, &mut gen_fn);
    let err_fn = |err: cpal::StreamError| eprintln!("an error occurred on stream: {}", err);
    let stream = device.build_output_stream(config, output_data_fn, err_fn)?;

    stream.play()?;
    std::thread::sleep(std::time::Duration::from_secs(args.time));
    drop(stream);

    Ok(())
}

fn write_data<T>(output: &mut [T], channels: usize, next_sample: &mut dyn FnMut() -> f32)
where
    T: cpal::Sample,
{
    for frame in output.chunks_mut(channels) {
        let value: T = cpal::Sample::from::<f32>(&next_sample());
        for sample in frame.iter_mut() {
            *sample = value;
        }
    }
}

fn gen_base_waveform(sample_clock: f32, sample_rate: f32, frequency: f32, v: i32) -> f32 {
    (2.0 * std::f32::consts::PI * frequency * sample_clock * v as f32 / sample_rate).sin()
}

fn gen_sine(
    mut sample_clock: f32,
    sample_rate: f32,
    frequency: f32,
) -> Box<dyn FnMut() -> f32 + Send> {
    Box::new(move || {
        sample_clock = (sample_clock + 1.0) % sample_rate;
        gen_base_waveform(sample_clock, sample_rate, frequency, 1i32)
    }) as Box<dyn FnMut() -> f32 + Send>
}

fn gen_sawtooth(
    mut sample_clock: f32,
    sample_rate: f32,
    frequency: f32,
) -> Box<dyn FnMut() -> f32 + Send> {
    Box::new(move || {
        sample_clock = (sample_clock + 1.0) % sample_rate;
        let mut result = 0f32;

        for n in 1..50 {
            result += 1.0 / n as f32 * gen_base_waveform(sample_clock, sample_rate, frequency, n);
        }

        result
    }) as Box<dyn FnMut() -> f32 + Send>
}

fn gen_square(
    mut sample_clock: f32,
    sample_rate: f32,
    frequency: f32,
) -> Box<dyn FnMut() -> f32 + Send> {
    Box::new(move || {
        sample_clock = (sample_clock + 1.0) % sample_rate;
        let mut result = 0f32;

        for n in (1..50).step_by(2) {
            result += 1.0 / n as f32 * gen_base_waveform(sample_clock, sample_rate, frequency, n)
        }

        result
    }) as Box<dyn FnMut() -> f32 + Send>
}

fn gen_triangle(
    mut sample_clock: f32,
    sample_rate: f32,
    frequency: f32,
) -> Box<dyn FnMut() -> f32 + Send> {
    Box::new(move || {
        sample_clock = (sample_clock + 1.0) % sample_rate;
        let mut result = 0f32;

        for n in (1..50 as i32).step_by(2) {
            let p = n.pow(2u32);
            result += 1.0 / p as f32 * gen_base_waveform(sample_clock, sample_rate, frequency, p);
        }

        result
    }) as Box<dyn FnMut() -> f32 + Send>
}

fn gen_white_noise(mut sample_clock: f32, sample_rate: f32) -> Box<dyn FnMut() -> f32 + Send> {
    Box::new(move || {
        sample_clock = (sample_clock + 1.0) % sample_rate;
        let seed = rand::random::<u32>();
        let theta = seed as f32 / std::u32::MAX as f32 * 2f32 * std::f32::consts::PI;

        (2.0 * std::f32::consts::PI * theta * sample_clock / sample_rate).sin()
    }) as Box<dyn FnMut() -> f32 + Send>
}
