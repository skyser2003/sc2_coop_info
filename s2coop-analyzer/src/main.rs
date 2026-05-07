use indicatif::{ProgressBar, ProgressStyle};
use s2coop_analyzer::cli::AnalyzerCli;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
struct CacheProgressUpdate {
    processed: u64,
    total: u64,
    eta: Option<String>,
}

struct CliProgressBar;

impl CliProgressBar {
    fn build() -> ProgressBar {
        let progress_bar = ProgressBar::new(0);
        let style = ProgressStyle::with_template(
        "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} {percent:>3}% {msg}",
    )
    .expect("progress bar template should be valid")
    .progress_chars("=>-");
        progress_bar.set_style(style);
        progress_bar
    }

    fn parse_update(message: &str) -> Option<CacheProgressUpdate> {
        let mut eta = None;
        let mut running_line = message.trim();
        let mut lines = message.lines();
        let first_line = lines.next()?.trim();

        if let Some(value) = first_line.strip_prefix("Estimated remaining time: ") {
            eta = Some(value.to_string());
            running_line = lines.next()?.trim();
        }

        let counts = running_line
            .strip_prefix("Running... ")?
            .split_whitespace()
            .next()?;
        let (processed, total) = counts.split_once('/')?;

        Some(CacheProgressUpdate {
            processed: processed.parse().ok()?,
            total: total.parse().ok()?,
            eta,
        })
    }

    fn update(progress_bar: &ProgressBar, message: &str) {
        if message == "Starting detailed analysis!" {
            progress_bar.set_message("Starting detailed analysis");
            progress_bar.enable_steady_tick(Duration::from_millis(120));
            return;
        }

        if let Some(progress) = Self::parse_update(message) {
            progress_bar.set_length(progress.total);
            progress_bar.set_position(progress.processed);
            progress_bar.set_message(match progress.eta {
                Some(eta) => format!("ETA {eta}"),
                None => "Analyzing replays".to_string(),
            });
            return;
        }

        if message.starts_with("Detailed analysis completed! ") {
            progress_bar.set_position(progress_bar.length().unwrap_or_default());
            progress_bar.set_message("Writing cache");
            return;
        }

        if message.starts_with("Detailed analysis completed in ") {
            progress_bar.finish_with_message(message.to_string());
            return;
        }

        progress_bar.println(message);
    }
}

fn main() {
    let args = std::env::args().collect::<Vec<String>>();
    let progress_bar = CliProgressBar::build();
    let logger_progress_bar = progress_bar.clone();
    let logger = move |message: String| CliProgressBar::update(&logger_progress_bar, &message);
    match AnalyzerCli::run_with_logger(&args, &logger) {
        Ok(output) => {
            if !progress_bar.is_finished() {
                progress_bar.finish_and_clear();
            }
            println!("{output}");
        }
        Err(error) => {
            if !progress_bar.is_finished() {
                progress_bar.finish_and_clear();
            }
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
