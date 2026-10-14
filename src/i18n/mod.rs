mod latin;
mod scripts;

use std::env;
use std::sync::OnceLock;

use latin::*;
use scripts::*;

pub struct Labels {
    pub code: &'static str,
    pub session: &'static str,
    pub week: &'static str,
    pub compact: &'static str,
    pub context: &'static str,
    pub quota: &'static str,
    pub time: &'static str,
    pub resets_in: &'static str,
    pub limit_near: &'static str,
    pub limit_reached: &'static str,
}

pub const ALL: [&Labels; 20] = [
    &ENGLISH,
    &PORTUGUESE,
    &SPANISH,
    &CHINESE,
    &HINDI,
    &ARABIC,
    &FRENCH,
    &GERMAN,
    &JAPANESE,
    &RUSSIAN,
    &KOREAN,
    &ITALIAN,
    &INDONESIAN,
    &TURKISH,
    &VIETNAMESE,
    &THAI,
    &BENGALI,
    &URDU,
    &POLISH,
    &DUTCH,
];

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
    let language = match locale.get(..2).map(str::to_ascii_lowercase).as_deref() {
        Some("in") => "id".to_owned(),
        Some(other) => other.to_owned(),
        None => return &ENGLISH,
    };
    ALL.into_iter().find(|labels| labels.code.get(..2) == Some(language.as_str())).unwrap_or(&ENGLISH)
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
    fn locales_map_to_their_language() {
        assert_eq!(for_locale("pt_BR.UTF-8").session, "sessão");
        assert_eq!(for_locale("es-ES").session, "sesión");
        assert_eq!(for_locale("fr_FR").week, "semaine");
        assert_eq!(for_locale("zh_CN.UTF-8").session, "会话");
        assert_eq!(for_locale("de-DE").session, "Sitzung");
        assert_eq!(for_locale("ja_JP.UTF-8").code, "ja");
        assert_eq!(for_locale("hi-IN").code, "hi");
        assert_eq!(for_locale("ar_SA").code, "ar");
        assert_eq!(for_locale("in_ID").code, "id");
        assert_eq!(for_locale("nl-BE").code, "nl");
        assert_eq!(for_locale("en_US").session, "session");
        assert_eq!(for_locale("sv_SE").session, "session");
        assert_eq!(for_locale("").session, "session");
    }

    #[test]
    fn codes_are_unique_and_every_text_is_filled() {
        for (i, labels) in ALL.iter().enumerate() {
            assert!(ALL[..i].iter().all(|other| other.code.get(..2) != labels.code.get(..2)), "{}", labels.code);
            let texts = [
                labels.session,
                labels.week,
                labels.compact,
                labels.context,
                labels.quota,
                labels.time,
                labels.resets_in,
                labels.limit_near,
                labels.limit_reached,
            ];
            assert!(texts.iter().all(|text| !text.trim().is_empty() && !text.contains("  ")), "{}", labels.code);
        }
    }
}
