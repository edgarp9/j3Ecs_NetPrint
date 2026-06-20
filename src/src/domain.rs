use std::{convert::Infallible, error::Error, fmt, net::IpAddr};

pub const DEFAULT_PRINTER_IP: &str = "192.168.0.1";
pub const DEFAULT_PRINTER_PORT: u16 = 9100;
pub const DEFAULT_FONT_SIZE_PX: u32 = 42;
pub const DEFAULT_PAPER_WIDTH_PX: u32 = 576;
pub const DEFAULT_FONT_FACE_NAME: &str = "Malgun Gothic";
pub const DEFAULT_MARGIN_PX: u32 = 10;
pub const MIN_FONT_SIZE_PX: u32 = 5;
pub const MIN_PAPER_WIDTH_PX: u32 = 100;
const MAX_DNS_HOST_LEN: usize = 253;
const MAX_DNS_LABEL_LEN: usize = 63;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AppSettings {
    pub print: PrintSettings,
    pub ui: UiSettings,
}

impl AppSettings {
    pub fn validate(&self) -> Result<(), DomainError> {
        self.print.validate()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct UiSettings {
    pub theme: UiTheme,
    pub language: UiLanguage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UiTheme {
    #[default]
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UiLanguage {
    #[default]
    English,
    Korean,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PrintSettings {
    pub printer: NetworkPrinterTarget,
    pub layout: TextImageLayout,
}

impl PrintSettings {
    pub fn validate(&self) -> Result<(), DomainError> {
        self.printer.validate()?;
        self.layout.validate()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkPrinterTarget {
    pub ip: String,
    pub port: u16,
}

impl NetworkPrinterTarget {
    pub fn validate(&self) -> Result<(), DomainError> {
        let host = self.ip.trim();
        if host.is_empty() {
            return Err(DomainError::EmptyPrinterHost);
        }

        if !is_valid_printer_host(host) {
            return Err(DomainError::InvalidPrinterHost {
                value: host.to_owned(),
            });
        }

        if self.port == 0 {
            return Err(DomainError::InvalidPrinterPort { value: self.port });
        }

        Ok(())
    }
}

fn is_valid_printer_host(host: &str) -> bool {
    if host.parse::<IpAddr>().is_ok() {
        return true;
    }

    if looks_like_invalid_ipv4_literal(host) {
        return false;
    }

    is_valid_dns_hostname(host)
}

fn looks_like_invalid_ipv4_literal(host: &str) -> bool {
    host.contains('.')
        && host
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.')
}

fn is_valid_dns_hostname(host: &str) -> bool {
    if host.is_empty() || host.len() > MAX_DNS_HOST_LEN {
        return false;
    }

    host.split('.').all(is_valid_dns_label)
}

fn is_valid_dns_label(label: &str) -> bool {
    if label.is_empty() || label.len() > MAX_DNS_LABEL_LEN {
        return false;
    }

    let valid_chars = label
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-');

    valid_chars && !label.starts_with('-') && !label.ends_with('-')
}

impl Default for NetworkPrinterTarget {
    fn default() -> Self {
        Self {
            ip: DEFAULT_PRINTER_IP.to_owned(),
            port: DEFAULT_PRINTER_PORT,
        }
    }
}

impl fmt::Display for NetworkPrinterTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.ip, self.port)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextImageLayout {
    pub font_size_px: u32,
    pub paper_width_px: u32,
    pub margin_px: u32,
    pub font_face_name: String,
}

impl TextImageLayout {
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.font_size_px < MIN_FONT_SIZE_PX {
            return Err(DomainError::FontSizeTooSmall {
                min_px: MIN_FONT_SIZE_PX,
                actual_px: self.font_size_px,
            });
        }

        if self.paper_width_px < MIN_PAPER_WIDTH_PX {
            return Err(DomainError::PaperWidthTooSmall {
                min_px: MIN_PAPER_WIDTH_PX,
                actual_px: self.paper_width_px,
            });
        }

        if !matches!(self.safe_width_px(), Some(width_px) if width_px > 0) {
            return Err(DomainError::PrintableWidthTooSmall {
                paper_width_px: self.paper_width_px,
                margin_px: self.margin_px,
            });
        }

        if self.font_face_name.trim().is_empty() {
            return Err(DomainError::EmptyFontFaceName);
        }

        Ok(())
    }

    pub fn safe_width_px(&self) -> Option<u32> {
        self.printable_width_px()
    }

    pub fn printable_width_px(&self) -> Option<u32> {
        self.paper_width_px
            .checked_sub(self.margin_px.checked_mul(2)?)
    }
}

impl Default for TextImageLayout {
    fn default() -> Self {
        Self {
            font_size_px: DEFAULT_FONT_SIZE_PX,
            paper_width_px: DEFAULT_PAPER_WIDTH_PX,
            margin_px: DEFAULT_MARGIN_PX,
            font_face_name: DEFAULT_FONT_FACE_NAME.to_owned(),
        }
    }
}

impl fmt::Display for TextImageLayout {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let printable_width = self.printable_width_px().unwrap_or_default();

        write!(
            formatter,
            "font={}px, paper={}px, margin={}px, printable={}px, font_face={}",
            self.font_size_px,
            self.paper_width_px,
            self.margin_px,
            printable_width,
            self.font_face_name
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrintJob {
    pub settings: PrintSettings,
    pub text: String,
}

impl PrintJob {
    pub fn new(settings: PrintSettings, text: impl Into<String>) -> Result<Self, DomainError> {
        settings.validate()?;

        let text = text.into().trim().to_owned();
        if text.is_empty() {
            return Err(DomainError::EmptyText);
        }

        Ok(Self { settings, text })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedReceiptImage {
    pub width_px: u32,
    pub height_px: u32,
    pub rgb_pixels: Vec<u8>,
}

pub trait TextWidthMeasurer {
    fn measure_text_width_px(&self, text: &str) -> u32;
}

impl<F> TextWidthMeasurer for F
where
    F: Fn(&str) -> u32,
{
    fn measure_text_width_px(&self, text: &str) -> u32 {
        self(text)
    }
}

pub trait FallibleTextWidthMeasurer {
    type Error;

    fn try_measure_text_width_px(&self, text: &str) -> Result<u32, Self::Error>;
}

impl<M> FallibleTextWidthMeasurer for M
where
    M: TextWidthMeasurer + ?Sized,
{
    type Error = Infallible;

    fn try_measure_text_width_px(&self, text: &str) -> Result<u32, Self::Error> {
        Ok(self.measure_text_width_px(text))
    }
}

pub fn wrap_text_for_layout<M>(
    text: &str,
    layout: &TextImageLayout,
    measurer: &M,
) -> Result<Vec<String>, DomainError>
where
    M: TextWidthMeasurer + ?Sized,
{
    layout.validate()?;
    let safe_width_px = layout
        .safe_width_px()
        .ok_or(DomainError::PrintableWidthTooSmall {
            paper_width_px: layout.paper_width_px,
            margin_px: layout.margin_px,
        })?;

    Ok(wrap_text_to_width(text, safe_width_px, measurer))
}

pub fn try_wrap_text_for_layout<M>(
    text: &str,
    layout: &TextImageLayout,
    measurer: &M,
) -> Result<Vec<String>, TextWrapError<M::Error>>
where
    M: FallibleTextWidthMeasurer + ?Sized,
{
    layout.validate().map_err(TextWrapError::Layout)?;
    let safe_width_px = layout.safe_width_px().ok_or(TextWrapError::Layout(
        DomainError::PrintableWidthTooSmall {
            paper_width_px: layout.paper_width_px,
            margin_px: layout.margin_px,
        },
    ))?;

    try_wrap_text_to_width(text, safe_width_px, measurer).map_err(TextWrapError::Measure)
}

pub fn wrap_text_to_width<M>(text: &str, max_width_px: u32, measurer: &M) -> Vec<String>
where
    M: TextWidthMeasurer + ?Sized,
{
    let mut lines = Vec::new();

    for paragraph in text.split('\n') {
        let mut current_line = String::new();

        for character in paragraph.chars() {
            let previous_len = current_line.len();
            current_line.push(character);

            if measurer.measure_text_width_px(&current_line) > max_width_px {
                current_line.truncate(previous_len);
                lines.push(std::mem::take(&mut current_line));
                current_line.push(character);
            }
        }

        lines.push(current_line);
    }

    lines
}

pub fn try_wrap_text_to_width<M>(
    text: &str,
    max_width_px: u32,
    measurer: &M,
) -> Result<Vec<String>, M::Error>
where
    M: FallibleTextWidthMeasurer + ?Sized,
{
    let mut lines = Vec::new();

    for paragraph in text.split('\n') {
        let mut current_line = String::new();

        for character in paragraph.chars() {
            let previous_len = current_line.len();
            current_line.push(character);

            if measurer.try_measure_text_width_px(&current_line)? > max_width_px {
                current_line.truncate(previous_len);
                lines.push(std::mem::take(&mut current_line));
                current_line.push(character);
            }
        }

        lines.push(current_line);
    }

    Ok(lines)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextWrapError<E> {
    Layout(DomainError),
    Measure(E),
}

impl<E> fmt::Display for TextWrapError<E>
where
    E: fmt::Display,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Layout(error) => write!(formatter, "{error}"),
            Self::Measure(error) => write!(formatter, "{error}"),
        }
    }
}

impl<E> Error for TextWrapError<E>
where
    E: Error + 'static,
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Layout(error) => Some(error),
            Self::Measure(error) => Some(error),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    EmptyPrinterHost,
    InvalidPrinterHost { value: String },
    InvalidPrinterPort { value: u16 },
    EmptyFontFaceName,
    FontSizeTooSmall { min_px: u32, actual_px: u32 },
    PaperWidthTooSmall { min_px: u32, actual_px: u32 },
    PrintableWidthTooSmall { paper_width_px: u32, margin_px: u32 },
    EmptyText,
}

impl fmt::Display for DomainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPrinterHost => write!(formatter, "printer host must not be empty"),
            Self::InvalidPrinterHost { value } => {
                write!(formatter, "printer host is not valid: {value}")
            }
            Self::InvalidPrinterPort { value } => {
                write!(
                    formatter,
                    "printer port must be between 1 and 65535: {value}"
                )
            }
            Self::EmptyFontFaceName => write!(formatter, "font face name must not be empty"),
            Self::FontSizeTooSmall { min_px, actual_px } => write!(
                formatter,
                "font size is too small: expected at least {min_px}px, got {actual_px}px"
            ),
            Self::PaperWidthTooSmall { min_px, actual_px } => write!(
                formatter,
                "paper width is too small: expected at least {min_px}px, got {actual_px}px"
            ),
            Self::PrintableWidthTooSmall {
                paper_width_px,
                margin_px,
            } => write!(
                formatter,
                "paper width {paper_width_px}px is too small for {margin_px}px margins"
            ),
            Self::EmptyText => write!(formatter, "print text must not be empty"),
        }
    }
}

impl Error for DomainError {}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeTextMeasurer {
        default_width_px: u32,
        widths_by_char: &'static [(char, u32)],
    }

    impl FakeTextMeasurer {
        const fn new(default_width_px: u32, widths_by_char: &'static [(char, u32)]) -> Self {
            Self {
                default_width_px,
                widths_by_char,
            }
        }
    }

    impl TextWidthMeasurer for FakeTextMeasurer {
        fn measure_text_width_px(&self, text: &str) -> u32 {
            text.chars()
                .map(|character| {
                    self.widths_by_char
                        .iter()
                        .find_map(|(candidate, width_px)| {
                            (*candidate == character).then_some(*width_px)
                        })
                        .unwrap_or(self.default_width_px)
                })
                .sum()
        }
    }

    fn monospace_width(width_px: u32) -> impl Fn(&str) -> u32 {
        move |text| text.chars().count() as u32 * width_px
    }

    #[test]
    fn default_layout_matches_python_safe_width() {
        let layout = TextImageLayout::default();

        assert_eq!(layout.margin_px, 10);
        assert_eq!(layout.paper_width_px, 576);
        assert_eq!(layout.safe_width_px(), Some(556));
    }

    #[test]
    fn default_app_settings_use_light_ui_theme_and_english_language() {
        let settings = AppSettings::default();

        assert_eq!(settings.print, PrintSettings::default());
        assert_eq!(settings.ui.theme, UiTheme::Light);
        assert_eq!(settings.ui.language, UiLanguage::English);
        assert_eq!(settings.validate(), Ok(()));
    }

    #[test]
    fn layout_rejects_empty_font_face_name() {
        let layout = TextImageLayout {
            font_face_name: " \t ".to_owned(),
            ..TextImageLayout::default()
        };

        assert_eq!(layout.validate(), Err(DomainError::EmptyFontFaceName));
    }

    #[test]
    fn printer_target_accepts_ip_literals_and_dns_hostnames() {
        for host in ["127.0.0.1", "::1", "localhost", "receipt-printer.local"] {
            let target = NetworkPrinterTarget {
                ip: host.to_owned(),
                port: DEFAULT_PRINTER_PORT,
            };

            assert_eq!(target.validate(), Ok(()));
        }
    }

    #[test]
    fn printer_target_rejects_empty_or_invalid_hosts() {
        for host in ["", " \t ", "bad host", "-printer.local", "printer-.local"] {
            let target = NetworkPrinterTarget {
                ip: host.to_owned(),
                port: DEFAULT_PRINTER_PORT,
            };

            assert!(matches!(
                target.validate(),
                Err(DomainError::EmptyPrinterHost | DomainError::InvalidPrinterHost { .. })
            ));
        }

        let invalid_ipv4 = NetworkPrinterTarget {
            ip: "999.999.999.999".to_owned(),
            port: DEFAULT_PRINTER_PORT,
        };

        assert_eq!(
            invalid_ipv4.validate(),
            Err(DomainError::InvalidPrinterHost {
                value: "999.999.999.999".to_owned()
            })
        );
    }

    #[test]
    fn wrap_text_moves_candidate_that_exceeds_safe_width_to_next_line() {
        let lines = wrap_text_to_width("abcde", 20, &monospace_width(8));

        assert_eq!(lines, vec!["ab", "cd", "e"]);
    }

    #[test]
    fn wrap_text_keeps_empty_paragraphs() {
        let measurer = FakeTextMeasurer::new(10, &[]);
        let lines = wrap_text_to_width("first\n\nlast\n", 100, &measurer);

        assert_eq!(lines, vec!["first", "", "last", ""]);
    }

    #[test]
    fn wrap_text_matches_python_for_long_korean_and_english_text() {
        let measurer =
            FakeTextMeasurer::new(4, &[('가', 9), ('나', 9), ('다', 9), ('라', 9), ('마', 9)]);
        let lines = wrap_text_to_width("가나다라마abcdef", 20, &measurer);

        assert_eq!(lines, vec!["가나", "다라", "마ab", "cdef"]);
    }

    #[test]
    fn wrap_text_keeps_line_when_candidate_width_is_exactly_safe_width() {
        let layout = TextImageLayout {
            paper_width_px: 120,
            margin_px: 10,
            ..TextImageLayout::default()
        };
        let measurer = FakeTextMeasurer::new(10, &[]);

        let exact_lines =
            wrap_text_for_layout("abcdefghij", &layout, &measurer).expect("layout should be valid");
        let overflow_lines = wrap_text_for_layout("abcdefghijk", &layout, &measurer)
            .expect("layout should be valid");

        assert_eq!(layout.safe_width_px(), Some(100));
        assert_eq!(exact_lines, vec!["abcdefghij"]);
        assert_eq!(overflow_lines, vec!["abcdefghij", "k"]);
    }

    #[test]
    fn wrap_text_matches_python_when_single_character_exceeds_safe_width() {
        let measurer = FakeTextMeasurer::new(20, &[]);
        let lines = wrap_text_to_width("가", 10, &measurer);

        assert_eq!(lines, vec!["", "가"]);
    }

    #[test]
    fn wrap_text_for_layout_uses_layout_safe_width() {
        let layout = TextImageLayout {
            paper_width_px: 120,
            margin_px: 10,
            ..TextImageLayout::default()
        };

        let lines = wrap_text_for_layout("abcdef", &layout, &monospace_width(25))
            .expect("layout should be valid");

        assert_eq!(lines, vec!["abcd", "ef"]);
    }

    #[test]
    fn try_wrap_text_for_layout_propagates_measurement_errors() {
        struct FailingMeasurer;

        impl FallibleTextWidthMeasurer for FailingMeasurer {
            type Error = &'static str;

            fn try_measure_text_width_px(&self, _text: &str) -> Result<u32, Self::Error> {
                Err("measurement failed")
            }
        }

        let error = try_wrap_text_for_layout("abc", &TextImageLayout::default(), &FailingMeasurer)
            .expect_err("measurement error should be returned");

        assert_eq!(error, TextWrapError::Measure("measurement failed"));
    }
}
