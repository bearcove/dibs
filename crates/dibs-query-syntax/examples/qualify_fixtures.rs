use std::{env, fs, process::ExitCode, time::Instant};

use dibs_query_syntax::{DibsParser, SourceId};

fn main() -> ExitCode {
    let paths = env::args_os().skip(1).collect::<Vec<_>>();
    if paths.is_empty() {
        eprintln!(
            "usage: cargo run -p dibs-query-syntax --example qualify_fixtures -- <source.dibs>..."
        );
        return ExitCode::FAILURE;
    }

    let started = Instant::now();
    let parser = DibsParser::new();
    eprintln!(
        "construct_ms={:.3}",
        started.elapsed().as_secs_f64() * 1000.0
    );

    for (index, path) in paths.iter().enumerate() {
        let source = match fs::read_to_string(path) {
            Ok(source) => source,
            Err(error) => {
                eprintln!("failed to read {path:?}: {error}");
                return ExitCode::FAILURE;
            }
        };
        let started = Instant::now();
        match parser.parse_strict(SourceId::new(index as u32), &source) {
            Ok(_) => eprintln!(
                "fixture={path:?} status=ok parse_ms={:.3}",
                started.elapsed().as_secs_f64() * 1000.0
            ),
            Err(diagnostics) => {
                let summary = diagnostics
                    .iter()
                    .map(|diagnostic| format!("{:?}@{}", diagnostic.code, diagnostic.primary.start))
                    .collect::<Vec<_>>()
                    .join(",");
                eprintln!(
                    "fixture={path:?} status=failed parse_ms={:.3} diagnostics={summary}",
                    started.elapsed().as_secs_f64() * 1000.0
                );
            }
        }
    }

    ExitCode::SUCCESS
}
