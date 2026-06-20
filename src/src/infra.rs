use std::{
    env,
    error::Error,
    fmt, fs,
    io::{self, Write},
    net::{SocketAddr, TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

pub use crate::domain::RenderedReceiptImage;

use crate::domain::{
    AppSettings, DomainError, NetworkPrinterTarget, PrintJob, PrintSettings, TextImageLayout,
    UiLanguage, UiSettings, UiTheme,
};

const CONFIG_PRINTER_SECTION: &str = "printer";
const CONFIG_LAYOUT_SECTION: &str = "layout";
const CONFIG_UI_SECTION: &str = "ui";
const CONFIG_PRINTER_IP_KEY: &str = "ip";
const CONFIG_PRINTER_PORT_KEY: &str = "port";
const CONFIG_FONT_SIZE_KEY: &str = "font_size_px";
const CONFIG_PAPER_WIDTH_KEY: &str = "paper_width_px";
const CONFIG_MARGIN_KEY: &str = "margin_px";
const CONFIG_FONT_FACE_KEY: &str = "font_face_name";
const CONFIG_LEGACY_FONT_FILE_KEY: &str = "font_file_name";
const CONFIG_UI_THEME_KEY: &str = "theme";
const CONFIG_UI_LANGUAGE_KEY: &str = "language";
const CONFIG_UI_THEME_LIGHT: &str = "light";
const CONFIG_UI_THEME_DARK: &str = "dark";
const CONFIG_UI_LANGUAGE_ENGLISH: &str = "english";
const CONFIG_UI_LANGUAGE_KOREAN: &str = "korean";
const SETTINGS_SAVE_TEMP_FILE_SUFFIX: &str = ".new";
const SETTINGS_SAVE_TEMP_ATTEMPTS: usize = 16;

const BYTES_PER_RGB_PIXEL: usize = 3;
const ESC_POS_ESC: u8 = 0x1B;
const ESC_POS_GS: u8 = 0x1D;
const ESC_POS_CUT_COMMAND: u8 = b'V';
const ESC_POS_PRINT_AND_FEED_COMMAND: u8 = b'd';
const ESC_POS_RASTER_MODE_NORMAL: u8 = 0x00;
const ESC_POS_RASTER_HEADER_LEN: usize = 8;
const ESC_POS_DEFAULT_CUT_FEED_LINES: u8 = 6;
const ESC_POS_DEFAULT_IMAGE_FRAGMENT_HEIGHT_PX: u16 = 960;
const PILLOW_BILEVEL_THRESHOLD: i32 = 128;
const PILLOW_LUMA_RED: u32 = 19_595;
const PILLOW_LUMA_GREEN: u32 = 38_470;
const PILLOW_LUMA_BLUE: u32 = 7_471;
const PILLOW_LUMA_ROUNDING: u32 = 0x8000;
const PRINTER_NETWORK_TIMEOUT: Duration = Duration::from_secs(60);

static SETTINGS_SAVE_TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub trait TextImageRenderer {
    fn render_text(&self, job: &PrintJob) -> Result<RenderedReceiptImage, InfraError>;
}

pub trait EscPosPrinter {
    fn send_image_and_cut(
        &self,
        target: &NetworkPrinterTarget,
        image: &RenderedReceiptImage,
    ) -> Result<(), InfraError>;
}

pub fn settings_file_path_next_to_executable() -> Result<PathBuf, SettingsFileError> {
    let executable_path =
        env::current_exe().map_err(|error| SettingsFileError::CurrentExecutablePath {
            details: error.to_string(),
        })?;

    settings_file_path_for_executable(&executable_path)
}

fn settings_file_path_for_executable(executable_path: &Path) -> Result<PathBuf, SettingsFileError> {
    let Some(executable_dir) = executable_path.parent() else {
        return Err(SettingsFileError::ExecutableDirectoryUnavailable {
            executable_path: executable_path.to_path_buf(),
        });
    };
    let Some(executable_stem) = executable_path.file_stem() else {
        return Err(SettingsFileError::ExecutableFileNameUnavailable {
            executable_path: executable_path.to_path_buf(),
        });
    };

    let mut settings_file_name = executable_stem.to_os_string();
    settings_file_name.push(".toml");
    Ok(executable_dir.join(settings_file_name))
}

pub fn load_or_create_print_settings_file(path: &Path) -> Result<PrintSettings, SettingsFileError> {
    Ok(load_or_create_app_settings_file(path)?.print)
}

pub fn load_or_create_app_settings_file(path: &Path) -> Result<AppSettings, SettingsFileError> {
    match fs::read_to_string(path) {
        Ok(contents) => parse_app_settings_toml(&contents, path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let settings = AppSettings::default();
            settings
                .validate()
                .map_err(|error| SettingsFileError::InvalidSettings {
                    path: path.to_path_buf(),
                    details: error.to_string(),
                })?;
            save_app_settings_file(path, &settings)?;
            Ok(settings)
        }
        Err(error) => Err(SettingsFileError::Read {
            path: path.to_path_buf(),
            details: error.to_string(),
        }),
    }
}

pub fn save_print_settings_file(
    path: &Path,
    settings: &PrintSettings,
) -> Result<(), SettingsFileError> {
    let app_settings = AppSettings {
        print: settings.clone(),
        ui: existing_ui_settings_or_default(path),
    };
    save_app_settings_file(path, &app_settings)
}

pub fn save_app_settings_file(
    path: &Path,
    settings: &AppSettings,
) -> Result<(), SettingsFileError> {
    settings
        .validate()
        .map_err(|error| SettingsFileError::InvalidSettings {
            path: path.to_path_buf(),
            details: error.to_string(),
        })?;

    let contents =
        app_settings_to_toml(settings).map_err(|error| SettingsFileError::Serialize {
            path: path.to_path_buf(),
            details: error.to_string(),
        })?;

    write_settings_file_atomically(path, contents.as_bytes())
}

fn existing_ui_settings_or_default(path: &Path) -> UiSettings {
    fs::read_to_string(path)
        .ok()
        .and_then(|contents| parse_app_settings_toml(&contents, path).ok())
        .map(|settings| settings.ui)
        .unwrap_or_default()
}

fn write_settings_file_atomically(path: &Path, contents: &[u8]) -> Result<(), SettingsFileError> {
    for _ in 0..SETTINGS_SAVE_TEMP_ATTEMPTS {
        let temp_path = temporary_settings_file_path(path)?;
        let temp_file = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(settings_write_error(
                    path,
                    "create temporary settings file",
                    error,
                ));
            }
        };

        return write_settings_file_to_temp_path(path, &temp_path, temp_file, contents);
    }

    Err(SettingsFileError::Write {
        path: path.to_path_buf(),
        details: "failed to create a unique temporary settings file".to_owned(),
    })
}

fn temporary_settings_file_path(path: &Path) -> Result<PathBuf, SettingsFileError> {
    let Some(file_name) = path.file_name() else {
        return Err(SettingsFileError::Write {
            path: path.to_path_buf(),
            details: "settings file path has no file name".to_owned(),
        });
    };
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let counter = SETTINGS_SAVE_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut temp_file_name = file_name.to_os_string();
    temp_file_name.push(format!(
        ".{}.{}{}",
        std::process::id(),
        counter,
        SETTINGS_SAVE_TEMP_FILE_SUFFIX
    ));

    Ok(parent.join(temp_file_name))
}

fn write_settings_file_to_temp_path(
    path: &Path,
    temp_path: &Path,
    mut temp_file: fs::File,
    contents: &[u8],
) -> Result<(), SettingsFileError> {
    let result = (|| {
        temp_file
            .write_all(contents)
            .map_err(|error| settings_write_error(path, "write temporary settings file", error))?;
        temp_file
            .flush()
            .map_err(|error| settings_write_error(path, "flush temporary settings file", error))?;
        temp_file
            .sync_all()
            .map_err(|error| settings_write_error(path, "sync temporary settings file", error))?;
        drop(temp_file);

        fs::rename(temp_path, path)
            .map_err(|error| settings_write_error(path, "replace settings file", error))?;

        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(temp_path);
    }

    result
}

fn settings_write_error(
    path: &Path,
    operation: &'static str,
    error: impl fmt::Display,
) -> SettingsFileError {
    SettingsFileError::Write {
        path: path.to_path_buf(),
        details: format!("{operation}: {error}"),
    }
}

#[cfg(test)]
fn print_settings_to_toml(settings: &PrintSettings) -> Result<String, toml::ser::Error> {
    let mut root = toml::Table::new();

    insert_print_settings_toml(&mut root, settings);
    serialize_toml(root)
}

fn app_settings_to_toml(settings: &AppSettings) -> Result<String, toml::ser::Error> {
    let mut root = toml::Table::new();
    let mut ui = toml::Table::new();

    insert_print_settings_toml(&mut root, &settings.print);
    ui.insert(
        CONFIG_UI_THEME_KEY.to_owned(),
        toml::Value::String(ui_theme_to_config_value(settings.ui.theme).to_owned()),
    );
    ui.insert(
        CONFIG_UI_LANGUAGE_KEY.to_owned(),
        toml::Value::String(ui_language_to_config_value(settings.ui.language).to_owned()),
    );
    root.insert(CONFIG_UI_SECTION.to_owned(), toml::Value::Table(ui));

    serialize_toml(root)
}

fn insert_print_settings_toml(root: &mut toml::Table, settings: &PrintSettings) {
    let mut printer = toml::Table::new();
    let mut layout = toml::Table::new();

    printer.insert(
        CONFIG_PRINTER_IP_KEY.to_owned(),
        toml::Value::String(settings.printer.ip.clone()),
    );
    printer.insert(
        CONFIG_PRINTER_PORT_KEY.to_owned(),
        toml::Value::Integer(i64::from(settings.printer.port)),
    );

    layout.insert(
        CONFIG_FONT_SIZE_KEY.to_owned(),
        toml::Value::Integer(i64::from(settings.layout.font_size_px)),
    );
    layout.insert(
        CONFIG_PAPER_WIDTH_KEY.to_owned(),
        toml::Value::Integer(i64::from(settings.layout.paper_width_px)),
    );
    layout.insert(
        CONFIG_MARGIN_KEY.to_owned(),
        toml::Value::Integer(i64::from(settings.layout.margin_px)),
    );
    layout.insert(
        CONFIG_FONT_FACE_KEY.to_owned(),
        toml::Value::String(settings.layout.font_face_name.clone()),
    );

    root.insert(
        CONFIG_PRINTER_SECTION.to_owned(),
        toml::Value::Table(printer),
    );
    root.insert(CONFIG_LAYOUT_SECTION.to_owned(), toml::Value::Table(layout));
}

fn serialize_toml(root: toml::Table) -> Result<String, toml::ser::Error> {
    let mut contents = toml::to_string(&root)?;
    if !contents.ends_with('\n') {
        contents.push('\n');
    }
    Ok(contents)
}

#[cfg(test)]
fn parse_print_settings_toml(
    contents: &str,
    path: &Path,
) -> Result<PrintSettings, SettingsFileError> {
    Ok(parse_app_settings_toml(contents, path)?.print)
}

fn parse_app_settings_toml(contents: &str, path: &Path) -> Result<AppSettings, SettingsFileError> {
    let document = contents
        .parse::<toml::Table>()
        .map_err(|error| SettingsFileError::Parse {
            path: path.to_path_buf(),
            details: error.to_string(),
        })?;
    let defaults = AppSettings::default();

    let printer_table = optional_table(&document, CONFIG_PRINTER_SECTION, path)?;
    let layout_table = optional_table(&document, CONFIG_LAYOUT_SECTION, path)?;
    let ui_table = optional_table(&document, CONFIG_UI_SECTION, path)?;

    let settings = AppSettings {
        print: PrintSettings {
            printer: NetworkPrinterTarget {
                ip: optional_string(
                    printer_table,
                    CONFIG_PRINTER_SECTION,
                    CONFIG_PRINTER_IP_KEY,
                    path,
                )?
                .map(|ip| ip.trim().to_owned())
                .unwrap_or(defaults.print.printer.ip),
                port: optional_u16(
                    printer_table,
                    CONFIG_PRINTER_SECTION,
                    CONFIG_PRINTER_PORT_KEY,
                    path,
                )?
                .unwrap_or(defaults.print.printer.port),
            },
            layout: TextImageLayout {
                font_size_px: optional_u32(
                    layout_table,
                    CONFIG_LAYOUT_SECTION,
                    CONFIG_FONT_SIZE_KEY,
                    path,
                )?
                .unwrap_or(defaults.print.layout.font_size_px),
                paper_width_px: optional_u32(
                    layout_table,
                    CONFIG_LAYOUT_SECTION,
                    CONFIG_PAPER_WIDTH_KEY,
                    path,
                )?
                .unwrap_or(defaults.print.layout.paper_width_px),
                margin_px: optional_u32(
                    layout_table,
                    CONFIG_LAYOUT_SECTION,
                    CONFIG_MARGIN_KEY,
                    path,
                )?
                .unwrap_or(defaults.print.layout.margin_px),
                font_face_name: match optional_string(
                    layout_table,
                    CONFIG_LAYOUT_SECTION,
                    CONFIG_FONT_FACE_KEY,
                    path,
                )? {
                    Some(font_face_name) => Some(font_face_name),
                    None => legacy_font_face_name(layout_table, path)?,
                }
                .map(|font_face_name| font_face_name.trim().to_owned())
                .unwrap_or(defaults.print.layout.font_face_name),
            },
        },
        ui: UiSettings {
            theme: match optional_string(ui_table, CONFIG_UI_SECTION, CONFIG_UI_THEME_KEY, path)? {
                Some(theme) => ui_theme_from_config_value(&theme).ok_or_else(|| {
                    invalid_config_type(path, field_name(CONFIG_UI_SECTION, CONFIG_UI_THEME_KEY))
                })?,
                None => defaults.ui.theme,
            },
            language: match optional_string(
                ui_table,
                CONFIG_UI_SECTION,
                CONFIG_UI_LANGUAGE_KEY,
                path,
            )? {
                Some(language) => ui_language_from_config_value(&language).ok_or_else(|| {
                    invalid_config_type(path, field_name(CONFIG_UI_SECTION, CONFIG_UI_LANGUAGE_KEY))
                })?,
                None => defaults.ui.language,
            },
        },
    };

    settings
        .validate()
        .map_err(|error| SettingsFileError::InvalidSettings {
            path: path.to_path_buf(),
            details: error.to_string(),
        })?;

    Ok(settings)
}

const fn ui_theme_to_config_value(theme: UiTheme) -> &'static str {
    match theme {
        UiTheme::Light => CONFIG_UI_THEME_LIGHT,
        UiTheme::Dark => CONFIG_UI_THEME_DARK,
    }
}

fn ui_theme_from_config_value(value: &str) -> Option<UiTheme> {
    match value.trim().to_ascii_lowercase().as_str() {
        CONFIG_UI_THEME_LIGHT => Some(UiTheme::Light),
        CONFIG_UI_THEME_DARK => Some(UiTheme::Dark),
        _ => None,
    }
}

const fn ui_language_to_config_value(language: UiLanguage) -> &'static str {
    match language {
        UiLanguage::English => CONFIG_UI_LANGUAGE_ENGLISH,
        UiLanguage::Korean => CONFIG_UI_LANGUAGE_KOREAN,
    }
}

fn ui_language_from_config_value(value: &str) -> Option<UiLanguage> {
    match value.trim().to_ascii_lowercase().as_str() {
        CONFIG_UI_LANGUAGE_ENGLISH | "en" => Some(UiLanguage::English),
        CONFIG_UI_LANGUAGE_KOREAN | "ko" => Some(UiLanguage::Korean),
        _ => None,
    }
}

fn legacy_font_face_name(
    layout_table: Option<&toml::Table>,
    path: &Path,
) -> Result<Option<String>, SettingsFileError> {
    let Some(value) = optional_string(
        layout_table,
        CONFIG_LAYOUT_SECTION,
        CONFIG_LEGACY_FONT_FILE_KEY,
        path,
    )?
    else {
        return Ok(None);
    };

    let trimmed = value.trim();
    if looks_like_font_file_name(trimmed) {
        Ok(None)
    } else {
        Ok(Some(trimmed.to_owned()))
    }
}

fn looks_like_font_file_name(value: &str) -> bool {
    let extension = Path::new(value)
        .extension()
        .and_then(|extension| extension.to_str());
    matches!(
        extension,
        Some(extension)
            if extension.eq_ignore_ascii_case("ttf")
                || extension.eq_ignore_ascii_case("otf")
                || extension.eq_ignore_ascii_case("ttc")
    )
}

fn optional_table<'a>(
    document: &'a toml::Table,
    section: &'static str,
    path: &Path,
) -> Result<Option<&'a toml::Table>, SettingsFileError> {
    document
        .get(section)
        .map(|value| {
            value
                .as_table()
                .ok_or_else(|| invalid_config_type(path, section))
        })
        .transpose()
}

fn optional_string(
    table: Option<&toml::Table>,
    section: &'static str,
    key: &'static str,
    path: &Path,
) -> Result<Option<String>, SettingsFileError> {
    let Some(table) = table else {
        return Ok(None);
    };

    table
        .get(key)
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| invalid_config_type(path, field_name(section, key)))
        })
        .transpose()
}

fn optional_u16(
    table: Option<&toml::Table>,
    section: &'static str,
    key: &'static str,
    path: &Path,
) -> Result<Option<u16>, SettingsFileError> {
    optional_integer(table, section, key, path)?
        .map(|value| {
            u16::try_from(value).map_err(|_| invalid_config_type(path, field_name(section, key)))
        })
        .transpose()
}

fn optional_u32(
    table: Option<&toml::Table>,
    section: &'static str,
    key: &'static str,
    path: &Path,
) -> Result<Option<u32>, SettingsFileError> {
    optional_integer(table, section, key, path)?
        .map(|value| {
            u32::try_from(value).map_err(|_| invalid_config_type(path, field_name(section, key)))
        })
        .transpose()
}

fn optional_integer(
    table: Option<&toml::Table>,
    section: &'static str,
    key: &'static str,
    path: &Path,
) -> Result<Option<i64>, SettingsFileError> {
    let Some(table) = table else {
        return Ok(None);
    };

    table
        .get(key)
        .map(|value| {
            value
                .as_integer()
                .ok_or_else(|| invalid_config_type(path, field_name(section, key)))
        })
        .transpose()
}

fn field_name(section: &str, key: &str) -> String {
    format!("{section}.{key}")
}

fn invalid_config_type(path: &Path, field: impl Into<String>) -> SettingsFileError {
    SettingsFileError::InvalidValue {
        path: path.to_path_buf(),
        field: field.into(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsFileError {
    CurrentExecutablePath { details: String },
    ExecutableDirectoryUnavailable { executable_path: PathBuf },
    ExecutableFileNameUnavailable { executable_path: PathBuf },
    Read { path: PathBuf, details: String },
    Write { path: PathBuf, details: String },
    Serialize { path: PathBuf, details: String },
    Parse { path: PathBuf, details: String },
    InvalidValue { path: PathBuf, field: String },
    InvalidSettings { path: PathBuf, details: String },
}

impl fmt::Display for SettingsFileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CurrentExecutablePath { details } => {
                write!(
                    formatter,
                    "failed to resolve current executable path: {details}"
                )
            }
            Self::ExecutableDirectoryUnavailable { executable_path } => write!(
                formatter,
                "failed to resolve executable directory from {}",
                executable_path.display()
            ),
            Self::ExecutableFileNameUnavailable { executable_path } => write!(
                formatter,
                "failed to resolve executable file name from {}",
                executable_path.display()
            ),
            Self::Read { path, details } => {
                write!(
                    formatter,
                    "failed to read settings file {}: {details}",
                    path.display()
                )
            }
            Self::Write { path, details } => {
                write!(
                    formatter,
                    "failed to write settings file {}: {details}",
                    path.display()
                )
            }
            Self::Serialize { path, details } => write!(
                formatter,
                "failed to serialize settings file {}: {details}",
                path.display()
            ),
            Self::Parse { path, details } => {
                write!(
                    formatter,
                    "failed to parse settings file {}: {details}",
                    path.display()
                )
            }
            Self::InvalidValue { path, field } => write!(
                formatter,
                "settings file {} has an invalid value for {field}",
                path.display()
            ),
            Self::InvalidSettings { path, details } => write!(
                formatter,
                "settings file {} contains invalid print settings: {details}",
                path.display()
            ),
        }
    }
}

impl Error for SettingsFileError {}

#[derive(Debug, Default)]
pub struct Win32GdiTextImageRenderer;

impl TextImageRenderer for Win32GdiTextImageRenderer {
    fn render_text(&self, job: &PrintJob) -> Result<RenderedReceiptImage, InfraError> {
        #[cfg(target_os = "windows")]
        {
            crate::platform::gdi::render_receipt_text(job).map_err(InfraError::text_rendering)
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = job;
            Err(InfraError::UnsupportedPlatform("Win32/GDI text rendering"))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetworkEscPosPrinter;

impl NetworkEscPosPrinter {
    pub const fn new() -> Self {
        Self
    }
}

impl Default for NetworkEscPosPrinter {
    fn default() -> Self {
        Self::new()
    }
}

impl EscPosPrinter for NetworkEscPosPrinter {
    fn send_image_and_cut(
        &self,
        target: &NetworkPrinterTarget,
        image: &RenderedReceiptImage,
    ) -> Result<(), InfraError> {
        target
            .validate()
            .map_err(InfraError::InvalidPrinterTarget)?;

        let raster_commands =
            gs_v0_raster_bit_image_commands(image).map_err(InfraError::EscPosEncoding)?;
        let feed_command = esc_d_feed_command(ESC_POS_DEFAULT_CUT_FEED_LINES);
        let cut_command = gs_v_cut_command();

        let mut stream = connect_printer_stream(target)?;
        stream
            .set_write_timeout(Some(PRINTER_NETWORK_TIMEOUT))
            .map_err(|error| InfraError::network_io(target, "set write timeout", error))?;
        for raster_command in raster_commands {
            stream
                .write_all(&raster_command)
                .map_err(|error| InfraError::network_io(target, "write raster image", error))?;
        }
        stream
            .write_all(&feed_command)
            .map_err(|error| InfraError::network_io(target, "feed before cut", error))?;
        stream
            .write_all(&cut_command)
            .map_err(|error| InfraError::network_io(target, "write cut command", error))?;
        stream
            .flush()
            .map_err(|error| InfraError::network_io(target, "flush", error))?;

        Ok(())
    }
}

fn connect_printer_stream(target: &NetworkPrinterTarget) -> Result<TcpStream, InfraError> {
    let socket_addrs = printer_socket_addrs(target)?;
    let mut last_error = None;

    for socket_addr in socket_addrs {
        match TcpStream::connect_timeout(&socket_addr, PRINTER_NETWORK_TIMEOUT) {
            Ok(stream) => return Ok(stream),
            Err(error) => last_error = Some(error),
        }
    }

    let error = last_error.unwrap_or_else(|| {
        io::Error::new(
            io::ErrorKind::AddrNotAvailable,
            "printer host resolved to no socket addresses",
        )
    });
    Err(InfraError::network_io(target, "connect", error))
}

fn printer_socket_addrs(target: &NetworkPrinterTarget) -> Result<Vec<SocketAddr>, InfraError> {
    let host = target.ip.trim();
    let socket_addrs = (host, target.port)
        .to_socket_addrs()
        .map_err(|error| InfraError::network_io(target, "resolve address", error))?;
    let socket_addrs = socket_addrs.collect::<Vec<_>>();

    if socket_addrs.is_empty() {
        return Err(InfraError::network_io(
            target,
            "resolve address",
            io::Error::new(
                io::ErrorKind::AddrNotAvailable,
                "printer host resolved to no socket addresses",
            ),
        ));
    }

    Ok(socket_addrs)
}

pub fn gs_v0_raster_bit_image_command(
    image: &RenderedReceiptImage,
) -> Result<Vec<u8>, EscPosEncodingError> {
    let size = validate_raster_image(image)?;
    let height_px_u16 = u16::try_from(size.height_px).map_err(|_| {
        EscPosEncodingError::RasterDimensionsTooLarge {
            width_bytes: u32::from(size.width_bytes_u16),
            height_px: image.height_px,
        }
    })?;
    gs_v0_raster_bit_image_command_for_rows(image, &size, 0, usize::from(height_px_u16))
}

pub fn gs_v0_raster_bit_image_commands(
    image: &RenderedReceiptImage,
) -> Result<Vec<Vec<u8>>, EscPosEncodingError> {
    gs_v0_raster_bit_image_fragment_commands(image, ESC_POS_DEFAULT_IMAGE_FRAGMENT_HEIGHT_PX)
}

fn gs_v0_raster_bit_image_fragment_commands(
    image: &RenderedReceiptImage,
    max_fragment_height_px: u16,
) -> Result<Vec<Vec<u8>>, EscPosEncodingError> {
    let size = validate_raster_image(image)?;
    let fragment_height = usize::from(max_fragment_height_px);
    if fragment_height == 0 {
        return Err(EscPosEncodingError::InvalidImageDimensions {
            width_px: image.width_px,
            height_px: 0,
        });
    }

    let command_count = size.height_px.div_ceil(fragment_height);
    let mut commands = Vec::new();
    commands.try_reserve_exact(command_count).map_err(|_| {
        EscPosEncodingError::AllocationFailed {
            byte_len: command_count,
        }
    })?;

    let mut y_start_px = 0;
    while y_start_px < size.height_px {
        let fragment_height_px = (size.height_px - y_start_px).min(fragment_height);
        commands.push(gs_v0_raster_bit_image_command_for_rows(
            image,
            &size,
            y_start_px,
            fragment_height_px,
        )?);
        y_start_px += fragment_height_px;
    }

    Ok(commands)
}

fn gs_v0_raster_bit_image_command_for_rows(
    image: &RenderedReceiptImage,
    size: &RasterImageSize,
    y_start_px: usize,
    height_px: usize,
) -> Result<Vec<u8>, EscPosEncodingError> {
    let height_px_u16 =
        u16::try_from(height_px).map_err(|_| EscPosEncodingError::RasterDimensionsTooLarge {
            width_bytes: u32::from(size.width_bytes_u16),
            height_px: image.height_px,
        })?;
    let raster_data = pack_python_escpos_raster_bits(image, size, y_start_px, height_px)?;
    gs_v0_raster_bit_image_command_from_data(size.width_bytes_u16, height_px_u16, &raster_data)
}

fn gs_v0_raster_bit_image_command_from_data(
    width_bytes_u16: u16,
    height_px_u16: u16,
    raster_data: &[u8],
) -> Result<Vec<u8>, EscPosEncodingError> {
    let command_len = ESC_POS_RASTER_HEADER_LEN
        .checked_add(raster_data.len())
        .ok_or(EscPosEncodingError::AllocationFailed {
            byte_len: usize::MAX,
        })?;
    let mut command = Vec::new();
    command
        .try_reserve_exact(command_len)
        .map_err(|_| EscPosEncodingError::AllocationFailed {
            byte_len: command_len,
        })?;

    let width_bytes = width_bytes_u16.to_le_bytes();
    let height_px = height_px_u16.to_le_bytes();
    command.extend_from_slice(&[
        ESC_POS_GS,
        b'v',
        b'0',
        ESC_POS_RASTER_MODE_NORMAL,
        width_bytes[0],
        width_bytes[1],
        height_px[0],
        height_px[1],
    ]);
    command.extend_from_slice(raster_data);

    Ok(command)
}

pub const fn esc_d_feed_command(lines: u8) -> [u8; 3] {
    [ESC_POS_ESC, ESC_POS_PRINT_AND_FEED_COMMAND, lines]
}

pub const fn gs_v_cut_command() -> [u8; 3] {
    [ESC_POS_GS, ESC_POS_CUT_COMMAND, 0x00]
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EscPosEncodingError {
    InvalidImageDimensions {
        width_px: u32,
        height_px: u32,
    },
    RasterDimensionsTooLarge {
        width_bytes: u32,
        height_px: u32,
    },
    InvalidRgbBufferLength {
        width_px: u32,
        height_px: u32,
        expected_len: usize,
        actual_len: usize,
    },
    AllocationFailed {
        byte_len: usize,
    },
}

impl fmt::Display for EscPosEncodingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidImageDimensions {
                width_px,
                height_px,
            } => write!(
                formatter,
                "rendered image dimensions must be non-zero: {width_px}x{height_px}px"
            ),
            Self::RasterDimensionsTooLarge {
                width_bytes,
                height_px,
            } => write!(
                formatter,
                "rendered image is too large for ESC/POS raster header: {width_bytes} bytes wide, {height_px}px high"
            ),
            Self::InvalidRgbBufferLength {
                width_px,
                height_px,
                expected_len,
                actual_len,
            } => write!(
                formatter,
                "rendered image {width_px}x{height_px}px has invalid RGB buffer length: expected {expected_len} bytes, got {actual_len}"
            ),
            Self::AllocationFailed { byte_len } => write!(
                formatter,
                "failed to allocate ESC/POS raster command buffer of {byte_len} bytes"
            ),
        }
    }
}

impl Error for EscPosEncodingError {}

#[derive(Debug, Clone, Copy)]
struct RasterImageSize {
    width_px: usize,
    height_px: usize,
    width_bytes: usize,
    width_bytes_u16: u16,
}

fn validate_raster_image(
    image: &RenderedReceiptImage,
) -> Result<RasterImageSize, EscPosEncodingError> {
    if image.width_px == 0 || image.height_px == 0 {
        return Err(EscPosEncodingError::InvalidImageDimensions {
            width_px: image.width_px,
            height_px: image.height_px,
        });
    }

    let width_bytes_u32 = image.width_px / 8 + u32::from(!image.width_px.is_multiple_of(8));
    let width_bytes_u16 = u16::try_from(width_bytes_u32).map_err(|_| {
        EscPosEncodingError::RasterDimensionsTooLarge {
            width_bytes: width_bytes_u32,
            height_px: image.height_px,
        }
    })?;
    let expected_len = checked_rgb_buffer_len(image.width_px, image.height_px).ok_or(
        EscPosEncodingError::RasterDimensionsTooLarge {
            width_bytes: width_bytes_u32,
            height_px: image.height_px,
        },
    )?;
    if image.rgb_pixels.len() != expected_len {
        return Err(EscPosEncodingError::InvalidRgbBufferLength {
            width_px: image.width_px,
            height_px: image.height_px,
            expected_len,
            actual_len: image.rgb_pixels.len(),
        });
    }

    Ok(RasterImageSize {
        width_px: usize::try_from(image.width_px).map_err(|_| {
            EscPosEncodingError::RasterDimensionsTooLarge {
                width_bytes: width_bytes_u32,
                height_px: image.height_px,
            }
        })?,
        height_px: usize::try_from(image.height_px).map_err(|_| {
            EscPosEncodingError::RasterDimensionsTooLarge {
                width_bytes: width_bytes_u32,
                height_px: image.height_px,
            }
        })?,
        width_bytes: usize::from(width_bytes_u16),
        width_bytes_u16,
    })
}

fn pack_python_escpos_raster_bits(
    image: &RenderedReceiptImage,
    size: &RasterImageSize,
    y_start_px: usize,
    height_px: usize,
) -> Result<Vec<u8>, EscPosEncodingError> {
    let data_len =
        size.width_bytes
            .checked_mul(height_px)
            .ok_or(EscPosEncodingError::AllocationFailed {
                byte_len: usize::MAX,
            })?;
    let mut raster_data = Vec::new();
    raster_data
        .try_reserve_exact(data_len)
        .map_err(|_| EscPosEncodingError::AllocationFailed { byte_len: data_len })?;
    raster_data.resize(data_len, 0);

    let error_len = size
        .width_px
        .checked_add(1)
        .ok_or(EscPosEncodingError::AllocationFailed {
            byte_len: usize::MAX,
        })?;
    let mut errors = Vec::new();
    errors
        .try_reserve_exact(error_len)
        .map_err(|_| EscPosEncodingError::AllocationFailed {
            byte_len: error_len,
        })?;
    errors.resize(error_len, 0i32);

    // python-escpos delegates RGB images to Pillow as:
    // RGB -> L (Pillow luma) -> invert -> convert("1") with Floyd-Steinberg.
    // This mirrors Pillow's integer error diffusion so antialiased text edges produce
    // the same raster bits as the Python original.
    for y in 0..height_px {
        let mut l = 0i32;
        let mut l0 = 0i32;
        let mut l1 = 0i32;
        let source_y = y_start_px + y;

        for x in 0..size.width_px {
            let pixel_offset = (source_y * size.width_px + x) * BYTES_PER_RGB_PIXEL;
            let rgb = &image.rgb_pixels[pixel_offset..pixel_offset + BYTES_PER_RGB_PIXEL];
            let inverted_luma = 255 - i32::from(pillow_luma(rgb));

            l = clip_u8_i32(inverted_luma + (l + errors[x + 1]) / 16);
            let output = if l > PILLOW_BILEVEL_THRESHOLD { 255 } else { 0 };
            if output != 0 {
                let byte_offset = y * size.width_bytes + x / 8;
                raster_data[byte_offset] |= 0x80 >> (x % 8);
            }

            l -= output;
            let l2 = l;
            let d2 = l + l;
            l += d2;
            errors[x] = l + l0;
            l += d2;
            l0 = l + l1;
            l1 = l2;
            l += d2;
        }

        errors[size.width_px] = l0;
    }

    Ok(raster_data)
}

fn pillow_luma(rgb: &[u8]) -> u8 {
    let red = u32::from(rgb[0]);
    let green = u32::from(rgb[1]);
    let blue = u32::from(rgb[2]);

    ((red * PILLOW_LUMA_RED
        + green * PILLOW_LUMA_GREEN
        + blue * PILLOW_LUMA_BLUE
        + PILLOW_LUMA_ROUNDING)
        >> 16) as u8
}

fn clip_u8_i32(value: i32) -> i32 {
    value.clamp(0, 255)
}

fn checked_rgb_buffer_len(width_px: u32, height_px: u32) -> Option<usize> {
    let width = usize::try_from(width_px).ok()?;
    let height = usize::try_from(height_px).ok()?;

    width.checked_mul(height)?.checked_mul(BYTES_PER_RGB_PIXEL)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InfraError {
    UnsupportedPlatform(&'static str),
    InvalidPrinterTarget(DomainError),
    TextRendering {
        details: String,
    },
    EscPosEncoding(EscPosEncodingError),
    NetworkIo {
        target: String,
        operation: &'static str,
        details: String,
    },
}

impl InfraError {
    fn text_rendering(error: impl fmt::Display) -> Self {
        Self::TextRendering {
            details: error.to_string(),
        }
    }

    fn network_io(
        target: &NetworkPrinterTarget,
        operation: &'static str,
        error: impl fmt::Display,
    ) -> Self {
        Self::NetworkIo {
            target: target.to_string(),
            operation,
            details: error.to_string(),
        }
    }
}

impl fmt::Display for InfraError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPlatform(feature) => {
                write!(formatter, "{feature} is only available on Windows")
            }
            Self::InvalidPrinterTarget(error) => {
                write!(formatter, "invalid printer target: {error}")
            }
            Self::TextRendering { details } => {
                write!(formatter, "text-to-image rendering failed: {details}")
            }
            Self::EscPosEncoding(error) => {
                write!(formatter, "ESC/POS raster encoding failed: {error}")
            }
            Self::NetworkIo {
                target,
                operation,
                details,
            } => write!(
                formatter,
                "ESC/POS network {operation} failed for {target}: {details}"
            ),
        }
    }
}

impl Error for InfraError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidPrinterTarget(error) => Some(error),
            Self::EscPosEncoding(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        DEFAULT_FONT_FACE_NAME, DEFAULT_MARGIN_PX, DEFAULT_PAPER_WIDTH_PX, DEFAULT_PRINTER_PORT,
    };
    use std::{
        fs,
        io::Read,
        net::TcpListener,
        path::Path,
        thread,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn unique_temp_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("test clock should be after unix epoch")
            .as_nanos();

        env::temp_dir().join(format!(
            "j3ecs-netprint-test-{}-{nanos}",
            std::process::id()
        ))
    }

    fn white_image(width_px: u32, height_px: u32) -> RenderedReceiptImage {
        let pixel_count = usize::try_from(width_px)
            .expect("test width should fit usize")
            .checked_mul(usize::try_from(height_px).expect("test height should fit usize"))
            .expect("test image should be small");
        let rgb_len = pixel_count
            .checked_mul(BYTES_PER_RGB_PIXEL)
            .expect("test image should be small");

        RenderedReceiptImage {
            width_px,
            height_px,
            rgb_pixels: vec![255; rgb_len],
        }
    }

    fn set_pixel(image: &mut RenderedReceiptImage, x: u32, y: u32, rgb: [u8; 3]) {
        let width = usize::try_from(image.width_px).expect("test width should fit usize");
        let x = usize::try_from(x).expect("test x should fit usize");
        let y = usize::try_from(y).expect("test y should fit usize");
        let offset = (y * width + x) * BYTES_PER_RGB_PIXEL;

        image.rgb_pixels[offset..offset + BYTES_PER_RGB_PIXEL].copy_from_slice(&rgb);
    }

    #[test]
    fn settings_file_path_uses_executable_stem_next_to_executable() {
        let executable_path = Path::new("bin").join("j3ecs-netprint.exe");

        let settings_path =
            settings_file_path_for_executable(&executable_path).expect("path should resolve");

        assert_eq!(settings_path, Path::new("bin").join("j3ecs-netprint.toml"));
    }

    #[test]
    fn print_settings_toml_round_trips() {
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

        let contents =
            print_settings_to_toml(&settings).expect("settings should serialize as TOML");
        let parsed = parse_print_settings_toml(&contents, Path::new("settings.toml"))
            .expect("serialized settings should parse");

        assert_eq!(parsed, settings);
    }

    #[test]
    fn app_settings_toml_round_trips_ui_preferences() {
        let settings = AppSettings {
            print: PrintSettings {
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
            },
            ui: UiSettings {
                theme: UiTheme::Dark,
                language: UiLanguage::Korean,
            },
        };

        let contents = app_settings_to_toml(&settings).expect("settings should serialize as TOML");
        let parsed = parse_app_settings_toml(&contents, Path::new("settings.toml"))
            .expect("serialized settings should parse");

        assert_eq!(parsed, settings);
        assert!(contents.contains("[ui]"));
        assert!(contents.contains("theme = \"dark\""));
        assert!(contents.contains("language = \"korean\""));
    }

    #[test]
    fn load_or_create_print_settings_file_writes_defaults_when_missing() {
        let temp_dir = unique_temp_dir();
        fs::create_dir_all(&temp_dir).expect("test temp dir should be created");
        let settings_path = temp_dir.join("j3ecs-netprint.toml");

        let settings = load_or_create_print_settings_file(&settings_path)
            .expect("missing settings file should be created");
        let contents =
            fs::read_to_string(&settings_path).expect("created settings file should be readable");

        assert_eq!(settings, PrintSettings::default());
        assert!(contents.contains("[printer]"));
        assert!(contents.contains("[layout]"));
        assert!(contents.contains("[ui]"));
        assert!(contents.contains("theme = \"light\""));
        assert!(contents.contains("language = \"english\""));

        fs::remove_dir_all(&temp_dir).expect("test temp dir should be removed");
    }

    #[test]
    fn save_print_settings_file_replaces_existing_settings_and_removes_temp_file() {
        let temp_dir = unique_temp_dir();
        fs::create_dir_all(&temp_dir).expect("test temp dir should be created");
        let settings_path = temp_dir.join("j3ecs-netprint.toml");
        fs::write(&settings_path, "not toml").expect("existing settings file should be writable");
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

        save_print_settings_file(&settings_path, &settings)
            .expect("settings should be saved atomically");

        let contents =
            fs::read_to_string(&settings_path).expect("saved settings file should be readable");
        let parsed = parse_print_settings_toml(&contents, &settings_path)
            .expect("saved settings file should parse");
        let file_names = fs::read_dir(&temp_dir)
            .expect("test temp dir should be readable")
            .map(|entry| {
                entry
                    .expect("test temp dir entry should be readable")
                    .file_name()
            })
            .collect::<Vec<_>>();

        assert_eq!(parsed, settings);
        assert_eq!(file_names.len(), 1);
        assert_eq!(
            file_names[0],
            std::ffi::OsString::from("j3ecs-netprint.toml")
        );

        fs::remove_dir_all(&temp_dir).expect("test temp dir should be removed");
    }

    #[test]
    fn save_print_settings_file_preserves_existing_ui_preferences() {
        let temp_dir = unique_temp_dir();
        fs::create_dir_all(&temp_dir).expect("test temp dir should be created");
        let settings_path = temp_dir.join("j3ecs-netprint.toml");
        let original = AppSettings {
            ui: UiSettings {
                theme: UiTheme::Dark,
                language: UiLanguage::Korean,
            },
            ..AppSettings::default()
        };
        save_app_settings_file(&settings_path, &original)
            .expect("initial app settings should save");

        let print = PrintSettings {
            printer: NetworkPrinterTarget {
                ip: "127.0.0.1".to_owned(),
                port: 9101,
            },
            ..PrintSettings::default()
        };
        save_print_settings_file(&settings_path, &print)
            .expect("print settings should save while preserving UI settings");

        let contents =
            fs::read_to_string(&settings_path).expect("saved settings file should be readable");
        let parsed = parse_app_settings_toml(&contents, &settings_path)
            .expect("saved app settings should parse");

        assert_eq!(parsed.print, print);
        assert_eq!(parsed.ui.theme, UiTheme::Dark);
        assert_eq!(parsed.ui.language, UiLanguage::Korean);

        fs::remove_dir_all(&temp_dir).expect("test temp dir should be removed");
    }

    #[test]
    fn atomic_settings_write_removes_temp_file_when_replace_fails() {
        let temp_dir = unique_temp_dir();
        fs::create_dir_all(&temp_dir).expect("test temp dir should be created");
        let settings_path = temp_dir.join("j3ecs-netprint.toml");
        fs::create_dir(&settings_path).expect("destination directory should be created");
        let temp_path = temp_dir.join("j3ecs-netprint.toml.atomic-test.new");
        let temp_file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
            .expect("test temp file should be created");

        let error =
            write_settings_file_to_temp_path(&settings_path, &temp_path, temp_file, b"[printer]\n")
                .expect_err("replacing a directory with a file should fail");

        assert!(matches!(
            error,
            SettingsFileError::Write { path, details }
                if path == settings_path && details.contains("replace settings file")
        ));
        assert!(settings_path.is_dir());
        assert!(!temp_path.exists());

        fs::remove_dir_all(&temp_dir).expect("test temp dir should be removed");
    }

    #[test]
    fn parse_print_settings_toml_uses_defaults_for_missing_fields() {
        let settings = parse_print_settings_toml(
            r#"
[printer]
ip = "127.0.0.1"

[layout]
font_size_px = 36
"#,
            Path::new("settings.toml"),
        )
        .expect("partial settings should parse with defaults");

        assert_eq!(settings.printer.ip, "127.0.0.1");
        assert_eq!(settings.printer.port, DEFAULT_PRINTER_PORT);
        assert_eq!(settings.layout.font_size_px, 36);
        assert_eq!(settings.layout.paper_width_px, DEFAULT_PAPER_WIDTH_PX);
        assert_eq!(settings.layout.margin_px, DEFAULT_MARGIN_PX);
        assert_eq!(settings.layout.font_face_name, DEFAULT_FONT_FACE_NAME);
    }

    #[test]
    fn parse_app_settings_toml_uses_default_ui_preferences() {
        let settings = parse_app_settings_toml(
            r#"
[printer]
ip = "127.0.0.1"
"#,
            Path::new("settings.toml"),
        )
        .expect("partial app settings should parse with defaults");

        assert_eq!(settings.ui.theme, UiTheme::Light);
        assert_eq!(settings.ui.language, UiLanguage::English);
    }

    #[test]
    fn parse_app_settings_toml_rejects_unknown_ui_theme() {
        let error = parse_app_settings_toml(
            r#"
[ui]
theme = "sepia"
"#,
            Path::new("settings.toml"),
        )
        .expect_err("unknown UI theme should be rejected");

        assert!(matches!(
            error,
            SettingsFileError::InvalidValue { field, .. } if field == "ui.theme"
        ));
    }

    #[test]
    fn parse_app_settings_toml_rejects_unknown_ui_language() {
        let error = parse_app_settings_toml(
            r#"
[ui]
language = "pirate"
"#,
            Path::new("settings.toml"),
        )
        .expect_err("unknown UI language should be rejected");

        assert!(matches!(
            error,
            SettingsFileError::InvalidValue { field, .. } if field == "ui.language"
        ));
    }

    #[test]
    fn parse_print_settings_toml_trims_strings_used_for_io() {
        let settings = parse_print_settings_toml(
            r#"
[printer]
ip = "  127.0.0.1  "

[layout]
font_face_name = "  Arial  "
"#,
            Path::new("settings.toml"),
        )
        .expect("settings with surrounding whitespace should parse");

        assert_eq!(settings.printer.ip, "127.0.0.1");
        assert_eq!(settings.layout.font_face_name, "Arial");
    }

    #[test]
    fn parse_print_settings_toml_ignores_legacy_font_file_names() {
        let settings = parse_print_settings_toml(
            r#"
[layout]
font_file_name = "IM_Hyemin-Bold.ttf"
"#,
            Path::new("settings.toml"),
        )
        .expect("legacy font file setting should fall back to the default system font");

        assert_eq!(settings.layout.font_face_name, DEFAULT_FONT_FACE_NAME);
    }

    #[test]
    fn parse_print_settings_toml_accepts_legacy_font_face_names() {
        let settings = parse_print_settings_toml(
            r#"
[layout]
font_file_name = "Arial"
"#,
            Path::new("settings.toml"),
        )
        .expect("legacy face-name setting should migrate");

        assert_eq!(settings.layout.font_face_name, "Arial");
    }

    #[test]
    fn parse_print_settings_toml_rejects_invalid_field_type() {
        let error = parse_print_settings_toml(
            r#"
[printer]
port = "9100"
"#,
            Path::new("settings.toml"),
        )
        .expect_err("string port should be rejected");

        assert!(matches!(
            error,
            SettingsFileError::InvalidValue { field, .. } if field == "printer.port"
        ));
    }

    #[test]
    fn raster_header_uses_byte_rounded_width_and_little_endian_dimensions() {
        let image = white_image(10, 2);

        let command = gs_v0_raster_bit_image_command(&image).expect("valid image should encode");

        assert_eq!(
            &command[..ESC_POS_RASTER_HEADER_LEN],
            &[0x1D, b'v', b'0', 0x00, 0x02, 0x00, 0x02, 0x00]
        );
        assert_eq!(command.len(), ESC_POS_RASTER_HEADER_LEN + 4);
    }

    #[test]
    fn raster_bit_packing_is_msb_first_and_pads_partial_byte_as_white() {
        let mut image = white_image(10, 1);
        set_pixel(&mut image, 0, 0, [0, 0, 0]);
        set_pixel(&mut image, 2, 0, [0, 0, 0]);
        set_pixel(&mut image, 7, 0, [0, 0, 0]);
        set_pixel(&mut image, 8, 0, [0, 0, 0]);
        let size = validate_raster_image(&image).expect("valid image should validate");

        let packed = pack_python_escpos_raster_bits(&image, &size, 0, size.height_px)
            .expect("valid image should pack");

        assert_eq!(packed, vec![0b1010_0001, 0b1000_0000]);
    }

    #[test]
    fn raster_encoding_matches_python_escpos_pillow_dither_for_antialiasing_grays() {
        let mut image = white_image(8, 2);
        set_pixel(&mut image, 1, 0, [200, 200, 200]);
        set_pixel(&mut image, 2, 0, [128, 128, 128]);
        set_pixel(&mut image, 3, 0, [127, 127, 127]);
        set_pixel(&mut image, 4, 0, [55, 55, 55]);
        set_pixel(&mut image, 5, 0, [0, 0, 0]);
        set_pixel(&mut image, 7, 0, [0, 0, 0]);
        set_pixel(&mut image, 0, 1, [30, 30, 30]);
        set_pixel(&mut image, 1, 1, [90, 90, 90]);
        set_pixel(&mut image, 2, 1, [120, 120, 120]);
        set_pixel(&mut image, 3, 1, [128, 128, 128]);
        set_pixel(&mut image, 4, 1, [180, 180, 180]);
        set_pixel(&mut image, 5, 1, [220, 220, 220]);
        set_pixel(&mut image, 6, 1, [40, 40, 40]);
        let command = gs_v0_raster_bit_image_command(&image).expect("valid image should encode");

        assert_eq!(
            &command[..ESC_POS_RASTER_HEADER_LEN],
            &[0x1D, b'v', b'0', 0x00, 0x01, 0x00, 0x02, 0x00]
        );
        assert_eq!(
            &command[ESC_POS_RASTER_HEADER_LEN..],
            &[0b0010_1101, 0b1101_0010]
        );
    }

    #[test]
    fn gs_v_cut_command_uses_full_cut() {
        assert_eq!(gs_v_cut_command(), [0x1D, b'V', 0x00]);
    }

    #[test]
    fn esc_d_feed_command_uses_python_escpos_default_cut_feed() {
        assert_eq!(ESC_POS_DEFAULT_CUT_FEED_LINES, 6);
        assert_eq!(
            esc_d_feed_command(ESC_POS_DEFAULT_CUT_FEED_LINES),
            [0x1B, b'd', 0x06]
        );
    }

    #[test]
    fn raster_fragment_commands_use_python_escpos_default_fragment_height() {
        let mut image = white_image(8, u32::from(ESC_POS_DEFAULT_IMAGE_FRAGMENT_HEIGHT_PX) + 2);
        set_pixel(&mut image, 0, 0, [0, 0, 0]);
        set_pixel(
            &mut image,
            7,
            u32::from(ESC_POS_DEFAULT_IMAGE_FRAGMENT_HEIGHT_PX) - 1,
            [0, 0, 0],
        );
        set_pixel(
            &mut image,
            0,
            u32::from(ESC_POS_DEFAULT_IMAGE_FRAGMENT_HEIGHT_PX),
            [0, 0, 0],
        );
        set_pixel(
            &mut image,
            7,
            u32::from(ESC_POS_DEFAULT_IMAGE_FRAGMENT_HEIGHT_PX) + 1,
            [0, 0, 0],
        );

        let commands = gs_v0_raster_bit_image_commands(&image).expect("valid image should encode");

        assert_eq!(commands.len(), 2);
        assert_eq!(
            &commands[0][..ESC_POS_RASTER_HEADER_LEN],
            &[0x1D, b'v', b'0', 0x00, 0x01, 0x00, 0xC0, 0x03]
        );
        assert_eq!(
            commands[0].len(),
            ESC_POS_RASTER_HEADER_LEN + usize::from(ESC_POS_DEFAULT_IMAGE_FRAGMENT_HEIGHT_PX)
        );
        assert_eq!(commands[0][ESC_POS_RASTER_HEADER_LEN], 0b1000_0000);
        assert_eq!(
            commands[0][ESC_POS_RASTER_HEADER_LEN
                + usize::from(ESC_POS_DEFAULT_IMAGE_FRAGMENT_HEIGHT_PX)
                - 1],
            0b0000_0001
        );
        assert_eq!(
            &commands[1][..ESC_POS_RASTER_HEADER_LEN],
            &[0x1D, b'v', b'0', 0x00, 0x01, 0x00, 0x02, 0x00]
        );
        assert_eq!(
            &commands[1][ESC_POS_RASTER_HEADER_LEN..],
            &[0b1000_0000, 0b0000_0001]
        );
    }

    #[test]
    fn printer_socket_addrs_use_trimmed_printer_host() {
        let target = NetworkPrinterTarget {
            ip: "  127.0.0.1  ".to_owned(),
            port: 9100,
        };

        let socket_addrs =
            printer_socket_addrs(&target).expect("trimmed printer host should resolve");

        assert!(
            socket_addrs
                .iter()
                .any(|socket_addr| socket_addr.ip().to_string() == "127.0.0.1"
                    && socket_addr.port() == 9100)
        );
    }

    #[test]
    fn network_printer_accepts_hostname_like_python_escpos() {
        let listener =
            TcpListener::bind(("localhost", 0)).expect("test listener should bind to localhost");
        let server_addr = listener
            .local_addr()
            .expect("test listener address should be available");
        let server = thread::spawn(move || -> std::io::Result<Vec<u8>> {
            let (mut stream, _) = listener.accept()?;
            let mut captured = Vec::new();
            stream.read_to_end(&mut captured)?;
            Ok(captured)
        });

        let image = white_image(8, 1);
        let target = NetworkPrinterTarget {
            ip: " localhost ".to_owned(),
            port: server_addr.port(),
        };
        NetworkEscPosPrinter::new()
            .send_image_and_cut(&target, &image)
            .expect("hostname printer target should receive bytes");
        let captured = server
            .join()
            .expect("mock printer thread should not panic")
            .expect("mock printer should read transmitted bytes");

        assert_eq!(captured.len(), ESC_POS_RASTER_HEADER_LEN + 1 + 3 + 3);
        assert_eq!(
            &captured[..ESC_POS_RASTER_HEADER_LEN],
            &[0x1D, b'v', b'0', 0x00, 0x01, 0x00, 0x01, 0x00]
        );
    }

    #[test]
    fn network_printer_sends_raster_image_then_cut_to_tcp_stream() {
        let listener =
            TcpListener::bind(("127.0.0.1", 0)).expect("test listener should bind to localhost");
        let server_addr = listener
            .local_addr()
            .expect("test listener address should be available");
        let server = thread::spawn(move || -> std::io::Result<Vec<u8>> {
            let (mut stream, _) = listener.accept()?;
            let mut captured = Vec::new();
            stream.read_to_end(&mut captured)?;
            Ok(captured)
        });

        let mut image = white_image(9, 2);
        set_pixel(&mut image, 0, 0, [0, 0, 0]);
        set_pixel(&mut image, 8, 0, [0, 0, 0]);
        set_pixel(&mut image, 1, 1, [0, 0, 0]);
        set_pixel(&mut image, 7, 1, [0, 0, 0]);

        let target = NetworkPrinterTarget {
            ip: server_addr.ip().to_string(),
            port: server_addr.port(),
        };
        NetworkEscPosPrinter::new()
            .send_image_and_cut(&target, &image)
            .expect("valid local printer target should receive bytes");
        let captured = server
            .join()
            .expect("mock printer thread should not panic")
            .expect("mock printer should read transmitted bytes");

        assert_eq!(captured.len(), ESC_POS_RASTER_HEADER_LEN + 4 + 3 + 3);
        assert_eq!(
            &captured[..ESC_POS_RASTER_HEADER_LEN],
            &[0x1D, b'v', b'0', 0x00, 0x02, 0x00, 0x02, 0x00]
        );
        assert_eq!(
            &captured[ESC_POS_RASTER_HEADER_LEN..ESC_POS_RASTER_HEADER_LEN + 4],
            &[0b1000_0000, 0b1000_0000, 0b0100_0001, 0b0000_0000]
        );
        assert_eq!(
            &captured[ESC_POS_RASTER_HEADER_LEN + 4..ESC_POS_RASTER_HEADER_LEN + 7],
            &esc_d_feed_command(ESC_POS_DEFAULT_CUT_FEED_LINES)
        );
        assert_eq!(
            &captured[ESC_POS_RASTER_HEADER_LEN + 7..],
            &gs_v_cut_command()
        );
    }

    #[test]
    fn network_printer_sends_fragmented_raster_images_then_cut_to_tcp_stream() {
        let listener =
            TcpListener::bind(("127.0.0.1", 0)).expect("test listener should bind to localhost");
        let server_addr = listener
            .local_addr()
            .expect("test listener address should be available");
        let server = thread::spawn(move || -> std::io::Result<Vec<u8>> {
            let (mut stream, _) = listener.accept()?;
            let mut captured = Vec::new();
            stream.read_to_end(&mut captured)?;
            Ok(captured)
        });

        let mut image = white_image(8, u32::from(ESC_POS_DEFAULT_IMAGE_FRAGMENT_HEIGHT_PX) + 1);
        set_pixel(&mut image, 0, 0, [0, 0, 0]);
        set_pixel(
            &mut image,
            7,
            u32::from(ESC_POS_DEFAULT_IMAGE_FRAGMENT_HEIGHT_PX),
            [0, 0, 0],
        );

        let target = NetworkPrinterTarget {
            ip: server_addr.ip().to_string(),
            port: server_addr.port(),
        };
        NetworkEscPosPrinter::new()
            .send_image_and_cut(&target, &image)
            .expect("valid local printer target should receive bytes");
        let captured = server
            .join()
            .expect("mock printer thread should not panic")
            .expect("mock printer should read transmitted bytes");

        let first_len =
            ESC_POS_RASTER_HEADER_LEN + usize::from(ESC_POS_DEFAULT_IMAGE_FRAGMENT_HEIGHT_PX);
        let second_start = first_len;
        let second_len = ESC_POS_RASTER_HEADER_LEN + 1;
        assert_eq!(captured.len(), first_len + second_len + 3 + 3);
        assert_eq!(
            &captured[..ESC_POS_RASTER_HEADER_LEN],
            &[0x1D, b'v', b'0', 0x00, 0x01, 0x00, 0xC0, 0x03]
        );
        assert_eq!(captured[ESC_POS_RASTER_HEADER_LEN], 0b1000_0000);
        assert_eq!(
            &captured[second_start..second_start + ESC_POS_RASTER_HEADER_LEN],
            &[0x1D, b'v', b'0', 0x00, 0x01, 0x00, 0x01, 0x00]
        );
        assert_eq!(
            captured[second_start + ESC_POS_RASTER_HEADER_LEN],
            0b0000_0001
        );
        assert_eq!(
            &captured[second_start + second_len..second_start + second_len + 3],
            &esc_d_feed_command(ESC_POS_DEFAULT_CUT_FEED_LINES)
        );
        assert_eq!(
            &captured[second_start + second_len + 3..],
            &gs_v_cut_command()
        );
    }

    #[test]
    fn raster_encoding_rejects_invalid_rgb_buffer_length() {
        let image = RenderedReceiptImage {
            width_px: 1,
            height_px: 1,
            rgb_pixels: vec![0, 0],
        };

        assert_eq!(
            gs_v0_raster_bit_image_command(&image),
            Err(EscPosEncodingError::InvalidRgbBufferLength {
                width_px: 1,
                height_px: 1,
                expected_len: 3,
                actual_len: 2,
            })
        );
    }
}
