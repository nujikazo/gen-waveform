use rand::{rngs::StdRng, Rng, SeedableRng};
use std::f32::consts::PI;
use std::fmt;
use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Waveform {
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

impl fmt::Display for Waveform {
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

/// Shared audio parameters that can be modified in real-time
#[derive(Clone)]
pub struct AudioParams {
    pub waveform: Waveform,
    pub frequency: f32,
    pub volume: f32,
}

impl AudioParams {
    pub fn new(waveform: Waveform, frequency: f32, volume: f32) -> Self {
        Self {
            waveform,
            frequency,
            volume: volume.clamp(0.0, 1.0),
        }
    }
}

/// Oscillator generates waveforms with proper phase tracking
pub struct Oscillator {
    params: Arc<Mutex<AudioParams>>,
    sample_rate: f32,
    phase: f32,
    rng: StdRng,
    sample_buffer: Arc<Mutex<Vec<f32>>>,
    sample_counter: AtomicUsize,
    // For interpolation
    previous_sample: f32,
    interpolation_enabled: bool,
    // For band-limiting
    band_limited: bool,
    // For phase-synchronized sampling
    last_phase: f32,
    collecting_cycle: bool,
    cycle_buffer: Vec<f32>,
}

impl Oscillator {
    pub fn new(
        params: Arc<Mutex<AudioParams>>,
        sample_rate: f32,
        sample_buffer: Arc<Mutex<Vec<f32>>>,
    ) -> Self {
        Self {
            params,
            sample_rate,
            phase: 0.0,
            rng: StdRng::from_entropy(),
            sample_buffer,
            sample_counter: AtomicUsize::new(0),
            previous_sample: 0.0,
            interpolation_enabled: false,
            band_limited: true,
            last_phase: 0.0,
            collecting_cycle: false,
            cycle_buffer: Vec::with_capacity(10000),
        }
    }

    /// Generate the next sample and advance the phase
    pub fn next_sample(&mut self) -> f32 {
        let params = self.params.lock().unwrap();
        let waveform = params.waveform;
        let frequency = params.frequency;
        let volume = params.volume;
        drop(params);

        let raw_sample = match waveform {
            Waveform::Sine => self.sine(),
            Waveform::Sawtooth => {
                if self.band_limited {
                    self.sawtooth_band_limited()
                } else {
                    self.sawtooth_naive()
                }
            }
            Waveform::Triangle => {
                if self.band_limited {
                    self.triangle_band_limited()
                } else {
                    self.triangle_naive()
                }
            }
            Waveform::Square => {
                if self.band_limited {
                    self.square_band_limited()
                } else {
                    self.square_naive()
                }
            }
            Waveform::Noise => self.white_noise(),
        };

        // Apply interpolation if enabled (except for noise)
        let sample = if self.interpolation_enabled && waveform != Waveform::Noise {
            let interpolation_factor = 0.1;
            self.previous_sample + (raw_sample - self.previous_sample) * interpolation_factor
        } else {
            raw_sample
        };

        self.previous_sample = sample;

        let output = sample * volume;

        // Phase-synchronized sample collection for visualization
        self.collect_visualization_samples(output, frequency);

        // Advance phase
        let phase_increment = frequency / self.sample_rate;
        self.last_phase = self.phase;
        self.phase += phase_increment;

        // Wrap phase to prevent overflow
        while self.phase >= 1.0 {
            self.phase -= 1.0;
        }

        output
    }

    /// Collect samples for visualization, starting from phase 0
    fn collect_visualization_samples(&mut self, sample: f32, frequency: f32) {
        // Detect phase wrap (zero crossing from positive to negative)
        let phase_wrapped = self.last_phase > 0.5 && self.phase < 0.5;

        if phase_wrapped && !self.collecting_cycle {
            // Start collecting a new cycle
            self.collecting_cycle = true;
            self.cycle_buffer.clear();
        }

        if self.collecting_cycle {
            self.cycle_buffer.push(sample);

            // Calculate how many samples we need for 3 complete cycles
            let samples_per_cycle = (self.sample_rate / frequency) as usize;
            let target_samples = samples_per_cycle * 3;

            // When we have collected enough samples, update the visualization buffer
            if self.cycle_buffer.len() >= target_samples {
                self.collecting_cycle = false;

                // Downsample to approximately 300 points for visualization
                let downsample_rate = (target_samples / 300).max(1);
                let mut visualization_samples = Vec::with_capacity(300);

                for (i, &s) in self.cycle_buffer.iter().enumerate() {
                    if i % downsample_rate == 0 {
                        visualization_samples.push(s);
                    }
                }

                // Update the shared buffer
                if let Ok(mut buffer) = self.sample_buffer.try_lock() {
                    *buffer = visualization_samples;
                }
            }
        }

        // For noise, update more frequently since phase doesn't matter
        if matches!(self.params.lock().unwrap().waveform, Waveform::Noise) {
            let counter = self.sample_counter.fetch_add(1, Ordering::Relaxed);
            if counter % 100 == 0 {
                if let Ok(mut buffer) = self.sample_buffer.try_lock() {
                    buffer.push(sample);
                    if buffer.len() > 300 {
                        buffer.drain(0..100);
                    }
                }
            }
        }
    }

    fn sine(&self) -> f32 {
        (2.0 * PI * self.phase).sin()
    }

    // Naive implementations (fast but with aliasing)
    fn sawtooth_naive(&self) -> f32 {
        2.0 * self.phase - 1.0
    }

    fn triangle_naive(&self) -> f32 {
        let p = self.phase;
        if p < 0.5 {
            4.0 * p - 1.0
        } else {
            3.0 - 4.0 * p
        }
    }

    fn square_naive(&self) -> f32 {
        if self.phase < 0.5 {
            1.0
        } else {
            -1.0
        }
    }

    // Band-limited implementations (slower but cleaner)
    fn sawtooth_band_limited(&self) -> f32 {
        // Band-limited sawtooth using additive synthesis to reduce aliasing
        let nyquist = self.sample_rate / 2.0;
        let mut sample = 0.0;
        let mut harmonic = 1;

        // Add harmonics up to Nyquist frequency
        let params = self.params.lock().unwrap();
        let frequency = params.frequency;
        drop(params);

        while frequency * harmonic as f32 <= nyquist && harmonic <= 64 {
            sample += (2.0 * PI * self.phase * harmonic as f32).sin() / harmonic as f32;
            harmonic += 1;
        }

        sample * 2.0 / PI
    }

    fn triangle_band_limited(&self) -> f32 {
        // Band-limited triangle wave
        let nyquist = self.sample_rate / 2.0;
        let mut sample = 0.0;
        let mut harmonic = 1;

        let params = self.params.lock().unwrap();
        let frequency = params.frequency;
        drop(params);

        // Triangle wave only has odd harmonics
        while frequency * (2 * harmonic - 1) as f32 <= nyquist && harmonic <= 32 {
            let n = 2 * harmonic - 1;
            let sign = if harmonic % 2 == 1 { 1.0 } else { -1.0 };
            sample += sign * (2.0 * PI * self.phase * n as f32).sin() / (n * n) as f32;
            harmonic += 1;
        }

        sample * 8.0 / (PI * PI)
    }

    fn square_band_limited(&self) -> f32 {
        // Band-limited square wave using additive synthesis
        let nyquist = self.sample_rate / 2.0;
        let mut sample = 0.0;
        let mut harmonic = 1;

        let params = self.params.lock().unwrap();
        let frequency = params.frequency;
        drop(params);

        // Square wave only has odd harmonics
        while frequency * (2 * harmonic - 1) as f32 <= nyquist && harmonic <= 32 {
            let n = 2 * harmonic - 1;
            sample += (2.0 * PI * self.phase * n as f32).sin() / n as f32;
            harmonic += 1;
        }

        sample * 4.0 / PI
    }

    fn white_noise(&mut self) -> f32 {
        self.rng.gen_range(-1.0..=1.0)
    }

    /// Get current phase (useful for debugging or visualization)
    #[allow(dead_code)]
    pub fn phase(&self) -> f32 {
        self.phase
    }

    /// Reset phase to 0
    #[allow(dead_code)]
    pub fn reset_phase(&mut self) {
        self.phase = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_waveform_from_str() {
        assert_eq!(Waveform::from_str("sine").unwrap(), Waveform::Sine);
        assert_eq!(Waveform::from_str("SINE").unwrap(), Waveform::Sine);
        assert_eq!(Waveform::from_str("sin").unwrap(), Waveform::Sine);
        assert!(Waveform::from_str("invalid").is_err());
    }

    #[test]
    fn test_audio_params_volume_clamping() {
        let params = AudioParams::new(Waveform::Sine, 440.0, 1.5);
        assert_eq!(params.volume, 1.0);

        let params = AudioParams::new(Waveform::Sine, 440.0, -0.5);
        assert_eq!(params.volume, 0.0);
    }

    #[test]
    fn test_oscillator_phase_wrapping() {
        let params = Arc::new(Mutex::new(AudioParams::new(Waveform::Sine, 1000.0, 1.0)));
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let mut osc = Oscillator::new(params, 1000.0, buffer);

        // Generate many samples to ensure phase wraps correctly
        for _ in 0..2000 {
            osc.next_sample();
        }

        // Phase should still be between 0 and 1
        assert!(osc.phase >= 0.0 && osc.phase < 1.0);
    }
}
