use std::time::Instant;

use dibs_query_syntax::{DibsExternalScanner, embedded_grammar_json};
use snark::{
    artifact::grammar_fingerprint,
    grammar::RawGrammarJson,
    lexical::LexicalFacts,
    lower::weavy::{WeavyParsePlan, parse_prepared_weavy_recovering_with_report_and_scanner},
    module::SnarkModule,
    parser::{ParseTable, ParserGrammar},
    validated::ValidatedGrammar,
};

fn main() {
    let grammar = embedded_grammar_json();
    let live_start = Instant::now();
    let raw = RawGrammarJson::from_tree_sitter_json_str(grammar).expect("raw grammar");
    let validated = ValidatedGrammar::from_raw(&raw).expect("validated grammar");
    let lexical = LexicalFacts::from_grammar(&validated);
    let parser = ParserGrammar::normalize_from_validated(&validated, &lexical)
        .expect("normalize")
        .prepare_productions_for_items()
        .expect("prepare");
    let table = ParseTable::from_grammar(&parser).expect("table");
    let plan = WeavyParsePlan::new(&validated, &parser, &table).expect("plan");
    let live_elapsed = live_start.elapsed();

    let sample = include_str!("../tests/fixtures/lexical-negative.dibs");
    let scanner = DibsExternalScanner;
    let live_tree = parse_prepared_weavy_recovering_with_report_and_scanner(
        &plan,
        &parser,
        &table,
        sample,
        Some(&scanner),
    )
    .expect("live parse")
    .tree()
    .clone();

    let generation_start = Instant::now();
    let module = SnarkModule::from_prepared(
        grammar_fingerprint(grammar),
        parser.clone(),
        table.clone(),
        plan.clone(),
    );
    let bytes = module.save().expect("save");
    let generation_elapsed = generation_start.elapsed();

    let load_start = Instant::now();
    let loaded = SnarkModule::load(&bytes).expect("load");
    assert_eq!(loaded.grammar_fingerprint(), grammar_fingerprint(grammar));
    let load_elapsed = load_start.elapsed();
    let [fingerprint_bytes, grammar_bytes, table_bytes, plan_bytes] =
        loaded.constant_sizes().expect("constant sizes");

    let first_start = Instant::now();
    let loaded_tree = loaded
        .parse_recovering(sample, Some(&scanner))
        .expect("loaded parse")
        .tree()
        .clone();
    let first_elapsed = first_start.elapsed();
    let warm_start = Instant::now();
    let warm_tree = loaded
        .parse_recovering(sample, Some(&scanner))
        .expect("warm parse")
        .tree()
        .clone();
    let warm_elapsed = warm_start.elapsed();
    assert_eq!(loaded_tree, live_tree);
    assert_eq!(warm_tree, live_tree);

    let inspection = SnarkModule::inspect(&bytes).expect("inspect");
    println!("states={}", loaded.parse_table().states().len());
    println!("conflicts={}", loaded.parse_table().conflicts().len());
    println!(
        "live_construction_ms={:.3}",
        live_elapsed.as_secs_f64() * 1000.0
    );
    println!("constant.grammar_fingerprint.bytes={fingerprint_bytes}");
    println!("constant.parser_grammar.bytes={grammar_bytes}");
    println!("constant.parse_table.bytes={table_bytes}");
    println!("constant.parse_plan.bytes={plan_bytes}");
    println!(
        "module_generation_ms={:.3}",
        generation_elapsed.as_secs_f64() * 1000.0
    );
    println!("module_file_bytes={}", bytes.len());
    println!("module_load_ms={:.3}", load_elapsed.as_secs_f64() * 1000.0);
    println!("first_parse_ms={:.3}", first_elapsed.as_secs_f64() * 1000.0);
    println!("warm_parse_ms={:.3}", warm_elapsed.as_secs_f64() * 1000.0);
    println!("unique_regexes={}", loaded.unique_regex_count());
    println!("regex_compilations={}", loaded.regex_compile_count());
    for section in inspection.sections {
        println!("section.{}.bytes={}", section.name, section.encoded_len);
    }
}
