use crate::domain::{UiLanguage, UiTheme};

#[derive(Debug)]
pub(super) struct UiText {
    pub(super) settings_group_label: &'static str,
    pub(super) text_group_label: &'static str,
    pub(super) ip_label: &'static str,
    pub(super) port_label: &'static str,
    pub(super) font_size_label: &'static str,
    pub(super) paper_width_label: &'static str,
    pub(super) font_face_label: &'static str,
    pub(super) theme_label: &'static str,
    pub(super) language_label: &'static str,
    pub(super) theme_light_label: &'static str,
    pub(super) theme_dark_label: &'static str,
    pub(super) language_english_label: &'static str,
    pub(super) language_korean_label: &'static str,
    pub(super) version_label: &'static str,
    pub(super) about_link_label: &'static str,
    pub(super) settings_hide_button_label: &'static str,
    pub(super) settings_show_button_label: &'static str,
    pub(super) print_button_label: &'static str,
    pub(super) printing_button_label: &'static str,
}

static ENGLISH_UI_TEXT: UiText = UiText {
    settings_group_label: "Printer and Font Settings",
    text_group_label: "Print Text",
    ip_label: "IP/Host:",
    port_label: "Port:",
    font_size_label: "Font Size:",
    paper_width_label: "Paper Width:",
    font_face_label: "Font:",
    theme_label: "Theme:",
    language_label: "Language:",
    theme_light_label: "Light",
    theme_dark_label: "Dark",
    language_english_label: "English",
    language_korean_label: "Korean",
    version_label: "Version",
    about_link_label: "About",
    settings_hide_button_label: "Hide Settings",
    settings_show_button_label: "Show Settings",
    print_button_label: "Convert to Image and Print",
    printing_button_label: "Printing...",
};

static KOREAN_UI_TEXT: UiText = UiText {
    settings_group_label: "프린터 및 폰트 설정",
    text_group_label: "출력할 내용",
    ip_label: "IP/호스트:",
    port_label: "Port:",
    font_size_label: "폰트 크기:",
    paper_width_label: "용지 폭:",
    font_face_label: "폰트:",
    theme_label: "테마:",
    language_label: "언어:",
    theme_light_label: "라이트",
    theme_dark_label: "다크",
    language_english_label: "영어",
    language_korean_label: "한글",
    version_label: "버전",
    about_link_label: "정보",
    settings_hide_button_label: "설정 숨기기",
    settings_show_button_label: "설정 보이기",
    print_button_label: "이미지로 변환 및 출력",
    printing_button_label: "출력 중...",
};

pub(super) const fn ui_text(language: UiLanguage) -> &'static UiText {
    match language {
        UiLanguage::English => &ENGLISH_UI_TEXT,
        UiLanguage::Korean => &KOREAN_UI_TEXT,
    }
}

pub(super) const fn localized(
    language: UiLanguage,
    english: &'static str,
    korean: &'static str,
) -> &'static str {
    match language {
        UiLanguage::English => english,
        UiLanguage::Korean => korean,
    }
}

pub(super) fn ui_theme_label(theme: UiTheme, language: UiLanguage) -> &'static str {
    let text = ui_text(language);
    match theme {
        UiTheme::Light => text.theme_light_label,
        UiTheme::Dark => text.theme_dark_label,
    }
}

pub(super) fn ui_theme_from_label(label: &str) -> Option<UiTheme> {
    match label.trim() {
        "Light" | "라이트" => Some(UiTheme::Light),
        "Dark" | "다크" => Some(UiTheme::Dark),
        _ => None,
    }
}

pub(super) fn ui_language_label(
    language: UiLanguage,
    display_language: UiLanguage,
) -> &'static str {
    let text = ui_text(display_language);
    match language {
        UiLanguage::English => text.language_english_label,
        UiLanguage::Korean => text.language_korean_label,
    }
}

pub(super) fn ui_language_from_label(label: &str) -> Option<UiLanguage> {
    match label.trim() {
        "English" | "영어" => Some(UiLanguage::English),
        "Korean" | "한국어" | "한글" => Some(UiLanguage::Korean),
        _ => None,
    }
}
