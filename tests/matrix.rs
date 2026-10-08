mod common;

use common::*;

#[test]
fn every_claude_model_effort_and_mode_fits_and_reads_right() {
    let temp = TempDir::new();
    let models = [
        ("Opus 5.5", "Opus"),
        ("Sonnet 5.5", "Sonnet"),
        ("Haiku 5.5", "Haiku"),
        ("Fable 5.1", "Fable"),
        ("Opus 4.6 (1M context)", "Opus"),
    ];
    let efforts = [
        (None, ""),
        (Some("low"), " - Low"),
        (Some("medium"), " - Medium"),
        (Some("high"), " - High"),
        (Some("xhigh"), " - Xhigh"),
        (Some("max"), " - Max"),
    ];
    for (display, short) in models {
        for (level, label) in efforts {
            for fast in [false, true] {
                let effort = level.map_or(String::new(), |l| format!(r#","effort":{{"level":"{l}"}}"#));
                let input = format!(
                    r#"{{"session_id":"m","model":{{"display_name":"{display}"}}{effort},"fast_mode":{fast},
                        "rate_limits":{{"five_hour":{{"used_percentage":91,"resets_at":{}}}}},
                        "context_window":{{"used_percentage":12,"context_window_size":1000000}},"cost":{{"total_duration_ms":1}}}}"#,
                    now_secs() + 600
                );
                let expected = format!("{short}{label}{}", if fast { " · fast" } else { "" });
                for columns in [70, 100, 200] {
                    let out = run(&temp, &input, Some(columns));
                    assert!(out.text.starts_with(&expected), "{display} {level:?} fast={fast}: {:?}", out.text);
                    assert_eq!(out.width, columns - 8, "{display} {level:?} fast={fast} at {columns}");
                }
            }
        }
    }
}

#[test]
fn every_agy_model_and_mode_fits_and_reads_right() {
    let temp = TempDir::new();
    let models = [
        ("Gemini 3 Pro", "Gemini Pro"),
        ("Gemini 3 Flash", "Gemini Flash"),
        ("Claude Sonnet 4.5", "Claude Sonnet"),
        ("GPT-OSS 120B", "GPT-OSS"),
    ];
    let modes = [(None, ""), (Some("planning"), " - Planning"), (Some("fast"), " - Fast")];
    for (display, short) in models {
        for (mode, label) in modes {
            let mode = mode.map_or(String::new(), |m| format!(r#","execution_mode":"{m}""#));
            for columns in [70, 100, 200] {
                let input = format!(
                    r#"{{"product":"antigravity","conversation_id":"c","model":{{"id":"x","display_name":"{display}"}}{mode},
                        "quota":{{"x":{{"remaining_fraction":0.4,"reset_in_seconds":900}}}},"terminal_width":{columns}}}"#
                );
                let out = run(&temp, &input, None);
                assert!(out.text.starts_with(&format!("{short}{label}")), "{display} {mode}: {:?}", out.text);
                assert_eq!(out.width, columns - 8, "{display} {mode} at {columns}");
            }
        }
    }
}
