use std::env;
use std::sync::OnceLock;

pub struct Labels {
    pub code: &'static str,
    pub session: &'static str,
    pub week: &'static str,
    pub compact: &'static str,
    pub context: &'static str,
    pub quota: &'static str,
    pub time: &'static str,
}

const ENGLISH: Labels = Labels {
    code: "en",
    session: "session",
    week: "week",
    compact: "compact",
    context: "context",
    quota: "quota",
    time: "time",
};
const SPANISH: Labels = Labels {
    code: "es",
    session: "sesión",
    week: "semana",
    compact: "compactar",
    context: "contexto",
    quota: "cuota",
    time: "tiempo",
};
const PORTUGUESE: Labels = Labels {
    code: "pt",
    session: "sessão",
    week: "semana",
    compact: "compactar",
    context: "contexto",
    quota: "cota",
    time: "tempo",
};
const FRENCH: Labels = Labels {
    code: "fr",
    session: "session",
    week: "semaine",
    compact: "compacter",
    context: "contexte",
    quota: "quota",
    time: "temps",
};
const CHINESE: Labels = Labels {
    code: "zh",
    session: "会话",
    week: "本周",
    compact: "压缩",
    context: "上下文",
    quota: "配额",
    time: "时长",
};

pub fn labels() -> &'static Labels {
    if cfg!(test) {
        return &PORTUGUESE;
    }
    static LABELS: OnceLock<&'static Labels> = OnceLock::new();
    LABELS.get_or_init(|| {
        let requested = ["STATUS_CLI_LANG", "LC_ALL", "LC_MESSAGES", "LANG"]
            .into_iter()
            .filter_map(|name| env::var(name).ok())
            .find(|value| !value.is_empty() && value != "C" && value != "POSIX");
        for_locale(&requested.or_else(system_locale).unwrap_or_default())
    })
}

fn for_locale(locale: &str) -> &'static Labels {
    match locale.get(..2).map(str::to_ascii_lowercase).as_deref() {
        Some("es") => &SPANISH,
        Some("pt") => &PORTUGUESE,
        Some("fr") => &FRENCH,
        Some("zh") => &CHINESE,
        _ => &ENGLISH,
    }
}

#[cfg(windows)]
fn system_locale() -> Option<String> {
    use windows_sys::Win32::Globalization::GetUserDefaultLocaleName;

    let mut buffer = [0u16; 85];
    let len = unsafe { GetUserDefaultLocaleName(buffer.as_mut_ptr(), buffer.len() as i32) };
    (len > 1).then(|| String::from_utf16_lossy(&buffer[..len as usize - 1]))
}

#[cfg(not(windows))]
fn system_locale() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locales_map_to_the_five_languages() {
        assert_eq!(for_locale("pt_BR.UTF-8").session, "sessão");
        assert_eq!(for_locale("es-ES").session, "sesión");
        assert_eq!(for_locale("fr_FR").week, "semaine");
        assert_eq!(for_locale("zh_CN.UTF-8").session, "会话");
        assert_eq!(for_locale("en_US").session, "session");
        assert_eq!(for_locale("de_DE").session, "session");
        assert_eq!(for_locale("").session, "session");
    }
}
