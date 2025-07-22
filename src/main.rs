mod audio;
mod oscillator;
mod tui;

use clap::Parser;
use oscillator::{AudioParams, Waveform};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

#[derive(Parser, Debug)]
#[clap(about, version, author)]
struct Args {
    /// Waveform type: sine, sawtooth, triangle, square, or noise
    #[clap(short, long, default_value_t = Waveform::Sine)]
    waveform: Waveform,

    /// Frequency in Hz
    #[clap(short, long, default_value_t = 440)]
    frequency: u32,

    /// Volume (0.0 to 1.0)
    #[clap(short, long, default_value_t = 0.5)]
    volume: f32,

    /// Use TUI mode (interactive interface)
    #[clap(short, long)]
    tui: bool,

    /// Duration in seconds (non-TUI mode only)
    #[clap(short, long, default_value_t = 1)]
    duration: u64,
}

fn main() -> Result<(), anyhow::Error> {
    let args = Args::parse();

    // Validate arguments
    if args.volume < 0.0 || args.volume > 1.0 {
        return Err(anyhow::anyhow!("Volume must be between 0.0 and 1.0"));
    }

    // Initialize shared state
    let params = Arc::new(Mutex::new(AudioParams::new(
        args.waveform,
        args.frequency as f32,
        args.volume,
    )));
    let sample_buffer = Arc::new(Mutex::new(Vec::with_capacity(500)));
    let should_quit = Arc::new(AtomicBool::new(false));

    if args.tui {
        // TUI mode
        println!("Starting TUI mode...");

        // Start audio engine in background
        let audio_engine = audio::AudioEngine::new(
            Arc::clone(&params),
            Arc::clone(&sample_buffer),
            Arc::clone(&should_quit),
        );
        let audio_thread = audio_engine.start()?;

        // Run TUI
        let result = tui::run_tui(params, sample_buffer, should_quit);

        // Wait for audio thread to finish
        audio_thread.join().unwrap()?;

        result
    } else {
        // Simple mode - just play for specified duration
        println!(
            "Playing {} wave at {}Hz for {}s at {:.0}% volume",
            args.waveform,
            args.frequency,
            args.duration,
            args.volume * 100.0
        );

        // Start audio engine
        let audio_engine = audio::AudioEngine::new(
            Arc::clone(&params),
            Arc::clone(&sample_buffer),
            Arc::clone(&should_quit),
        );
        let audio_thread = audio_engine.start()?;

        // Wait for specified duration
        std::thread::sleep(std::time::Duration::from_secs(args.duration));

        // Signal quit
        should_quit.store(true, std::sync::atomic::Ordering::Relaxed);

        // Wait for audio thread to finish
        audio_thread.join().unwrap()
    }
}
