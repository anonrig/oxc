//! Regression test for the `parse_jest_fn_call` fast-path optimization.
//!
//! `prefer-importing-jest-globals` runs via `run_once` (it is NOT gated on the
//! file being a test file) and, by default, also flags Jest globals whose name
//! maps to `JestFnKind::Unknown` — most notably `pending`. On a file that does
//! not import from `@jest/globals`/`vitest` and is not named like a test file,
//! `ContextHost::sniff_for_frameworks` (the real, non-`cfg(test)` version that
//! this *integration* test exercises) leaves `is_jest()`/`is_vitest()` as
//! `false`. That is exactly the `(false, false)` framework branch in
//! `parse_jest_fn_call`.
//!
//! The fast-path early-return must NOT swallow `pending()` on this branch: doing
//! so would silently drop the `prefer-importing-jest-globals` diagnostic. This
//! test drives the shipped `Linter::run` end to end and asserts that the
//! diagnostic is still produced, while a genuinely non-Jest call (`foo()`) is
//! still ignored.

use std::{path::Path, sync::Arc};

use rustc_hash::FxHashMap;

use oxc_allocator::Allocator;
use oxc_linter::{
    ConfigStore, ConfigStoreBuilder, ContextSubHost, ContextSubHostOptions, ExternalPluginStore,
    LintOptions, Linter, ModuleRecord,
};
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;

/// Lint `source` (treated as living at `path`) with every built-in rule enabled,
/// returning the rendered diagnostic messages.
fn lint_all_rules(source: &str, path: &str) -> Vec<String> {
    let allocator = Allocator::default();
    let source_type = SourceType::from_path(path).unwrap();
    let parser_ret = Parser::new(&allocator, source, source_type).parse();
    let semantic = SemanticBuilder::new_linter().build(&parser_ret.program).semantic;
    let path = Path::new(path);
    let module_record = Arc::new(ModuleRecord::new(path, &parser_ret.module_record, &semantic));

    let mut external_plugin_store = ExternalPluginStore::default();
    let lint_config = ConfigStoreBuilder::all().build(&mut external_plugin_store).unwrap();
    let linter = Linter::new(
        LintOptions::default(),
        ConfigStore::new(lint_config, FxHashMap::default(), external_plugin_store),
        None,
    );

    linter
        .run(
            path,
            vec![ContextSubHost::new(
                semantic,
                Arc::clone(&module_record),
                0,
                ContextSubHostOptions::default(),
            )],
            &allocator,
        )
        .into_iter()
        .map(|message| format!("{}", message.error))
        .collect()
}

#[test]
fn prefer_importing_jest_globals_still_fires_for_pending_in_non_test_file() {
    // No `@jest/globals` import and a non-test path => `is_jest()` is `false`,
    // i.e. the `(false, false)` branch the fast-path must preserve.
    let diagnostics = lint_all_rules("pending();\n", "src/app.ts");

    assert!(
        diagnostics
            .iter()
            .any(|message| message.contains("@jest/globals") && message.contains("pending")),
        "expected `prefer-importing-jest-globals` to report `pending` on a non-test file, \
         got diagnostics: {diagnostics:#?}",
    );
}

#[test]
fn non_jest_call_is_not_reported_as_a_jest_global() {
    // A genuinely non-Jest call must never be flagged as a Jest global to import;
    // this is the case the fast-path is allowed to short-circuit.
    let diagnostics = lint_all_rules("foo();\n", "src/app.ts");

    assert!(
        !diagnostics.iter().any(|message| message.contains("@jest/globals")),
        "did not expect any `@jest/globals` import suggestion for a non-Jest call, \
         got diagnostics: {diagnostics:#?}",
    );
}
