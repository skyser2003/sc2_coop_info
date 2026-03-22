use std::path::PathBuf;
use std::time::Instant;

use s2protocol_port::{build_protocol_store, parse_file_with_store_simple, ParsedReplay};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut start_index: usize = 0;
    let mut max_files: Option<usize> = None;

    let mut i = 1usize;
    while i < args.len() {
        match args[i].as_str() {
            "--start" => {
                if i + 1 < args.len() {
                    start_index = args[i + 1].parse().unwrap_or(0);
                    i += 2;
                } else {
                    break;
                }
            }
            "--max" => {
                if i + 1 < args.len() {
                    max_files = args[i + 1].parse().ok();
                    i += 2;
                } else {
                    break;
                }
            }
            _ => {
                i += 1;
            }
        }
    }

    let root = match std::env::var("S2_REPLAY_DIR") {
        Ok(v) if !v.is_empty() => v,
        _ => "".to_string(),
    };

    let root = PathBuf::from(root);
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(&root)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if path
            .extension()
            .and_then(|ext: &std::ffi::OsStr| ext.to_str())
            .map(|ext: &str| ext.eq_ignore_ascii_case("sc2replay"))
            != Some(true)
        {
            continue;
        }
        files.push(path.to_path_buf());
    }

    files.sort();

    let store = match build_protocol_store() {
        Ok(store) => store,
        Err(err) => {
            eprintln!("failed to load protocol store: {err}");
            return;
        }
    };

    let mut failures = Vec::new();
    let mut processed = 0usize;
    for (idx, path) in files.iter().enumerate() {
        if idx < start_index {
            continue;
        }
        if max_files.is_some_and(|max| processed >= max) {
            break;
        }
        processed += 1;
        let name = path
            .file_name()
            .and_then(|name: &std::ffi::OsStr| name.to_str())
            .unwrap_or("<unknown>");
        let start = Instant::now();
        let result = parse_file_with_store_simple(path, &store);
        let elapsed = start.elapsed();

        match result {
            Ok(ParsedReplay { base_build, .. }) => {
                println!(
                    "{:>5}/{:>5} ok {:>8?} {} ({})",
                    idx + 1,
                    files.len(),
                    elapsed,
                    name,
                    base_build
                );
            }
            Err(err) => {
                println!(
                    "{:>5}/{:>5} bad {:>8?} {} :: {err}",
                    idx + 1,
                    files.len(),
                    elapsed,
                    name
                );
                failures.push((path.clone(), err.to_string()));
            }
        }
    }

    println!("done: {} failures", failures.len());
    for (path, reason) in failures {
        let name = path
            .file_name()
            .and_then(|name: &std::ffi::OsStr| name.to_str())
            .unwrap_or("<unknown>");
        println!("  {} :: {}", name, reason);
    }
}
