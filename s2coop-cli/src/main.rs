use s2coop_cli::app::CliApplication;
use s2coop_cli::progress::CliProgressBar;

fn main() {
    let args = std::env::args().collect::<Vec<String>>();
    let progress_bar = CliProgressBar::build();
    let logger_progress_bar = progress_bar.clone();
    let logger = move |message: String| CliProgressBar::update(&logger_progress_bar, &message);

    match CliApplication::run_with_logger(&args, &logger) {
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
