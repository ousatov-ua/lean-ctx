use lean_ctx::core::compressor::aggressive_compress;
use lean_ctx::core::tokens::count_tokens;
use lean_ctx::shell::compress::compress_if_beneficial_pub;

#[test]
fn markdown_aggressive_keeps_headings_and_saves_tokens() {
    let mut doc =
        String::from("# Context Engine\n\nIntro text with stable overview.\n\n## Setup\n\n");
    for i in 0..60 {
        doc.push_str(&format!(
            "Repeated explanatory prose line {i} about background concepts and examples.\n"
        ));
    }
    doc.push_str("- `ctx_read` keeps exact source references for agent tasks.\n");
    doc.push_str("## Safety\n\nMUST preserve diagnostics, commands, and recovery notes.\n");
    for i in 0..60 {
        doc.push_str(&format!(
            "Additional explanatory prose line {i} about operational background.\n"
        ));
    }

    let compressed = aggressive_compress(&doc, Some("md"));
    assert!(count_tokens(&compressed) < count_tokens(&doc));
    assert!(compressed.contains("# Context Engine"));
    assert!(compressed.contains("## Setup"));
    assert!(compressed.contains("## Safety"));
    assert!(compressed.contains("ctx_read"));
    assert!(compressed.contains("MUST preserve"));
}

#[test]
fn cargo_warning_output_folds_progress_and_preserves_diagnostics() {
    let mut output = String::new();
    for i in 0..80 {
        output.push_str(&format!(
            "   Compiling crate_{i:03} v0.1.0 (/tmp/ws/crate_{i:03})\n"
        ));
    }
    output.push_str("warning: unused variable: `tmp`\n");
    output.push_str("  --> crates/demo/src/lib.rs:42:9\n");
    output.push_str("   |\n42 |     let tmp = 1;\n   |         ^^^\n");
    output.push_str("warning: `demo` generated 1 warning\n");
    output.push_str("    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.23s\n");

    let compressed = compress_if_beneficial_pub("cargo build", &output);
    assert!(count_tokens(&compressed) < count_tokens(&output));
    assert!(compressed.contains("cargo compile/check lines folded"));
    assert!(compressed.contains("warning: unused variable"));
    assert!(compressed.contains("crates/demo/src/lib.rs:42:9"));
    assert!(compressed.contains("generated 1 warning"));
}

#[test]
fn pytest_success_output_folds_passed_lines_and_preserves_summary() {
    let mut output = String::from(
        "============================= test session starts =============================\n",
    );
    for i in 0..80 {
        output.push_str(&format!(
            "tests/test_mod_{:02}.py::test_case_{i:03} PASSED [ {:02}%]\n",
            i / 10,
            i % 100
        ));
    }
    output.push_str(
        "======================= 80 passed, 0 warnings in 2.34s =======================\n",
    );

    let compressed = compress_if_beneficial_pub("pytest", &output);
    assert!(count_tokens(&compressed) < count_tokens(&output));
    assert!(compressed.contains("pytest PASSED lines folded"));
    assert!(compressed.contains("80 passed"));
}
