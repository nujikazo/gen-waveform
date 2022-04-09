use {
    clap::Parser,
    cpal::traits::{DeviceTrait, HostTrait, StreamTrait},
    std::f32::consts::PI,
    std::fmt,
    std::fmt::Display,
    std::str::FromStr,
    std::u32::MAX,
};

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
    SAWTOOTH,
    TRIANGLE,
    SQUARE,
    NOISE,
}

impl FromStr for Waveform {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> anyhow::Result<Self, anyhow::Error> {
        match s {
            "sine" => Ok(Waveform::SINE),
            "sin" => Ok(Waveform::SINE),
            "sawtooth" => Ok(Waveform::SAWTOOTH),
            "saw" => Ok(Waveform::SAWTOOTH),
            "triangle" => Ok(Waveform::TRIANGLE),
            "tri" => Ok(Waveform::TRIANGLE),
            "square" => Ok(Waveform::SQUARE),
            "squ" => Ok(Waveform::SQUARE),
            "noise" => Ok(Waveform::NOISE),
            "noi" => Ok(Waveform::NOISE),
            _ => Err(anyhow::anyhow!("Unknown waveform")),
        }
    }
}

impl Display for Waveform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match &self {
            Waveform::SINE => "sine",
            Waveform::SAWTOOTH => "sawtooth",
            Waveform::TRIANGLE => "triangle",
            Waveform::SQUARE => "square",
            Waveform::NOISE => "noise",
        };

        write!(f, "{}", s)
    }
}

struct WaveformRequest {
    frequency: f32,
    sample_clock: f32,
    sample_rate: f32,
}

impl WaveformRequest {
    fn new(frequency: f32, sample_clock: f32, sample_rate: f32) -> Self {
        Self {
            frequency,
            sample_clock,
            sample_rate,
        }
    }

    fn base_waveform(&mut self, value: f32, frequency: f32, sample_rate: f32) -> f32 {
        (2f32 * PI * frequency * self.sample_clock * value / self.sample_rate + sample_rate).sin()
    }

    fn tick(&mut self) {
        self.sample_clock = (self.sample_clock + 1f32) % self.sample_rate;
    }

    fn sine(mut self) -> Box<dyn FnMut() -> f32 + Send> {
        Box::new(move || {
            self.tick();
            self.base_waveform(1f32, self.frequency, 0f32)
        })
    }

    fn sawtooth(mut self) -> Box<dyn FnMut() -> f32 + Send> {
        Box::new(move || {
            self.tick();
            let mut result = 0f32;

            for n in 1..50 {
                result += 1f32 / n as f32 * self.base_waveform(n as f32, self.frequency, 0f32);
            }

            result
        })
    }

    fn square(mut self) -> Box<dyn FnMut() -> f32 + Send> {
        Box::new(move || {
            self.tick();
            let mut result = 0f32;

            for n in (1..50).step_by(2) {
                result += 1f32 / n as f32 * self.base_waveform(n as f32, self.frequency, 0f32);
            }

            result
        })
    }

    fn triangle(mut self) -> Box<dyn FnMut() -> f32 + Send> {
        Box::new(move || {
            self.tick();
            let mut result = 0f32;

            for n in (1..50 as i32).step_by(2) {
                let p: f32 = n.pow(2) as f32;
                result += 1f32 / p * self.base_waveform(p, self.frequency, 0f32);
            }

            result
        })
    }

    fn white_noise(mut self) -> Box<dyn FnMut() -> f32 + Send> {
        Box::new(move || {
            self.tick();
            let seed = rand::random::<u32>();
            let theta = seed as f32 / MAX as f32 * 2f32 * PI;

            self.base_waveform(1f32, theta, theta)
        })
    }
}

fn main() -> Result<(), anyhow::Error> {
    let host = cpal::default_host();
    let output_device = host
        .default_output_device()
        .expect("failed to find a default output device");
    println!("Output device: {}", output_device.name()?);

    let config = output_device.default_output_config()?;
    println!("Default output config: {:?}", config);

    let args = Args::parse();

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
    let waveform_req =
        WaveformRequest::new(args.frequency as f32, 0f32, config.sample_rate.0 as f32);
    let mut waveform_fn: Box<dyn FnMut() -> f32 + Send> = match args.waveform {
        Waveform::SINE => waveform_req.sine(),
        Waveform::SAWTOOTH => waveform_req.sawtooth(),
        Waveform::TRIANGLE => waveform_req.triangle(),
        Waveform::SQUARE => waveform_req.square(),
        Waveform::NOISE => waveform_req.white_noise(),
    };
    let err_fn = |err: cpal::StreamError| eprintln!("an error occurred on stream: {}", err);
    let output_data_fn = move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
        write_data(data, channels, &mut waveform_fn)
    };
    let stream = output_device.build_output_stream(config, output_data_fn, err_fn)?;

    stream.play()?;
    std::thread::sleep(std::time::Duration::from_secs(args.time));
    drop(stream);

    Ok(())
}

//fn write_data<T>(output: &mut [T], channels: usize, next_sample: &mut dyn FnMut() -> f32)
fn write_data<T, F>(output: &mut [T], channels: usize, next_sample: &mut F)
where
    T: cpal::Sample,
    F: FnMut() -> f32 + Send,
{
    for frame in output.chunks_mut(channels) {
        let value: T = cpal::Sample::from::<f32>(&next_sample());
        for sample in frame.iter_mut() {
            *sample = value;
        }
    }
}
