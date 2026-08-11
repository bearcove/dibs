use std::{env, fs, process::ExitCode};

use dibs_query_syntax::{DibsParser, SourceId};

fn main() -> ExitCode {
    let Some(path) = env::args_os().nth(1) else {
        eprintln!("usage: cargo run -p dibs-query-syntax --example parse -- <source.dibs>");
        return ExitCode::FAILURE;
    };
    let source = match fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("failed to read {:?}: {error}", path);
            return ExitCode::FAILURE;
        }
    };
    let parser = DibsParser::new();
    eprintln!("parser facts: {:?}", parser.parser_facts());
    match parser.parse_strict(SourceId::new(0), &source) {
        Ok(file) => {
            println!("{file:#?}");
            ExitCode::SUCCESS
        }
        Err(diagnostics) => {
            for diagnostic in diagnostics {
                eprintln!("{diagnostic:#?}");
            }
            ExitCode::FAILURE
        }
    }
}
