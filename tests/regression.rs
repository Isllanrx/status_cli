mod common;

use common::*;

fn claude_with(five_hour: &str) -> String {
    format!(
        r#"{{"session_id":"regression","model":{{"display_name":"Opus 5.5"}},"rate_limits":{{"five_hour":{five_hour}}},
            "cost":{{"total_duration_ms":60000}}}}"#
    )
}

#[test]
fn bar_fill_matches_usage_without_a_pace_marker_at_the_end() {
    let temp = TempDir::new();
    let reset = now_secs() + 360;
    let out = run(&temp, &claude_with(&format!(r#"{{"used_percentage":79,"resets_at":{reset}}}"#)), Some(160));
    assert!(out.text.contains("sessão ━━━━━━━━── 79%"), "{:?}", out.text);
    assert!(!out.text.contains('│'), "{:?}", out.text);
}

#[test]
fn fresh_window_does_not_show_an_exhaustion_next_to_the_reset() {
    let temp = TempDir::new();
    let reset = now_secs() + 4 * 3600 + 48 * 60;
    let out = run(&temp, &claude_with(&format!(r#"{{"used_percentage":4,"resets_at":{reset}}}"#)), Some(160));
    assert!(!out.text.contains('⇥'), "{:?}", out.text);
}

#[test]
fn clock_shows_seconds_so_it_visibly_ticks() {
    let temp = TempDir::new();
    let out = run(&temp, &claude_with("null"), Some(160));
    assert!(out.text.trim_end().ends_with("00:01:00"), "{:?}", out.text);
}

#[test]
fn sub_percent_changes_reach_the_line() {
    let temp = TempDir::new();
    run(&temp, &claude_with(r#"{"used_percentage":79.1}"#), Some(160));
    let out = run(&temp, &claude_with(r#"{"used_percentage":79.4}"#), Some(160));
    assert!(out.text.contains(" 79%"), "{:?}", out.text);
}

#[test]
fn agy_uses_the_current_model_family_not_the_most_consumed_bucket() {
    let temp = TempDir::new();
    let payload = r#"{"product":"antigravity","model":{"display_name":"Gemini 3.8 Flash (High)"},"terminal_width":160,
        "quota":{"3p-weekly":{"remaining_fraction":0.0004,"reset_in_seconds":204931},
                 "gemini-5h":{"remaining_fraction":0.9989992,"reset_in_seconds":17279},
                 "gemini-weekly":{"remaining_fraction":0.0058754333,"reset_in_seconds":194772}}}"#;
    let out = run(&temp, payload, None);
    assert!(out.text.contains("sessão ────────── 0%"), "{:?}", out.text);
    assert!(out.text.contains("semana ━━━━━━━━━━ 99%"), "{:?}", out.text);
}

#[test]
fn powershell_bom_and_huge_widths_are_harmless() {
    let temp = TempDir::new();
    let out = run(&temp, "\u{feff}{\"model\":{\"display_name\":\"Opus\"},\"terminal_width\":4000000000}", None);
    assert!(out.text.starts_with("Opus ╱ "), "{:?}", out.text);
}

#[test]
fn escape_sequences_from_the_host_cannot_reach_the_terminal() {
    let temp = TempDir::new();
    let out = run(&temp, r#"{"model":{"display_name":"Opus\u001b]0;pwned\u0007"}}"#, None);
    assert!(!out.raw.contains("\x1b]0;pwned"), "{:?}", out.raw);
}
