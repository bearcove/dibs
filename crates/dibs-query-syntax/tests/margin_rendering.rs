use dibs_query_syntax::{
    Diagnostic, DiagnosticCode, Repair, SourceId, Span, to_margin_diagnostics,
};
use margin_term::{ColorLevel, GlyphMode, HyperlinkMode, TerminalCapabilities, render};

#[test]
fn plaintext_margin_rendering_contains_dibs_diagnostic_context() {
    let source = "query Café() -> one {\n  select §\n}\n";
    let start = source.find('§').unwrap();
    let diagnostic = Diagnostic {
        code: DiagnosticCode::UnexpectedToken,
        source_id: SourceId::new(42),
        primary: Span::new(start as u32, (start + '§'.len_utf8()) as u32),
        unexpected: Some("§".to_string()),
        expected: None,
        repair: Some(Repair::SkipUnexpected),
        cost: Some(1),
        message: "unexpected \"§\"".to_string(),
        hints: vec!["remove the unexpected token".to_string()],
    };
    let diagnostics = to_margin_diagnostics("queries/unicode.dibs", source, [&diagnostic]);

    let rendered = render(
        &diagnostics,
        TerminalCapabilities {
            width: 72,
            glyph_mode: GlyphMode::Ascii,
            color_level: ColorLevel::None,
            hyperlink_mode: HyperlinkMode::None,
            tab_width: 4,
        },
    )
    .unwrap();

    println!("{rendered}");
    assert!(
        rendered.contains("error[DIBS-SYNTAX-UNEXPECTED]: unexpected \"§\""),
        "{rendered}"
    );
    assert!(
        rendered.contains("--> queries/unicode.dibs:2:10"),
        "{rendered}"
    );
    assert!(rendered.contains("2 |   select §"), "{rendered}");
    assert!(rendered.contains("unexpected \"§\""), "{rendered}");
    assert!(
        rendered.contains("= help: remove the unexpected token"),
        "{rendered}"
    );
    assert!(!rendered.contains("\u{1b}["), "{rendered}");
}
