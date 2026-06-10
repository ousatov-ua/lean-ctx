//! Live conformance run against a real lean-ctx server (GL #395).
//!
//! Driven by `scripts/sdk-conformance.sh` (CI job `sdk-conformance`): the
//! script builds the engine, starts `lean-ctx serve` and exports
//! `LEANCTX_CONFORMANCE_URL`. Without that variable the test is a no-op, so
//! plain `cargo test` runs stay hermetic.

use lean_ctx_client::{run_conformance, LeanCtxClient};

#[test]
fn live_conformance_all_checks_pass() {
    let Ok(url) = std::env::var("LEANCTX_CONFORMANCE_URL") else {
        eprintln!("skipping: LEANCTX_CONFORMANCE_URL not set");
        return;
    };

    let mut builder = LeanCtxClient::builder(url.trim());
    if let Ok(token) = std::env::var("LEANCTX_CONFORMANCE_TOKEN") {
        if !token.trim().is_empty() {
            builder = builder.bearer_token(token.trim());
        }
    }
    let client = builder.build().expect("client builds");
    let card = run_conformance(&client);

    if let Ok(matrix_dir) = std::env::var("LEANCTX_MATRIX_DIR") {
        if !matrix_dir.trim().is_empty() {
            let checks: Vec<serde_json::Value> = card
                .checks
                .iter()
                .map(|c| {
                    serde_json::json!({
                        "name": c.name,
                        "passed": c.passed,
                        "detail": c.detail,
                    })
                })
                .collect();
            let doc = serde_json::json!({
                "sdk": "rust",
                "passed": card.passed(),
                "total": card.total(),
                "all_passed": card.all_passed(),
                "checks": checks,
            });
            let out = std::path::Path::new(matrix_dir.trim()).join("conformance-rust.json");
            std::fs::write(out, serde_json::to_string_pretty(&doc).expect("serialize"))
                .expect("write matrix artifact");
        }
    }

    let failed: Vec<String> = card
        .checks
        .iter()
        .filter(|c| !c.passed)
        .map(|c| format!("{}: {}", c.name, c.detail))
        .collect();
    assert!(card.all_passed(), "failed checks: {failed:?}");
}
