use crate::oscillator::{AudioParams, Waveform};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{Axis, Block, Borders, Chart, Dataset, Gauge, List, ListItem, Paragraph, Wrap},
    Frame, Terminal,
};
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// TUI application state
pub struct App {
    params: Arc<Mutex<AudioParams>>,
    sample_buffer: Arc<Mutex<Vec<f32>>>,
    selected_param: usize,
    should_quit: Arc<AtomicBool>,
}

impl App {
    pub fn new(
        params: Arc<Mutex<AudioParams>>,
        sample_buffer: Arc<Mutex<Vec<f32>>>,
        should_quit: Arc<AtomicBool>,
    ) -> Self {
        Self {
            params,
            sample_buffer,
            selected_param: 0,
            should_quit,
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.should_quit.store(true, Ordering::Relaxed);
            }
            KeyCode::Up => {
                if self.selected_param > 0 {
                    self.selected_param -= 1;
                }
            }
            KeyCode::Down => {
                if self.selected_param < 2 {
                    self.selected_param += 1;
                }
            }
            KeyCode::Left => self.decrease_param(),
            KeyCode::Right => self.increase_param(),
            KeyCode::Char('1') => self.set_waveform(Waveform::Sine),
            KeyCode::Char('2') => self.set_waveform(Waveform::Sawtooth),
            KeyCode::Char('3') => self.set_waveform(Waveform::Triangle),
            KeyCode::Char('4') => self.set_waveform(Waveform::Square),
            KeyCode::Char('5') => self.set_waveform(Waveform::Noise),
            _ => {}
        }
    }

    fn set_waveform(&mut self, waveform: Waveform) {
        let mut params = self.params.lock().unwrap();
        params.waveform = waveform;
    }

    fn increase_param(&mut self) {
        let mut params = self.params.lock().unwrap();
        match self.selected_param {
            0 => {
                // Waveform - cycle through
                params.waveform = match params.waveform {
                    Waveform::Sine => Waveform::Sawtooth,
                    Waveform::Sawtooth => Waveform::Triangle,
                    Waveform::Triangle => Waveform::Square,
                    Waveform::Square => Waveform::Noise,
                    Waveform::Noise => Waveform::Sine,
                };
            }
            1 => {
                // Frequency
                params.frequency = (params.frequency * 1.05).min(20000.0);
            }
            2 => {
                // Volume
                params.volume = (params.volume + 0.05).min(1.0);
            }
            _ => {}
        }
    }

    fn decrease_param(&mut self) {
        let mut params = self.params.lock().unwrap();
        match self.selected_param {
            0 => {
                // Waveform - cycle through backwards
                params.waveform = match params.waveform {
                    Waveform::Sine => Waveform::Noise,
                    Waveform::Sawtooth => Waveform::Sine,
                    Waveform::Triangle => Waveform::Sawtooth,
                    Waveform::Square => Waveform::Triangle,
                    Waveform::Noise => Waveform::Square,
                };
            }
            1 => {
                // Frequency
                params.frequency = (params.frequency / 1.05).max(20.0);
            }
            2 => {
                // Volume
                params.volume = (params.volume - 0.05).max(0.0);
            }
            _ => {}
        }
    }

    fn draw(&self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(10),
                Constraint::Length(10),
            ])
            .split(frame.area());

        // Title
        let title = Paragraph::new("Waveform Generator - Press 'q' to quit")
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(title, chunks[0]);

        // Main area split horizontally
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
            .split(chunks[1]);

        // Controls
        self.draw_controls(frame, main_chunks[0]);

        // Waveform visualization
        self.draw_waveform(frame, main_chunks[1]);

        // Help
        self.draw_help(frame, chunks[2]);
    }

    fn draw_controls(&self, frame: &mut Frame, area: Rect) {
        let params = self.params.lock().unwrap();

        let items = vec![
            ListItem::new(format!("Waveform: {}", params.waveform)).style(
                if self.selected_param == 0 {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                },
            ),
            ListItem::new(format!("Frequency: {:.1} Hz", params.frequency)).style(
                if self.selected_param == 1 {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                },
            ),
            ListItem::new(format!("Volume: {:.0}%", params.volume * 100.0)).style(
                if self.selected_param == 2 {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                },
            ),
        ];

        let list = List::new(items)
            .block(Block::default().title("Controls").borders(Borders::ALL))
            .style(Style::default().fg(Color::White));

        frame.render_widget(list, area);

        // Volume gauge
        let gauge_area = Rect::new(area.x + 1, area.y + area.height - 3, area.width - 2, 1);
        let gauge = Gauge::default()
            .percent((params.volume * 100.0) as u16)
            .style(Style::default().fg(Color::Green));
        frame.render_widget(gauge, gauge_area);
    }

    fn draw_waveform(&self, frame: &mut Frame, area: Rect) {
        let samples = self.sample_buffer.lock().unwrap();
        if samples.len() < 2 {
            // Not enough samples to draw
            let no_data = Paragraph::new("Waiting for waveform data...")
                .block(Block::default().title("Waveform").borders(Borders::ALL))
                .alignment(Alignment::Center);
            frame.render_widget(no_data, area);
            return;
        }

        // Get frequency for cycle calculation
        let params = self.params.lock().unwrap();
        let frequency = params.frequency;
        let waveform = params.waveform;
        drop(params);

        // Create points for visualization
        // For the chart, we want x to go from 0 to the number of samples
        let mut points: Vec<(f64, f64)> = Vec::new();

        // Use all samples but space them appropriately
        for (i, &sample) in samples.iter().enumerate() {
            points.push((i as f64, sample as f64));
        }

        // If we have very few points, interpolate to make the waveform smoother
        if points.len() < 50 && waveform != Waveform::Noise {
            points = interpolate_waveform(&samples, 200);
        }

        let datasets = vec![Dataset::default()
            .name("Waveform")
            .marker(symbols::Marker::Braille)
            .style(Style::default().fg(Color::Cyan))
            .graph_type(ratatui::widgets::GraphType::Line)
            .data(&points)];

        // Calculate time span for x-axis
        let time_span = if frequency > 0.0 {
            format!(
                "{:.1}ms",
                (points.len() as f64 / frequency as f64) * 1000.0 / 3.0
            )
        } else {
            "".to_string()
        };

        let chart = Chart::new(datasets)
            .block(
                Block::default()
                    .title(format!(
                        "Waveform ({} @ {:.0}Hz) [{}]",
                        waveform, frequency, time_span
                    ))
                    .borders(Borders::ALL),
            )
            .x_axis(
                Axis::default()
                    .bounds([0.0, points.len() as f64])
                    .labels(vec![
                        Line::from("0"),
                        Line::from(format!("{}", points.len() / 2)),
                        Line::from(format!("{}", points.len())),
                    ]),
            )
            .y_axis(Axis::default().bounds([-1.2, 1.2]).labels(vec![
                Line::from("-1"),
                Line::from("0"),
                Line::from("1"),
            ]));

        frame.render_widget(chart, area);
    }

    fn draw_help(&self, frame: &mut Frame, area: Rect) {
        let help_text = vec![
            Line::from(vec![
                Span::raw("Navigate: "),
                Span::styled("↑↓", Style::default().fg(Color::Green)),
                Span::raw(" | Adjust: "),
                Span::styled("←→", Style::default().fg(Color::Green)),
                Span::raw(" | Waveforms: "),
                Span::styled("1-5", Style::default().fg(Color::Green)),
                Span::raw(" | Quit: "),
                Span::styled("q/ESC", Style::default().fg(Color::Red)),
            ]),
            Line::from(vec![Span::raw(
                "1: Sine, 2: Sawtooth, 3: Triangle, 4: Square, 5: Noise",
            )]),
        ];

        let help = Paragraph::new(help_text)
            .block(Block::default().title("Help").borders(Borders::ALL))
            .wrap(Wrap { trim: true });

        frame.render_widget(help, area);
    }
}

pub fn run_tui(
    params: Arc<Mutex<AudioParams>>,
    sample_buffer: Arc<Mutex<Vec<f32>>>,
    should_quit: Arc<AtomicBool>,
) -> Result<(), anyhow::Error> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app
    let mut app = App::new(params, sample_buffer, Arc::clone(&should_quit));

    // Main loop
    loop {
        terminal.draw(|f| app.draw(f))?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c') {
                    should_quit.store(true, Ordering::Relaxed);
                    break;
                }
                app.handle_key(key);
            }
        }

        if should_quit.load(Ordering::Relaxed) {
            break;
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

/// Interpolate waveform data for smooth visualization
fn interpolate_waveform(samples: &[f32], target_points: usize) -> Vec<(f64, f64)> {
    if samples.is_empty() {
        return vec![];
    }

    let mut result = Vec::with_capacity(target_points);
    let step = (samples.len() - 1) as f64 / (target_points - 1) as f64;

    for i in 0..target_points {
        let pos = i as f64 * step;
        let idx = pos.floor() as usize;
        let frac = pos - idx as f64;

        let value = if idx + 1 < samples.len() {
            samples[idx] as f64 * (1.0 - frac) + samples[idx + 1] as f64 * frac
        } else {
            samples[idx] as f64
        };

        result.push((i as f64, value));
    }

    result
}
