use std::{env, fs, process::ExitCode, time::Instant};

use dibs_query_syntax::{DibsParser, SourceId};

fn main() -> ExitCode {
    let Some(path) = env::args_os().nth(1) else {
        eprintln!(
            "usage: cargo run -p dibs-query-syntax --example parse_repeated -- <source.dibs>"
        );
        return ExitCode::FAILURE;
    };
    let source = match fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("failed to read {path:?}: {error}");
            return ExitCode::FAILURE;
        }
    };

    let started = Instant::now();
    let parser = DibsParser::new();
    eprintln!(
        "construct_ms={:.3}",
        started.elapsed().as_secs_f64() * 1000.0
    );

    for iteration in 1..=3 {
        let started = Instant::now();
        if let Err(diagnostics) = parser.parse_strict(SourceId::new(iteration), &source) {
            eprintln!("iteration={iteration} diagnostics={diagnostics:#?}");
            return ExitCode::FAILURE;
        }
        eprintln!(
            "parse_{iteration}_ms={:.3}",
            started.elapsed().as_secs_f64() * 1000.0
        );
    }

    ExitCode::SUCCESS
}
