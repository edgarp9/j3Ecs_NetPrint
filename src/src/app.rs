use std::{
    error::Error,
    fmt,
    path::{Path, PathBuf},
    str::FromStr,
};

use crate::{
    domain::{
        AppSettings, DEFAULT_FONT_FACE_NAME, DEFAULT_FONT_SIZE_PX, DEFAULT_MARGIN_PX,
        DEFAULT_PAPER_WIDTH_PX, DEFAULT_PRINTER_IP, DEFAULT_PRINTER_PORT, DomainError,
        NetworkPrinterTarget, PrintJob, PrintSettings, TextImageLayout, UiLanguage, UiTheme,
    },
    infra::{self, EscPosPrinter, InfraError, SettingsFileError, TextImageRenderer},
    platform::{self, NativeWindowHandle},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputValidationError {
    NumericFieldsMustBeNumbers,
}

impl fmt::Display for InputValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NumericFieldsMustBeNumbers => write!(
                formatter,
                "printer port, font size, and paper width must be numeric"
            ),
        }
    }
}

impl Error for InputValidationError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppError {
    Input(InputValidationError),
    Domain(DomainError),
    Infra(InfraError),
    Settings(SettingsFileError),
    Ui(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Input(error) => write!(formatter, "{error}"),
            Self::Domain(error) => write!(formatter, "{error}"),
            Self::Infra(error) => write!(formatter, "{error}"),
            Self::Settings(error) => write!(formatter, "{error}"),
            Self::Ui(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for AppError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Input(error) => Some(error),
            Self::Domain(error) => Some(error),
            Self::Infra(error) => Some(error),
            Self::Settings(error) => Some(error),
            Self::Ui(_) => None,
        }
    }
}

impl From<InputValidationError> for AppError {
    fn from(error: InputValidationError) -> Self {
        Self::Input(error)
    }
}

impl From<DomainError> for AppError {
    fn from(error: DomainError) -> Self {
        Self::Domain(error)
    }
}

impl From<InfraError> for AppError {
    fn from(error: InfraError) -> Self {
        Self::Infra(error)
    }
}

impl From<SettingsFileError> for AppError {
    fn from(error: SettingsFileError) -> Self {
        Self::Settings(error)
    }
}

#[derive(Debug)]
pub struct AppState {
    app_settings: AppSettings,
    settings_file_path: PathBuf,
    owner_window: NativeWindowHandle,
}

impl AppState {
    pub fn bootstrap() -> Result<Self, AppError> {
        let settings_file_path = infra::settings_file_path_next_to_executable()?;
        let app_settings = infra::load_or_create_app_settings_file(&settings_file_path)?;
        app_settings.validate()?;

        Ok(Self {
            app_settings,
            settings_file_path,
            owner_window: platform::default_owner_window(),
        })
    }

    pub fn default_settings(&self) -> &PrintSettings {
        &self.app_settings.print
    }

    pub fn app_settings(&self) -> &AppSettings {
        &self.app_settings
    }

    pub fn ui_theme(&self) -> UiTheme {
        self.app_settings.ui.theme
    }

    pub fn ui_language(&self) -> UiLanguage {
        self.app_settings.ui.language
    }

    pub fn settings_file_path(&self) -> &Path {
        &self.settings_file_path
    }

    pub fn owner_window(&self) -> NativeWindowHandle {
        self.owner_window
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrintJobInput {
    pub printer_ip: String,
    pub printer_port: String,
    pub font_size_px: String,
    pub paper_width_px: String,
    pub font_face_name: String,
    pub text: String,
}

impl Default for PrintJobInput {
    fn default() -> Self {
        Self {
            printer_ip: DEFAULT_PRINTER_IP.to_owned(),
            printer_port: DEFAULT_PRINTER_PORT.to_string(),
            font_size_px: DEFAULT_FONT_SIZE_PX.to_string(),
            paper_width_px: DEFAULT_PAPER_WIDTH_PX.to_string(),
            font_face_name: DEFAULT_FONT_FACE_NAME.to_owned(),
            text: String::new(),
        }
    }
}

impl PrintJobInput {
    pub fn from_settings(settings: &PrintSettings) -> Self {
        Self {
            printer_ip: settings.printer.ip.clone(),
            printer_port: settings.printer.port.to_string(),
            font_size_px: settings.layout.font_size_px.to_string(),
            paper_width_px: settings.layout.paper_width_px.to_string(),
            font_face_name: settings.layout.font_face_name.clone(),
            text: String::new(),
        }
    }

    pub fn into_print_job(self) -> Result<PrintJob, AppError> {
        let text = self.text.trim().to_owned();
        if text.is_empty() {
            return Err(DomainError::EmptyText.into());
        }

        let font_face_name = self.font_face_name.trim().to_owned();
        if font_face_name.is_empty() {
            return Err(DomainError::EmptyFontFaceName.into());
        }

        let printer_port = parse_numeric_field(&self.printer_port)?;
        let font_size_px = parse_numeric_field(&self.font_size_px)?;
        let paper_width_px = parse_numeric_field(&self.paper_width_px)?;

        let settings = PrintSettings {
            printer: NetworkPrinterTarget {
                ip: self.printer_ip.trim().to_owned(),
                port: printer_port,
            },
            layout: TextImageLayout {
                font_size_px,
                paper_width_px,
                margin_px: DEFAULT_MARGIN_PX,
                font_face_name,
            },
        };

        Ok(PrintJob::new(settings, text)?)
    }
}

pub fn build_print_job(input: PrintJobInput) -> Result<PrintJob, AppError> {
    input.into_print_job()
}

pub fn save_print_settings(path: &Path, settings: &PrintSettings) -> Result<(), AppError> {
    infra::save_print_settings_file(path, settings)?;
    Ok(())
}

pub fn save_app_settings(path: &Path, settings: &AppSettings) -> Result<(), AppError> {
    infra::save_app_settings_file(path, settings)?;
    Ok(())
}

fn parse_numeric_field<T>(value: &str) -> Result<T, InputValidationError>
where
    T: FromStr,
{
    value
        .trim()
        .parse()
        .map_err(|_| InputValidationError::NumericFieldsMustBeNumbers)
}

pub fn execute_print_job(
    job: &PrintJob,
    renderer: &dyn TextImageRenderer,
    printer: &dyn EscPosPrinter,
) -> Result<(), AppError> {
    let image = renderer.render_text(job)?;
    printer.send_image_and_cut(&job.settings.printer, &image)?;
    Ok(())
}

pub fn run() -> Result<(), AppError> {
    #[cfg(target_os = "windows")]
    {
        platform::win32_gui::run().map_err(|error| AppError::Ui(error.to_string()))?;
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        Err(InfraError::UnsupportedPlatform("Windows GUI printer app").into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc};

    use crate::domain::RenderedReceiptImage;

    fn valid_input() -> PrintJobInput {
        PrintJobInput {
            text: "hello\nworld".to_owned(),
            ..PrintJobInput::default()
        }
    }

    #[test]
    fn build_print_job_uses_python_defaults() {
        let job = build_print_job(valid_input()).expect("default input should build");

        assert_eq!(job.settings.printer.ip, "192.168.0.1");
        assert_eq!(job.settings.printer.port, 9100);
        assert_eq!(job.settings.layout.font_size_px, 42);
        assert_eq!(job.settings.layout.paper_width_px, 576);
        assert_eq!(job.settings.layout.margin_px, 10);
        assert_eq!(job.settings.layout.safe_width_px(), Some(556));
        assert_eq!(job.settings.layout.font_face_name, "Malgun Gothic");
        assert_eq!(job.text, "hello\nworld");
    }

    #[test]
    fn default_app_state_ui_preferences_are_light_theme_and_english_language() {
        assert_eq!(AppSettings::default().ui.theme, UiTheme::Light);
        assert_eq!(AppSettings::default().ui.language, UiLanguage::English);
    }

    #[test]
    fn print_job_input_from_settings_uses_settings_without_text() {
        let settings = PrintSettings {
            printer: NetworkPrinterTarget {
                ip: "127.0.0.1".to_owned(),
                port: 9101,
            },
            layout: TextImageLayout {
                font_size_px: 36,
                paper_width_px: 384,
                margin_px: 12,
                font_face_name: "Arial".to_owned(),
            },
        };

        let input = PrintJobInput::from_settings(&settings);

        assert_eq!(input.printer_ip, "127.0.0.1");
        assert_eq!(input.printer_port, "9101");
        assert_eq!(input.font_size_px, "36");
        assert_eq!(input.paper_width_px, "384");
        assert_eq!(input.font_face_name, "Arial");
        assert_eq!(input.text, "");
    }

    #[test]
    fn build_print_job_rejects_empty_text_before_numeric_fields() {
        let input = PrintJobInput {
            text: " \n\t ".to_owned(),
            printer_port: "not-a-port".to_owned(),
            ..valid_input()
        };

        assert_eq!(
            build_print_job(input),
            Err(AppError::Domain(DomainError::EmptyText))
        );
    }

    #[test]
    fn build_print_job_rejects_empty_font_face_name() {
        let input = PrintJobInput {
            font_face_name: "   ".to_owned(),
            ..valid_input()
        };

        assert_eq!(
            build_print_job(input),
            Err(AppError::Domain(DomainError::EmptyFontFaceName))
        );
    }

    #[test]
    fn build_print_job_rejects_non_numeric_port() {
        assert_eq!(
            build_print_job(PrintJobInput {
                printer_port: "not-a-port".to_owned(),
                ..valid_input()
            }),
            Err(AppError::Input(
                InputValidationError::NumericFieldsMustBeNumbers
            ))
        );
    }

    #[test]
    fn build_print_job_rejects_non_numeric_font_size() {
        assert_eq!(
            build_print_job(PrintJobInput {
                font_size_px: "large".to_owned(),
                ..valid_input()
            }),
            Err(AppError::Input(
                InputValidationError::NumericFieldsMustBeNumbers
            ))
        );
    }

    #[test]
    fn build_print_job_rejects_non_numeric_paper_width() {
        assert_eq!(
            build_print_job(PrintJobInput {
                paper_width_px: "wide".to_owned(),
                ..valid_input()
            }),
            Err(AppError::Input(
                InputValidationError::NumericFieldsMustBeNumbers
            ))
        );
    }

    #[test]
    fn build_print_job_rejects_font_size_below_minimum() {
        let input = PrintJobInput {
            font_size_px: "4".to_owned(),
            ..valid_input()
        };

        assert_eq!(
            build_print_job(input),
            Err(AppError::Domain(DomainError::FontSizeTooSmall {
                min_px: 5,
                actual_px: 4,
            }))
        );
    }

    #[test]
    fn build_print_job_rejects_paper_width_below_minimum() {
        let input = PrintJobInput {
            paper_width_px: "99".to_owned(),
            ..valid_input()
        };

        assert_eq!(
            build_print_job(input),
            Err(AppError::Domain(DomainError::PaperWidthTooSmall {
                min_px: 100,
                actual_px: 99,
            }))
        );
    }

    #[test]
    fn build_print_job_accepts_minimum_font_size_and_paper_width() {
        let job = build_print_job(PrintJobInput {
            font_size_px: "5".to_owned(),
            paper_width_px: "100".to_owned(),
            ..valid_input()
        })
        .expect("minimum valid font size and paper width should build");

        assert_eq!(job.settings.layout.font_size_px, 5);
        assert_eq!(job.settings.layout.paper_width_px, 100);
        assert_eq!(job.settings.layout.safe_width_px(), Some(80));
    }

    #[test]
    fn execute_print_job_renders_then_sends_image_to_configured_printer() {
        let job = build_print_job(valid_input()).expect("valid input should build");
        let events = Rc::new(RefCell::new(Vec::new()));
        let image = RenderedReceiptImage {
            width_px: 2,
            height_px: 1,
            rgb_pixels: vec![255, 255, 255, 0, 0, 0],
        };
        let renderer = RecordingRenderer {
            events: Rc::clone(&events),
            image: image.clone(),
        };
        let printer = RecordingPrinter {
            events: Rc::clone(&events),
            captured: RefCell::new(None),
        };

        execute_print_job(&job, &renderer, &printer).expect("print workflow should execute");

        assert_eq!(*events.borrow(), vec!["render", "send"]);
        assert_eq!(
            *printer.captured.borrow(),
            Some((job.settings.printer.clone(), image))
        );
    }

    struct RecordingRenderer {
        events: Rc<RefCell<Vec<&'static str>>>,
        image: RenderedReceiptImage,
    }

    impl TextImageRenderer for RecordingRenderer {
        fn render_text(&self, job: &PrintJob) -> Result<RenderedReceiptImage, InfraError> {
            self.events.borrow_mut().push("render");
            assert_eq!(job.text, "hello\nworld");

            Ok(self.image.clone())
        }
    }

    struct RecordingPrinter {
        events: Rc<RefCell<Vec<&'static str>>>,
        captured: RefCell<Option<(NetworkPrinterTarget, RenderedReceiptImage)>>,
    }

    impl EscPosPrinter for RecordingPrinter {
        fn send_image_and_cut(
            &self,
            target: &NetworkPrinterTarget,
            image: &RenderedReceiptImage,
        ) -> Result<(), InfraError> {
            self.events.borrow_mut().push("send");
            self.captured
                .borrow_mut()
                .replace((target.clone(), image.clone()));

            Ok(())
        }
    }
}
