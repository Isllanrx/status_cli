mod common;

use common::*;

#[test]
fn every_language_labels_the_line_and_keeps_the_width() {
    let temp = TempDir::new();
    let payload = r#"{"session_id":"lang","model":{"display_name":"Opus 5.5"},
        "rate_limits":{"five_hour":{"used_percentage":40}},"context_window":{"used_percentage":20,"context_window_size":200000},
        "cost":{"total_duration_ms":60000}}"#;
    for (lang, session, compact, time) in [
        ("en", "session", "compact", "time"),
        ("es", "sesión", "compactar", "tiempo"),
        ("pt_BR.UTF-8", "sessão", "compactar", "tempo"),
        ("fr", "session", "compacter", "temps"),
        ("zh_CN", "会话", "压缩", "时长"),
    ] {
        let out = run_with(&temp, payload, Some(120), &[("STATUS_CLI_LANG", lang)]);
        for label in [session, compact, time] {
            assert!(out.text.contains(label), "{lang}: {:?}", out.text);
        }
        let columns: usize =
            out.visible.chars().map(|c| if ('\u{4e00}'..='\u{9fff}').contains(&c) { 2 } else { 1 }).sum();
        assert_eq!(columns, 112, "{lang}: {:?}", out.visible);
    }
}
