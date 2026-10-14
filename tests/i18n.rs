mod common;

use common::*;

const LANGUAGES: [(&str, &str, &str, &str); 20] = [
    ("en", "session", "compact", "time"),
    ("pt_BR.UTF-8", "sessão", "compactar", "tempo"),
    ("es", "sesión", "compactar", "tiempo"),
    ("zh_CN", "会话", "压缩", "时长"),
    ("hi_IN", "सत्र", "संक्षेपण", "समय"),
    ("ar_SA", "الجلسة", "الضغط", "الوقت"),
    ("fr", "session", "compacter", "temps"),
    ("de_DE", "Sitzung", "verdichten", "Zeit"),
    ("ja_JP", "セッション", "圧縮", "時間"),
    ("ru_RU", "сессия", "сжатие", "время"),
    ("ko_KR", "세션", "압축", "시간"),
    ("it_IT", "sessione", "compattare", "tempo"),
    ("id_ID", "sesi", "ringkas", "waktu"),
    ("tr_TR", "oturum", "sıkıştır", "süre"),
    ("vi_VN", "phiên", "nén", "thời gian"),
    ("th_TH", "เซสชัน", "บีบอัด", "เวลา"),
    ("bn_BD", "সেশন", "সংকোচন", "সময়"),
    ("ur_PK", "سیشن", "کمپیکٹ", "وقت"),
    ("pl_PL", "sesja", "kompaktowanie", "czas"),
    ("nl_NL", "sessie", "compacteren", "tijd"),
];

fn columns(text: &str) -> usize {
    text.chars()
        .map(|c| match c as u32 {
            0x0900..=0x0902 | 0x093C | 0x0941..=0x0948 | 0x094D | 0x09BC | 0x09C1..=0x09C4 | 0x09CD => 0,
            0x0E31 | 0x0E34..=0x0E3A | 0x0E47..=0x0E4E => 0,
            0x1100..=0x115F | 0x3041..=0x33FF | 0x4E00..=0x9FFF | 0xAC00..=0xD7A3 | 0xFF00..=0xFF60 => 2,
            _ => 1,
        })
        .sum()
}

#[test]
fn every_language_labels_the_line_and_keeps_the_width() {
    let temp = TempDir::new();
    let payload = r#"{"session_id":"lang","model":{"display_name":"Opus 5.5"},
        "rate_limits":{"five_hour":{"used_percentage":40}},"context_window":{"used_percentage":20,"context_window_size":200000},
        "cost":{"total_duration_ms":60000}}"#;
    for (lang, session, compact, time) in LANGUAGES {
        let out = run_with(&temp, payload, Some(120), &[("STATUS_CLI_LANG", lang)]);
        for label in [session, compact, time] {
            assert!(out.text.contains(label), "{lang}: {:?}", out.text);
        }
        assert_eq!(columns(&out.visible), 112, "{lang}: {:?}", out.visible);
    }
}
