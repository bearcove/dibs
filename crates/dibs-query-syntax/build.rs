use std::{env, path::PathBuf, process::ExitCode};

use snark_dsl::typed_ast::{TypedAstConfig, generate_typed_ast};

fn main() -> ExitCode {
    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"));
    let grammar_js = manifest_dir.join("grammar.js");
    let annotations_js = manifest_dir.join("dibs_query_ast.snark.js");
    let config = TypedAstConfig {
        grammar_js: &grammar_js,
        annotations_js: &annotations_js,
        out_dir: &out_dir,
        grammar_output: "dibs_query_grammar.json",
        ast_output: "dibs_query_ast.rs",
        annotation_source_name: "dibs_query_ast.snark.js",
        generated_by: "crates/dibs-query-syntax/build.rs",
        language_name: "Dibs query",
    };

    if let Err(error) = generate_typed_ast(&config) {
        eprintln!("dibs-query-syntax AST generation failed: {error}");
        let mut source = std::error::Error::source(&error);
        while let Some(cause) = source {
            eprintln!("caused by: {cause}");
            source = cause.source();
        }
        return ExitCode::FAILURE;
    }

    let grammar_json = std::fs::read_to_string(out_dir.join("dibs_query_grammar.json"))
        .unwrap_or_else(|error| panic!("read generated Dibs grammar: {error}"));
    let bytes = match snark::module::SnarkModule::compile_grammar_json(&grammar_json)
        .and_then(|module| module.save())
    {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("dibs-query-syntax parser module generation failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(error) = std::fs::write(out_dir.join("dibs_query_parser.weavy"), bytes) {
        eprintln!("dibs-query-syntax parser module write failed: {error}");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
