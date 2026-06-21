use std::{
    env,
    error::Error,
    fmt, fs,
    path::{Path, PathBuf},
    ptr::{null, null_mut},
    thread,
};

use windows_sys::Win32::{
    Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{
        ANTIALIASED_QUALITY, CLIP_DEFAULT_PRECIS, CLR_INVALID, COLOR_BTNFACE, COLOR_WINDOW,
        COLOR_WINDOWTEXT, CreateCompatibleDC, CreateFontW, CreateSolidBrush, DEFAULT_CHARSET,
        DEFAULT_GUI_FONT, DEFAULT_PITCH, DeleteDC, DeleteObject, EnumFontFamiliesExW, FF_DONTCARE,
        FW_NORMAL, FillRect, GetStockObject, GetSysColor, HBRUSH, HDC, HFONT, HGDIOBJ,
        InvalidateRect, LF_FACESIZE, LOGFONTW, OUT_DEFAULT_PRECIS, SetBkColor, SetTextColor,
        TEXTMETRICW, UpdateWindow,
    },
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Input::KeyboardAndMouse::EnableWindow,
        Shell::ShellExecuteW,
        WindowsAndMessaging::{
            BN_CLICKED, BS_DEFPUSHBUTTON, BS_GROUPBOX, CB_ADDSTRING, CB_ERR, CB_ERRSPACE,
            CB_FINDSTRINGEXACT, CB_GETCURSEL, CB_GETLBTEXT, CB_GETLBTEXTLEN, CB_RESETCONTENT,
            CB_SETCURSEL, CBN_SELCHANGE, CBS_DROPDOWNLIST, CBS_HASSTRINGS, CBS_SORT, CREATESTRUCTW,
            CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
            ES_AUTOHSCROLL, ES_AUTOVSCROLL, ES_LEFT, ES_MULTILINE, ES_NUMBER, ES_READONLY,
            ES_WANTRETURN, GWLP_USERDATA, GetClientRect, GetMessageW, GetSystemMetrics,
            GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW, HICON, HMENU, ICON_BIG,
            ICON_SMALL, IDC_ARROW, IMAGE_ICON, KillTimer, LR_DEFAULTSIZE, LR_SHARED, LoadCursorW,
            LoadImageW, MB_ICONERROR, MB_ICONINFORMATION, MB_ICONWARNING, MB_OK, MSG, MessageBoxW,
            MoveWindow, PostMessageW, PostQuitMessage, RegisterClassW, SM_CXICON, SM_CXSMICON,
            SM_CYICON, SM_CYSMICON, STN_CLICKED, SW_HIDE, SW_SHOW, SendMessageW, SetTimer,
            SetWindowLongPtrW, SetWindowTextW, ShowWindow, TranslateMessage, WM_COMMAND, WM_CREATE,
            WM_CTLCOLORBTN, WM_CTLCOLOREDIT, WM_CTLCOLORLISTBOX, WM_CTLCOLORSTATIC, WM_DESTROY,
            WM_ERASEBKGND, WM_SETFONT, WM_SETICON, WM_TIMER, WM_USER, WNDCLASSW, WS_BORDER,
            WS_CAPTION, WS_CHILD, WS_EX_CLIENTEDGE, WS_OVERLAPPEDWINDOW, WS_SYSMENU, WS_TABSTOP,
            WS_VISIBLE, WS_VSCROLL,
        },
    },
};

use crate::{
    app::{self, AppError, InputValidationError, PrintJobInput},
    domain::{AppSettings, DomainError, UiLanguage, UiSettings, UiTheme},
    infra::{InfraError, NetworkEscPosPrinter, Win32GdiTextImageRenderer},
};

mod ui_text;

use self::ui_text::{
    UiText, localized, ui_language_from_label, ui_language_label, ui_text, ui_theme_from_label,
    ui_theme_label,
};

const WINDOW_TITLE: &str = "ESC/POS Printer Text to Image";
const WINDOW_CLASS_NAME: &str = "J3EcsNetPrintWindow";
const ABOUT_WINDOW_CLASS_NAME: &str = "J3EcsNetPrintAboutWindow";
const APP_ICON_RESOURCE_ID: u16 = 1;
const WM_PRINT_COMPLETED: u32 = WM_USER + 1;
const WORKER_COMPLETION_TIMER_ID: usize = 1;
const WORKER_COMPLETION_POLL_MS: u32 = 250;

const ID_PRINT_BUTTON: i32 = 1001;
const ID_FONT_FACE_COMBO: i32 = 1002;
const ID_THEME_COMBO: i32 = 1003;
const ID_SETTINGS_TOGGLE_BUTTON: i32 = 1004;
const ID_LANGUAGE_COMBO: i32 = 1005;
const ID_GITHUB_LINK: i32 = 1006;
const ID_ABOUT_LINK: i32 = 1007;
const ID_ABOUT_OK_BUTTON: i32 = 2001;
const ID_ABOUT_PROJECT_LINK: i32 = 2002;

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const APP_NAME: &str = "j3Ecs NetPrint";
const PROJECT_LINK_LABEL: &str = "edgarp9/j3Ecs_NetPrint";
const PROJECT_LINK_URL: &str = "https://github.com/edgarp9/j3Ecs_NetPrint";
const ABOUT_TEXT_FILE: &str = "about.txt";
const DEFAULT_ABOUT_TEXT: &str = include_str!("../../about.txt");
const SHELL_EXECUTE_SUCCESS_MIN: isize = 33;
const STATIC_NOTIFY_STYLE: u32 = 0x0000_0100;

const WINDOW_WIDTH: i32 = 520;
const WINDOW_HEIGHT: i32 = 632;

const EDIT_HEIGHT: i32 = 24;
const LABEL_HEIGHT: i32 = 20;
const TEXT_EDIT_FONT_HEIGHT_PX: i32 = 16;
const TEXT_EDIT_FONT_FACE_NAME: &str = "Malgun Gothic";
const ABOUT_BODY_FONT_HEIGHT_PX: i32 = 14;
const ABOUT_BODY_FONT_FACE_NAME: &str = "Consolas";

const SETTINGS_GROUP_RECT: ControlRect = ControlRect {
    x: 16,
    y: 12,
    width: 472,
    height: 212,
};
const SETTINGS_TOGGLE_RECT: ControlRect = ControlRect {
    x: 370,
    y: 12,
    width: 104,
    height: EDIT_HEIGHT,
};
const TEXT_GROUP_EXPANDED_RECT: ControlRect = ControlRect {
    x: 16,
    y: 237,
    width: 472,
    height: 260,
};
const TEXT_EDIT_EXPANDED_RECT: ControlRect = ControlRect {
    x: 30,
    y: 262,
    width: 444,
    height: 220,
};
const TEXT_GROUP_COLLAPSED_RECT: ControlRect = ControlRect {
    x: 16,
    y: 48,
    width: 472,
    height: 377,
};
const TEXT_EDIT_COLLAPSED_RECT: ControlRect = ControlRect {
    x: 30,
    y: 73,
    width: 444,
    height: 337,
};
const PRINT_BUTTON_RECT: ControlRect = ControlRect {
    x: 16,
    y: 512,
    width: 472,
    height: 44,
};
const ABOUT_WINDOW_WIDTH: i32 = 620;
const ABOUT_WINDOW_HEIGHT: i32 = 430;
const ABOUT_VERSION_LABEL_RECT: ControlRect = ControlRect {
    x: 16,
    y: 16,
    width: 570,
    height: LABEL_HEIGHT,
};
const ABOUT_BODY_EDIT_RECT: ControlRect = ControlRect {
    x: 16,
    y: 48,
    width: 570,
    height: 298,
};
const ABOUT_PROJECT_LINK_RECT: ControlRect = ControlRect {
    x: 16,
    y: 366,
    width: 360,
    height: LABEL_HEIGHT,
};
const ABOUT_OK_BUTTON_RECT: ControlRect = ControlRect {
    x: 490,
    y: 362,
    width: 96,
    height: 28,
};

pub fn run() -> Result<(), GuiError> {
    let hinstance = module_handle()?;
    register_window_class(hinstance)?;
    register_about_window_class(hinstance)?;

    let class_name = wide_null(WINDOW_CLASS_NAME);
    let window_title = wide_null(WINDOW_TITLE);

    // SAFETY: class_name and window_title are null-terminated and live for the duration of the
    // call. hinstance is the current module handle, and all handles passed here are either null or
    // owned by the system.
    let hwnd = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            window_title.as_ptr(),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            WINDOW_WIDTH,
            WINDOW_HEIGHT,
            null_mut(),
            null_mut(),
            hinstance,
            null_mut(),
        )
    };

    if hwnd.is_null() {
        return Err(GuiError::Win32CallFailed("CreateWindowExW(main)"));
    }

    set_window_icons(hwnd, hinstance)?;

    // SAFETY: hwnd is a live top-level window returned by CreateWindowExW.
    unsafe {
        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);
    }

    message_loop()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuiError {
    Win32CallFailed(&'static str),
    WindowTextTooLong { control: &'static str },
    WorkerStartFailed { details: String },
    App(AppError),
}

impl fmt::Display for GuiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Win32CallFailed(function) => write!(formatter, "Win32 call failed: {function}"),
            Self::WindowTextTooLong { control } => {
                write!(formatter, "window text is too long: {control}")
            }
            Self::WorkerStartFailed { details } => {
                write!(formatter, "failed to start print worker: {details}")
            }
            Self::App(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for GuiError {}

#[derive(Debug, Clone, Copy)]
enum MessageIcon {
    Info,
    Warning,
    Error,
}

impl MessageIcon {
    const fn flags(self) -> u32 {
        match self {
            Self::Info => MB_OK | MB_ICONINFORMATION,
            Self::Warning => MB_OK | MB_ICONWARNING,
            Self::Error => MB_OK | MB_ICONERROR,
        }
    }
}

#[derive(Debug)]
struct UserMessage {
    title: String,
    body: String,
    icon: MessageIcon,
}

impl UserMessage {
    fn info(title: &str, body: &str) -> Self {
        Self {
            title: title.to_owned(),
            body: body.to_owned(),
            icon: MessageIcon::Info,
        }
    }

    fn warning(title: &str, body: &str) -> Self {
        Self {
            title: title.to_owned(),
            body: body.to_owned(),
            icon: MessageIcon::Warning,
        }
    }

    fn error(title: &str, body: impl Into<String>) -> Self {
        Self {
            title: title.to_owned(),
            body: body.into(),
            icon: MessageIcon::Error,
        }
    }
}

#[derive(Debug)]
struct AboutWindowContent {
    title: String,
    version_label: String,
    body_text: String,
    project_url: &'static str,
    ok_label: &'static str,
}

#[derive(Debug)]
struct AboutWindowCreateParams {
    theme: UiTheme,
    language: UiLanguage,
    content: *const AboutWindowContent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ControlRect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SettingsPanelLayout {
    text_group: ControlRect,
    text_edit: ControlRect,
}

#[derive(Debug)]
struct WorkerResult {
    message: UserMessage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrintWorkflowState {
    Idle,
    WorkerRunning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkerCompletionPostState {
    Posted,
    FallbackPending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkerCompletionPollAction {
    Wait,
    Complete,
    Stop,
}

impl PrintWorkflowState {
    const fn print_button_enabled(self) -> i32 {
        match self {
            Self::Idle => 1,
            Self::WorkerRunning => 0,
        }
    }

    fn print_button_label(self, text: &UiText) -> &'static str {
        match self {
            Self::Idle => text.print_button_label,
            Self::WorkerRunning => text.printing_button_label,
        }
    }

    const fn is_worker_running(self) -> bool {
        matches!(self, Self::WorkerRunning)
    }
}

const fn worker_completion_post_state(posted: i32) -> WorkerCompletionPostState {
    if posted == 0 {
        WorkerCompletionPostState::FallbackPending
    } else {
        WorkerCompletionPostState::Posted
    }
}

const fn worker_completion_poll_action(
    has_worker_handle: bool,
    worker_finished: bool,
) -> WorkerCompletionPollAction {
    match (has_worker_handle, worker_finished) {
        (true, true) => WorkerCompletionPollAction::Complete,
        (true, false) => WorkerCompletionPollAction::Wait,
        (false, _) => WorkerCompletionPollAction::Stop,
    }
}

#[derive(Debug)]
struct WindowState {
    controls: Controls,
    app_settings: AppSettings,
    settings_file_path: PathBuf,
    theme_resources: ThemeResources,
    settings_panel_visible: bool,
    workflow_state: PrintWorkflowState,
    worker_handle: Option<thread::JoinHandle<WorkerResult>>,
}

impl WindowState {
    fn create(parent: HWND, hinstance: HINSTANCE) -> Result<Self, GuiError> {
        let app_state = app::AppState::bootstrap().map_err(GuiError::App)?;
        let app_settings = app_state.app_settings().clone();
        let defaults = PrintJobInput::from_settings(&app_settings.print);
        let theme_resources = ThemeResources::create(app_settings.ui.theme)?;
        trace_gui(format!(
            "settings file: {}",
            app_state.settings_file_path().display()
        ));

        Ok(Self {
            controls: Controls::create(parent, hinstance, &defaults, &app_settings.ui)?,
            app_settings,
            settings_file_path: app_state.settings_file_path().to_path_buf(),
            theme_resources,
            settings_panel_visible: true,
            workflow_state: PrintWorkflowState::Idle,
            worker_handle: None,
        })
    }

    fn collect_input(&self) -> Result<PrintJobInput, GuiError> {
        Ok(PrintJobInput {
            printer_ip: window_text(self.controls.ip_edit, "IP/호스트")?,
            printer_port: window_text(self.controls.port_edit, "Port")?,
            font_size_px: window_text(self.controls.font_size_edit, "폰트 크기")?,
            paper_width_px: window_text(self.controls.paper_width_edit, "용지 폭")?,
            font_face_name: combo_box_selected_text(self.controls.font_face_combo, "폰트")?,
            text: window_text(self.controls.text_edit, "출력할 내용")?,
        })
    }

    fn set_workflow_state(&mut self, workflow_state: PrintWorkflowState) {
        if self.workflow_state == workflow_state {
            return;
        }

        trace_gui(format!(
            "workflow state: {:?} -> {:?}",
            self.workflow_state, workflow_state
        ));
        self.workflow_state = workflow_state;

        // SAFETY: print_button is a child HWND created with this state and label is encoded as a
        // valid null-terminated UTF-16 string for the duration of the calls.
        unsafe {
            EnableWindow(
                self.controls.print_button,
                self.workflow_state.print_button_enabled(),
            );
        }
        let text = ui_text(self.app_settings.ui.language);
        let _ = set_window_text(
            self.controls.print_button,
            self.workflow_state.print_button_label(text),
        );
    }

    fn save_print_settings(
        &mut self,
        settings: &crate::domain::PrintSettings,
    ) -> Result<(), AppError> {
        self.app_settings.print = settings.clone();
        app::save_app_settings(&self.settings_file_path, &self.app_settings)
    }

    fn set_ui_theme(&mut self, theme: UiTheme) -> Result<(), GuiError> {
        if self.app_settings.ui.theme == theme {
            return Ok(());
        }

        trace_gui(format!(
            "UI theme: {:?} -> {:?}",
            self.app_settings.ui.theme, theme
        ));
        self.theme_resources = ThemeResources::create(theme)?;
        self.app_settings.ui.theme = theme;
        app::save_app_settings(&self.settings_file_path, &self.app_settings).map_err(GuiError::App)
    }

    fn set_ui_language(&mut self, language: UiLanguage) -> Result<(), GuiError> {
        if self.app_settings.ui.language == language {
            return Ok(());
        }

        trace_gui(format!(
            "UI language: {:?} -> {:?}",
            self.app_settings.ui.language, language
        ));
        self.app_settings.ui.language = language;
        self.controls.apply_language(
            language,
            self.app_settings.ui.theme,
            self.settings_panel_visible,
            self.workflow_state,
        )?;
        app::save_app_settings(&self.settings_file_path, &self.app_settings).map_err(GuiError::App)
    }

    fn toggle_settings_panel(&mut self) -> Result<(), GuiError> {
        let next_visible = toggled_settings_panel_visibility(self.settings_panel_visible);
        trace_gui(format!(
            "settings panel visible: {} -> {}",
            self.settings_panel_visible, next_visible
        ));
        self.controls
            .set_settings_panel_visible(next_visible, self.app_settings.ui.language)?;
        self.settings_panel_visible = next_visible;
        Ok(())
    }

    fn wait_for_worker_before_destroy(&mut self) {
        let Some(worker_handle) = self.worker_handle.take() else {
            return;
        };

        trace_gui("waiting for print worker before destroying window");
        let _ = join_worker_result(worker_handle, self.app_settings.ui.language);
    }
}

#[derive(Debug)]
struct Controls {
    setting_group: HWND,
    ip_label: HWND,
    ip_edit: HWND,
    port_label: HWND,
    port_edit: HWND,
    font_size_label: HWND,
    font_size_edit: HWND,
    paper_width_label: HWND,
    paper_width_edit: HWND,
    font_face_label: HWND,
    font_face_combo: HWND,
    theme_label: HWND,
    theme_combo: HWND,
    language_label: HWND,
    language_combo: HWND,
    version_label: HWND,
    github_link: HWND,
    about_link: HWND,
    text_group: HWND,
    text_edit: HWND,
    settings_toggle_button: HWND,
    print_button: HWND,
    _text_edit_font: OwnedGuiFont,
    settings_panel_windows: Vec<HWND>,
    theme_windows: Vec<HWND>,
}

impl Controls {
    fn create(
        parent: HWND,
        hinstance: HINSTANCE,
        defaults: &PrintJobInput,
        ui_settings: &UiSettings,
    ) -> Result<Self, GuiError> {
        let gui_font = default_gui_font();
        let text_edit_font =
            OwnedGuiFont::create(TEXT_EDIT_FONT_FACE_NAME, TEXT_EDIT_FONT_HEIGHT_PX)?;
        let text = ui_text(ui_settings.language);

        let setting_group = create_child(
            hinstance,
            parent,
            "BUTTON",
            text.settings_group_label,
            WS_CHILD | WS_VISIBLE | (BS_GROUPBOX as u32),
            SETTINGS_GROUP_RECT.x,
            SETTINGS_GROUP_RECT.y,
            SETTINGS_GROUP_RECT.width,
            SETTINGS_GROUP_RECT.height,
            0,
            0,
        )?;
        let settings_toggle_button = create_child(
            hinstance,
            parent,
            "BUTTON",
            text.settings_hide_button_label,
            WS_CHILD | WS_VISIBLE | WS_TABSTOP,
            SETTINGS_TOGGLE_RECT.x,
            SETTINGS_TOGGLE_RECT.y,
            SETTINGS_TOGGLE_RECT.width,
            SETTINGS_TOGGLE_RECT.height,
            ID_SETTINGS_TOGGLE_BUTTON,
            0,
        )?;

        let ip_label = create_static(hinstance, parent, text.ip_label, 30, 40, 65, LABEL_HEIGHT)?;
        let ip_edit = create_edit(
            hinstance,
            parent,
            &defaults.printer_ip,
            100,
            38,
            150,
            EDIT_HEIGHT,
            ES_LEFT | ES_AUTOHSCROLL,
            0,
        )?;

        let port_label = create_static(
            hinstance,
            parent,
            text.port_label,
            270,
            40,
            42,
            LABEL_HEIGHT,
        )?;
        let port_edit = create_edit(
            hinstance,
            parent,
            &defaults.printer_port,
            315,
            38,
            70,
            EDIT_HEIGHT,
            ES_LEFT | ES_AUTOHSCROLL | ES_NUMBER,
            0,
        )?;

        let font_size_label = create_static(
            hinstance,
            parent,
            text.font_size_label,
            30,
            76,
            70,
            LABEL_HEIGHT,
        )?;
        let font_size_edit = create_edit(
            hinstance,
            parent,
            &defaults.font_size_px,
            105,
            74,
            58,
            EDIT_HEIGHT,
            ES_LEFT | ES_AUTOHSCROLL | ES_NUMBER,
            0,
        )?;
        let font_size_unit = create_static(hinstance, parent, "px", 170, 76, 28, LABEL_HEIGHT)?;

        let paper_width_label = create_static(
            hinstance,
            parent,
            text.paper_width_label,
            220,
            76,
            80,
            LABEL_HEIGHT,
        )?;
        let paper_width_edit = create_edit(
            hinstance,
            parent,
            &defaults.paper_width_px,
            305,
            74,
            70,
            EDIT_HEIGHT,
            ES_LEFT | ES_AUTOHSCROLL | ES_NUMBER,
            0,
        )?;
        let paper_width_unit = create_static(hinstance, parent, "px", 382, 76, 28, LABEL_HEIGHT)?;

        let font_face_label = create_static(
            hinstance,
            parent,
            text.font_face_label,
            30,
            112,
            70,
            LABEL_HEIGHT,
        )?;
        let font_face_combo =
            create_sorted_combo_box(hinstance, parent, 105, 108, 220, 180, ID_FONT_FACE_COMBO)?;
        populate_font_face_combo(font_face_combo, &defaults.font_face_name)?;

        let theme_label = create_static(
            hinstance,
            parent,
            text.theme_label,
            340,
            112,
            50,
            LABEL_HEIGHT,
        )?;
        let theme_combo = create_combo_box(hinstance, parent, 390, 108, 84, 120, ID_THEME_COMBO)?;
        populate_theme_combo(theme_combo, ui_settings.theme, ui_settings.language)?;

        let language_label = create_static(
            hinstance,
            parent,
            text.language_label,
            30,
            148,
            70,
            LABEL_HEIGHT,
        )?;
        let language_combo =
            create_combo_box(hinstance, parent, 105, 144, 120, 120, ID_LANGUAGE_COMBO)?;
        populate_language_combo(language_combo, ui_settings.language, ui_settings.language)?;

        let version_label = create_static(
            hinstance,
            parent,
            &program_version_text(text),
            30,
            184,
            150,
            LABEL_HEIGHT,
        )?;
        let github_link = create_clickable_static(
            hinstance,
            parent,
            PROJECT_LINK_LABEL,
            ControlRect {
                x: 205,
                y: 184,
                width: 230,
                height: LABEL_HEIGHT,
            },
            ID_GITHUB_LINK,
        )?;
        let about_link = create_clickable_static(
            hinstance,
            parent,
            text.about_link_label,
            ControlRect {
                x: 30,
                y: 204,
                width: 120,
                height: LABEL_HEIGHT,
            },
            ID_ABOUT_LINK,
        )?;

        let text_group = create_child(
            hinstance,
            parent,
            "BUTTON",
            text.text_group_label,
            WS_CHILD | WS_VISIBLE | (BS_GROUPBOX as u32),
            TEXT_GROUP_EXPANDED_RECT.x,
            TEXT_GROUP_EXPANDED_RECT.y,
            TEXT_GROUP_EXPANDED_RECT.width,
            TEXT_GROUP_EXPANDED_RECT.height,
            0,
            0,
        )?;

        let text_edit = create_edit(
            hinstance,
            parent,
            "",
            TEXT_EDIT_EXPANDED_RECT.x,
            TEXT_EDIT_EXPANDED_RECT.y,
            TEXT_EDIT_EXPANDED_RECT.width,
            TEXT_EDIT_EXPANDED_RECT.height,
            ES_LEFT | ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN,
            WS_VSCROLL,
        )?;

        let print_button = create_child(
            hinstance,
            parent,
            "BUTTON",
            text.print_button_label,
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | (BS_DEFPUSHBUTTON as u32),
            PRINT_BUTTON_RECT.x,
            PRINT_BUTTON_RECT.y,
            PRINT_BUTTON_RECT.width,
            PRINT_BUTTON_RECT.height,
            ID_PRINT_BUTTON,
            0,
        )?;

        let theme_windows = vec![
            setting_group,
            ip_label,
            ip_edit,
            port_label,
            port_edit,
            font_size_label,
            font_size_edit,
            font_size_unit,
            paper_width_label,
            paper_width_edit,
            paper_width_unit,
            font_face_label,
            font_face_combo,
            theme_label,
            theme_combo,
            language_label,
            language_combo,
            version_label,
            github_link,
            about_link,
            settings_toggle_button,
            text_group,
            text_edit,
            print_button,
        ];
        for hwnd in &theme_windows {
            apply_font(*hwnd, gui_font);
        }
        apply_font(text_edit, text_edit_font.handle() as HGDIOBJ);

        Ok(Self {
            setting_group,
            ip_label,
            ip_edit,
            port_label,
            port_edit,
            font_size_label,
            font_size_edit,
            paper_width_label,
            paper_width_edit,
            font_face_label,
            font_face_combo,
            theme_label,
            theme_combo,
            language_label,
            language_combo,
            version_label,
            github_link,
            about_link,
            text_group,
            text_edit,
            settings_toggle_button,
            print_button,
            _text_edit_font: text_edit_font,
            settings_panel_windows: vec![
                setting_group,
                ip_label,
                ip_edit,
                port_label,
                port_edit,
                font_size_label,
                font_size_edit,
                font_size_unit,
                paper_width_label,
                paper_width_edit,
                paper_width_unit,
                font_face_label,
                font_face_combo,
                theme_label,
                theme_combo,
                language_label,
                language_combo,
                version_label,
                github_link,
                about_link,
            ],
            theme_windows,
        })
    }

    fn set_settings_panel_visible(
        &self,
        visible: bool,
        language: UiLanguage,
    ) -> Result<(), GuiError> {
        let text = ui_text(language);
        set_window_text(
            self.settings_toggle_button,
            settings_toggle_button_label(visible, text),
        )?;
        for hwnd in &self.settings_panel_windows {
            show_child_window(*hwnd, visible);
        }
        self.apply_text_area_layout(settings_panel_layout(visible))?;
        self.invalidate();
        Ok(())
    }

    fn apply_language(
        &self,
        language: UiLanguage,
        selected_theme: UiTheme,
        settings_panel_visible: bool,
        workflow_state: PrintWorkflowState,
    ) -> Result<(), GuiError> {
        let text = ui_text(language);

        set_window_text(self.setting_group, text.settings_group_label)?;
        set_window_text(self.ip_label, text.ip_label)?;
        set_window_text(self.port_label, text.port_label)?;
        set_window_text(self.font_size_label, text.font_size_label)?;
        set_window_text(self.paper_width_label, text.paper_width_label)?;
        set_window_text(self.font_face_label, text.font_face_label)?;
        set_window_text(self.theme_label, text.theme_label)?;
        set_window_text(self.language_label, text.language_label)?;
        set_window_text(self.version_label, &program_version_text(text))?;
        set_window_text(self.github_link, PROJECT_LINK_LABEL)?;
        set_window_text(self.about_link, text.about_link_label)?;
        set_window_text(self.text_group, text.text_group_label)?;
        set_window_text(
            self.settings_toggle_button,
            settings_toggle_button_label(settings_panel_visible, text),
        )?;
        set_window_text(self.print_button, workflow_state.print_button_label(text))?;

        reset_combo_box(self.theme_combo)?;
        populate_theme_combo(self.theme_combo, selected_theme, language)?;
        reset_combo_box(self.language_combo)?;
        populate_language_combo(self.language_combo, language, language)?;

        self.invalidate();
        Ok(())
    }

    fn apply_text_area_layout(&self, layout: SettingsPanelLayout) -> Result<(), GuiError> {
        move_child_window(self.text_group, layout.text_group)?;
        move_child_window(self.text_edit, layout.text_edit)
    }

    fn invalidate(&self) {
        for hwnd in &self.theme_windows {
            invalidate_window(*hwnd);
        }
    }
}

#[derive(Debug)]
struct AboutWindowState {
    controls: AboutWindowControls,
    language: UiLanguage,
    theme_resources: ThemeResources,
}

impl AboutWindowState {
    fn create(
        parent: HWND,
        hinstance: HINSTANCE,
        theme: UiTheme,
        language: UiLanguage,
        content: &AboutWindowContent,
    ) -> Result<Self, GuiError> {
        let theme_resources = ThemeResources::create(theme)?;
        Ok(Self {
            controls: AboutWindowControls::create(parent, hinstance, content)?,
            language,
            theme_resources,
        })
    }
}

#[derive(Debug)]
struct AboutWindowControls {
    project_link: HWND,
    _body_edit_font: OwnedGuiFont,
    theme_windows: Vec<HWND>,
}

impl AboutWindowControls {
    fn create(
        parent: HWND,
        hinstance: HINSTANCE,
        content: &AboutWindowContent,
    ) -> Result<Self, GuiError> {
        let gui_font = default_gui_font();
        let body_edit_font =
            OwnedGuiFont::create(ABOUT_BODY_FONT_FACE_NAME, ABOUT_BODY_FONT_HEIGHT_PX)?;
        let version_label = create_static(
            hinstance,
            parent,
            &content.version_label,
            ABOUT_VERSION_LABEL_RECT.x,
            ABOUT_VERSION_LABEL_RECT.y,
            ABOUT_VERSION_LABEL_RECT.width,
            ABOUT_VERSION_LABEL_RECT.height,
        )?;
        let body_edit_text = win32_edit_multiline_text(&content.body_text);
        let body_edit = create_edit(
            hinstance,
            parent,
            &body_edit_text,
            ABOUT_BODY_EDIT_RECT.x,
            ABOUT_BODY_EDIT_RECT.y,
            ABOUT_BODY_EDIT_RECT.width,
            ABOUT_BODY_EDIT_RECT.height,
            ES_LEFT | ES_MULTILINE | ES_AUTOVSCROLL | ES_READONLY,
            WS_VSCROLL,
        )?;
        let project_link = create_clickable_static(
            hinstance,
            parent,
            content.project_url,
            ABOUT_PROJECT_LINK_RECT,
            ID_ABOUT_PROJECT_LINK,
        )?;
        let ok_button = create_child(
            hinstance,
            parent,
            "BUTTON",
            content.ok_label,
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | (BS_DEFPUSHBUTTON as u32),
            ABOUT_OK_BUTTON_RECT.x,
            ABOUT_OK_BUTTON_RECT.y,
            ABOUT_OK_BUTTON_RECT.width,
            ABOUT_OK_BUTTON_RECT.height,
            ID_ABOUT_OK_BUTTON,
            0,
        )?;

        let theme_windows = vec![version_label, body_edit, project_link, ok_button];
        for hwnd in &theme_windows {
            apply_font(*hwnd, gui_font);
        }
        apply_font(body_edit, body_edit_font.handle() as HGDIOBJ);

        Ok(Self {
            project_link,
            _body_edit_font: body_edit_font,
            theme_windows,
        })
    }

    fn invalidate(&self) {
        for hwnd in &self.theme_windows {
            invalidate_window(*hwnd);
        }
    }
}

#[derive(Debug)]
struct ThemeResources {
    theme: UiTheme,
    background_brush: Option<HBRUSH>,
    field_brush: Option<HBRUSH>,
    button_brush: Option<HBRUSH>,
}

impl ThemeResources {
    fn create(theme: UiTheme) -> Result<Self, GuiError> {
        match theme {
            UiTheme::Light => Ok(Self {
                theme,
                background_brush: None,
                field_brush: None,
                button_brush: None,
            }),
            UiTheme::Dark => Self::create_dark(),
        }
    }

    fn create_dark() -> Result<Self, GuiError> {
        let background_brush = create_solid_brush(DARK_BACKGROUND_COLOR)?;
        let field_brush = match create_solid_brush(DARK_FIELD_BACKGROUND_COLOR) {
            Ok(brush) => brush,
            Err(error) => {
                delete_owned_brush(background_brush);
                return Err(error);
            }
        };
        let button_brush = match create_solid_brush(DARK_BUTTON_BACKGROUND_COLOR) {
            Ok(brush) => brush,
            Err(error) => {
                delete_owned_brush(background_brush);
                delete_owned_brush(field_brush);
                return Err(error);
            }
        };

        Ok(Self {
            theme: UiTheme::Dark,
            background_brush: Some(background_brush),
            field_brush: Some(field_brush),
            button_brush: Some(button_brush),
        })
    }

    fn background_brush(&self) -> HBRUSH {
        self.background_brush
            .unwrap_or_else(|| system_color_brush(COLOR_WINDOW))
    }

    fn field_brush(&self) -> HBRUSH {
        self.field_brush
            .unwrap_or_else(|| system_color_brush(COLOR_WINDOW))
    }

    fn button_brush(&self) -> HBRUSH {
        self.button_brush
            .unwrap_or_else(|| system_color_brush(COLOR_BTNFACE))
    }

    fn text_color(&self) -> u32 {
        match self.theme {
            UiTheme::Light => system_color(COLOR_WINDOWTEXT),
            UiTheme::Dark => DARK_TEXT_COLOR,
        }
    }

    fn background_color(&self) -> u32 {
        match self.theme {
            UiTheme::Light => system_color(COLOR_WINDOW),
            UiTheme::Dark => DARK_BACKGROUND_COLOR,
        }
    }

    fn field_background_color(&self) -> u32 {
        match self.theme {
            UiTheme::Light => system_color(COLOR_WINDOW),
            UiTheme::Dark => DARK_FIELD_BACKGROUND_COLOR,
        }
    }

    fn button_background_color(&self) -> u32 {
        match self.theme {
            UiTheme::Light => system_color(COLOR_BTNFACE),
            UiTheme::Dark => DARK_BUTTON_BACKGROUND_COLOR,
        }
    }

    fn apply_background_colors(&self, hdc: HDC) {
        set_dc_colors(hdc, self.text_color(), self.background_color());
    }

    fn apply_field_colors(&self, hdc: HDC) {
        set_dc_colors(hdc, self.text_color(), self.field_background_color());
    }

    fn apply_button_colors(&self, hdc: HDC) {
        set_dc_colors(hdc, self.text_color(), self.button_background_color());
    }

    fn apply_link_colors(&self, hdc: HDC) {
        let link_color = match self.theme {
            UiTheme::Light => LIGHT_LINK_TEXT_COLOR,
            UiTheme::Dark => DARK_LINK_TEXT_COLOR,
        };
        set_dc_colors(hdc, link_color, self.background_color());
    }
}

impl Drop for ThemeResources {
    fn drop(&mut self) {
        for brush in [
            self.background_brush.take(),
            self.field_brush.take(),
            self.button_brush.take(),
        ]
        .into_iter()
        .flatten()
        {
            delete_owned_brush(brush);
        }
    }
}

const DARK_BACKGROUND_COLOR: u32 = rgb(32, 33, 36);
const DARK_FIELD_BACKGROUND_COLOR: u32 = rgb(43, 45, 49);
const DARK_BUTTON_BACKGROUND_COLOR: u32 = rgb(55, 58, 64);
const DARK_TEXT_COLOR: u32 = rgb(241, 243, 244);
const LIGHT_LINK_TEXT_COLOR: u32 = rgb(0, 102, 204);
const DARK_LINK_TEXT_COLOR: u32 = rgb(138, 180, 248);

const fn rgb(red: u8, green: u8, blue: u8) -> u32 {
    (blue as u32) << 16 | (green as u32) << 8 | red as u32
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_CREATE => handle_create(hwnd, lparam),
        WM_COMMAND => {
            let control_id = loword(wparam);
            let notification = hiword(wparam);
            if control_id == ID_PRINT_BUTTON && u32::from(notification) == BN_CLICKED {
                handle_print_clicked(hwnd);
                return 0;
            }
            if control_id == ID_SETTINGS_TOGGLE_BUTTON && u32::from(notification) == BN_CLICKED {
                handle_settings_toggle_clicked(hwnd);
                return 0;
            }
            if control_id == ID_THEME_COMBO && u32::from(notification) == CBN_SELCHANGE {
                handle_theme_changed(hwnd);
                return 0;
            }
            if control_id == ID_LANGUAGE_COMBO && u32::from(notification) == CBN_SELCHANGE {
                handle_language_changed(hwnd);
                return 0;
            }
            if control_id == ID_GITHUB_LINK && u32::from(notification) == STN_CLICKED {
                handle_github_link_clicked(hwnd);
                return 0;
            }
            if control_id == ID_ABOUT_LINK && u32::from(notification) == STN_CLICKED {
                handle_about_link_clicked(hwnd);
                return 0;
            }

            // SAFETY: unhandled messages are delegated to the system default window procedure.
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_ERASEBKGND => {
            if fill_window_background(hwnd, wparam as HDC) {
                return 1;
            }

            // SAFETY: unhandled background erase messages use the system default window procedure.
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_CTLCOLOREDIT | WM_CTLCOLORSTATIC | WM_CTLCOLORBTN | WM_CTLCOLORLISTBOX => {
            if let Some(brush) = control_color_brush(hwnd, message, wparam as HDC, lparam as HWND) {
                return brush as LRESULT;
            }

            // SAFETY: control color messages without window state use the system default handling.
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_PRINT_COMPLETED => {
            handle_print_completed(hwnd);
            0
        }
        WM_TIMER => {
            if wparam == WORKER_COMPLETION_TIMER_ID {
                handle_worker_completion_poll(hwnd);
                return 0;
            }

            // SAFETY: unhandled timer messages are delegated to the system default window procedure.
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_DESTROY => {
            drop_window_state(hwnd);
            // SAFETY: posts a standard quit message for the current GUI thread.
            unsafe {
                PostQuitMessage(0);
            }
            0
        }
        _ => {
            // SAFETY: unhandled messages are delegated to the system default window procedure.
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
    }
}

unsafe extern "system" fn about_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_CREATE => handle_about_create(hwnd, lparam),
        WM_COMMAND => {
            let control_id = loword(wparam);
            let notification = hiword(wparam);
            if control_id == ID_ABOUT_OK_BUTTON && u32::from(notification) == BN_CLICKED {
                // SAFETY: hwnd is the live About window receiving the button command.
                unsafe {
                    DestroyWindow(hwnd);
                }
                return 0;
            }
            if control_id == ID_ABOUT_PROJECT_LINK && u32::from(notification) == STN_CLICKED {
                handle_about_project_link_clicked(hwnd);
                return 0;
            }

            // SAFETY: unhandled messages are delegated to the system default window procedure.
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_ERASEBKGND => {
            if fill_about_window_background(hwnd, wparam as HDC) {
                return 1;
            }

            // SAFETY: unhandled background erase messages use the system default window procedure.
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_CTLCOLOREDIT | WM_CTLCOLORSTATIC | WM_CTLCOLORBTN | WM_CTLCOLORLISTBOX => {
            if let Some(brush) =
                about_control_color_brush(hwnd, message, wparam as HDC, lparam as HWND)
            {
                return brush as LRESULT;
            }

            // SAFETY: control color messages without window state use the system default handling.
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_DESTROY => {
            drop_about_window_state(hwnd);
            0
        }
        _ => {
            // SAFETY: unhandled messages are delegated to the system default window procedure.
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
    }
}

fn handle_about_create(hwnd: HWND, lparam: LPARAM) -> LRESULT {
    let create = lparam as *const CREATESTRUCTW;
    if create.is_null() {
        trace_gui("About window creation information could not be read");
        return -1;
    }

    // SAFETY: lparam for WM_CREATE points to a valid CREATESTRUCTW for this message.
    let params = unsafe { (*create).lpCreateParams as *const AboutWindowCreateParams };
    if params.is_null() {
        trace_gui("About window creation parameters could not be read");
        return -1;
    }

    // SAFETY: params and content point to stack data owned by show_about_window during
    // synchronous CreateWindowExW/WM_CREATE processing.
    let params = unsafe { &*params };
    if params.content.is_null() {
        trace_gui("About window content could not be read");
        return -1;
    }

    // SAFETY: lparam for WM_CREATE points to a valid CREATESTRUCTW for this message.
    let hinstance = unsafe { (*create).hInstance };
    // SAFETY: content is checked non-null above and is valid during this synchronous call.
    let content = unsafe { &*params.content };
    match AboutWindowState::create(hwnd, hinstance, params.theme, params.language, content) {
        Ok(state) => {
            let state_ptr = Box::into_raw(Box::new(state));
            // SAFETY: state_ptr remains owned by the About window until WM_DESTROY drops it.
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);
            }
            redraw_about_theme(hwnd);
            0
        }
        Err(error) => {
            trace_gui(format!("About window could not be initialized: {error}"));
            -1
        }
    }
}

fn handle_create(hwnd: HWND, lparam: LPARAM) -> LRESULT {
    let language = UiLanguage::English;
    let create = lparam as *const CREATESTRUCTW;
    if create.is_null() {
        show_message(
            hwnd,
            &UserMessage::error(
                localized(language, "Startup Error", "시작 오류"),
                localized(
                    language,
                    "Window creation information could not be read.",
                    "창 생성 정보를 읽을 수 없습니다.",
                ),
            ),
        );
        return -1;
    }

    // SAFETY: lparam for WM_CREATE points to a valid CREATESTRUCTW for the duration of handling.
    let hinstance = unsafe { (*create).hInstance };
    match WindowState::create(hwnd, hinstance) {
        Ok(state) => {
            let state_ptr = Box::into_raw(Box::new(state));
            // SAFETY: state_ptr remains owned by the window until WM_DESTROY drops it.
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);
            }
            redraw_theme(hwnd);
            0
        }
        Err(error) => {
            show_message(
                hwnd,
                &UserMessage::error(
                    localized(language, "Startup Error", "시작 오류"),
                    format!(
                        "{}\n\n{error}",
                        localized(
                            language,
                            "The GUI could not be initialized.",
                            "GUI를 초기화할 수 없습니다.",
                        )
                    ),
                ),
            );
            -1
        }
    }
}

fn handle_settings_toggle_clicked(hwnd: HWND) {
    let Some(state) = window_state_mut(hwnd) else {
        let language = UiLanguage::English;
        show_message(
            hwnd,
            &UserMessage::error(
                localized(language, "Settings Error", "설정 오류"),
                localized(
                    language,
                    "The window state could not be read.",
                    "창 상태를 읽을 수 없습니다.",
                ),
            ),
        );
        return;
    };

    let language = state.app_settings.ui.language;
    if let Err(error) = state.toggle_settings_panel() {
        show_message(hwnd, &user_message_for_gui_error(&error, language));
        return;
    }

    redraw_theme(hwnd);
}

fn handle_print_clicked(hwnd: HWND) {
    let Some(state) = window_state_mut(hwnd) else {
        let language = UiLanguage::English;
        show_message(
            hwnd,
            &UserMessage::error(
                localized(language, "Print Failed", "출력 실패"),
                localized(
                    language,
                    "The window state could not be read.",
                    "창 상태를 읽을 수 없습니다.",
                ),
            ),
        );
        return;
    };
    let language = state.app_settings.ui.language;

    if state.workflow_state.is_worker_running() {
        trace_gui("print click ignored because a worker is already running");
        return;
    }

    trace_gui("print button clicked");
    let input = match state.collect_input() {
        Ok(input) => input,
        Err(error) => {
            show_message(
                hwnd,
                &UserMessage::error(
                    localized(language, "Input Error", "입력 오류"),
                    format!(
                        "{}\n\n{error}",
                        localized(
                            language,
                            "The input values could not be read.",
                            "입력값을 읽을 수 없습니다.",
                        )
                    ),
                ),
            );
            return;
        }
    };

    let job = match app::build_print_job(input) {
        Ok(job) => {
            trace_gui(format!(
                "validated print job: target={}, layout={}, text_chars={}",
                job.settings.printer,
                job.settings.layout,
                job.text.chars().count()
            ));
            job
        }
        Err(error) => {
            trace_gui(format!("input validation failed: {error}"));
            show_message(hwnd, &user_message_for_app_error(&error, language));
            return;
        }
    };

    if let Err(error) = state.save_print_settings(&job.settings) {
        trace_gui(format!("settings save failed: {error}"));
        show_message(hwnd, &user_message_for_app_error(&error, language));
        return;
    }

    state.set_workflow_state(PrintWorkflowState::WorkerRunning);
    let hwnd_value = hwnd as isize;
    let spawn_result = thread::Builder::new()
        .name("escpos-print-worker".to_owned())
        .spawn(move || {
            trace_gui("print worker started");
            let renderer = Win32GdiTextImageRenderer;
            let printer = NetworkEscPosPrinter::new();
            let result = app::execute_print_job(&job, &renderer, &printer);
            match &result {
                Ok(()) => trace_gui("print worker completed successfully"),
                Err(error) => trace_gui(format!("print worker failed: {error}")),
            }
            let worker_result = worker_result_from_app_result(result, language);
            post_worker_completed(hwnd_value);
            worker_result
        });

    match spawn_result {
        Ok(worker_handle) => {
            state.worker_handle = Some(worker_handle);
            start_worker_completion_poll(hwnd);
        }
        Err(error) => {
            state.set_workflow_state(PrintWorkflowState::Idle);
            let gui_error = GuiError::WorkerStartFailed {
                details: error.to_string(),
            };
            show_message(
                hwnd,
                &UserMessage::error(
                    localized(language, "Print Failed", "출력 실패"),
                    format!(
                        "{}\n\n{gui_error}",
                        localized(
                            language,
                            "The print worker could not be started.",
                            "출력 작업을 시작할 수 없습니다.",
                        )
                    ),
                ),
            );
        }
    }
}

fn handle_theme_changed(hwnd: HWND) {
    let Some(state) = window_state_mut(hwnd) else {
        let language = UiLanguage::English;
        show_message(
            hwnd,
            &UserMessage::error(
                localized(language, "Theme Error", "테마 오류"),
                localized(
                    language,
                    "The window state could not be read.",
                    "창 상태를 읽을 수 없습니다.",
                ),
            ),
        );
        return;
    };
    let language = state.app_settings.ui.language;

    let theme = match theme_combo_selected_theme(state.controls.theme_combo) {
        Ok(theme) => theme,
        Err(error) => {
            show_message(
                hwnd,
                &UserMessage::error(
                    localized(language, "Theme Error", "테마 오류"),
                    format!(
                        "{}\n\n{error}",
                        localized(
                            language,
                            "The selected theme could not be read.",
                            "선택한 테마를 읽을 수 없습니다.",
                        )
                    ),
                ),
            );
            return;
        }
    };

    if let Err(error) = state.set_ui_theme(theme) {
        show_message(hwnd, &user_message_for_gui_error(&error, language));
        return;
    }

    redraw_theme(hwnd);
}

fn handle_language_changed(hwnd: HWND) {
    let Some(state) = window_state_mut(hwnd) else {
        let language = UiLanguage::English;
        show_message(
            hwnd,
            &UserMessage::error(
                localized(language, "Language Error", "언어 오류"),
                localized(
                    language,
                    "The window state could not be read.",
                    "창 상태를 읽을 수 없습니다.",
                ),
            ),
        );
        return;
    };
    let current_language = state.app_settings.ui.language;

    let language = match language_combo_selected_language(state.controls.language_combo) {
        Ok(language) => language,
        Err(error) => {
            show_message(
                hwnd,
                &UserMessage::error(
                    localized(current_language, "Language Error", "언어 오류"),
                    format!(
                        "{}\n\n{error}",
                        localized(
                            current_language,
                            "The selected language could not be read.",
                            "선택한 언어를 읽을 수 없습니다.",
                        )
                    ),
                ),
            );
            return;
        }
    };

    if let Err(error) = state.set_ui_language(language) {
        show_message(
            hwnd,
            &user_message_for_gui_error(&error, state.app_settings.ui.language),
        );
        return;
    }

    redraw_theme(hwnd);
}

fn handle_github_link_clicked(hwnd: HWND) {
    let language = window_state_mut(hwnd)
        .map(|state| state.app_settings.ui.language)
        .unwrap_or(UiLanguage::English);

    trace_gui(format!("opening project link: {PROJECT_LINK_URL}"));
    if let Err(error) = open_url_in_default_browser(PROJECT_LINK_URL) {
        show_message(
            hwnd,
            &UserMessage::error(
                localized(language, "Link Error", "링크 오류"),
                format!(
                    "{}\n\n{error}",
                    localized(
                        language,
                        "The project link could not be opened.",
                        "프로젝트 링크를 열 수 없습니다.",
                    )
                ),
            ),
        );
    }
}

fn handle_about_project_link_clicked(hwnd: HWND) {
    let language = about_window_state_mut(hwnd)
        .map(|state| state.language)
        .unwrap_or(UiLanguage::English);

    trace_gui(format!("opening About project URL: {PROJECT_LINK_URL}"));
    if let Err(error) = open_url_in_default_browser(PROJECT_LINK_URL) {
        show_message(
            hwnd,
            &UserMessage::error(
                localized(language, "Link Error", "링크 오류"),
                format!(
                    "{}\n\n{error}",
                    localized(
                        language,
                        "The project URL could not be opened.",
                        "프로젝트 URL을 열 수 없습니다.",
                    )
                ),
            ),
        );
    }
}

fn handle_about_link_clicked(hwnd: HWND) {
    let (language, theme) = window_state_mut(hwnd)
        .map(|state| (state.app_settings.ui.language, state.app_settings.ui.theme))
        .unwrap_or((UiLanguage::English, UiTheme::Light));

    trace_gui("showing About window");
    if let Err(error) = show_about_window(hwnd, language, theme) {
        show_message(hwnd, &user_message_for_gui_error(&error, language));
    }
}

fn show_about_window(owner: HWND, language: UiLanguage, theme: UiTheme) -> Result<(), GuiError> {
    let hinstance = module_handle()?;
    let content = about_window_content();
    let params = AboutWindowCreateParams {
        theme,
        language,
        content: &content,
    };
    let class_name = wide_null(ABOUT_WINDOW_CLASS_NAME);
    let title = wide_null(&content.title);

    // SAFETY: class_name and title are null-terminated and live for this call. params points to
    // stack data that is read only during the synchronous WM_CREATE handling inside
    // CreateWindowExW.
    let hwnd = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_CAPTION | WS_SYSMENU,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            ABOUT_WINDOW_WIDTH,
            ABOUT_WINDOW_HEIGHT,
            owner,
            null_mut(),
            hinstance,
            (&params as *const AboutWindowCreateParams).cast(),
        )
    };

    if hwnd.is_null() {
        return Err(GuiError::Win32CallFailed("CreateWindowExW(about)"));
    }

    if let Err(error) = set_window_icons(hwnd, hinstance) {
        // SAFETY: hwnd was returned by CreateWindowExW above and is not shown yet.
        unsafe {
            DestroyWindow(hwnd);
        }
        return Err(error);
    }

    // SAFETY: hwnd is a live top-level About window returned by CreateWindowExW.
    unsafe {
        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);
    }

    Ok(())
}

fn about_window_content() -> AboutWindowContent {
    AboutWindowContent {
        title: format!("About {APP_NAME}"),
        version_label: format!("{APP_NAME} {APP_VERSION}"),
        body_text: load_about_text(),
        project_url: PROJECT_LINK_URL,
        ok_label: "OK",
    }
}

fn load_about_text() -> String {
    env::current_exe()
        .ok()
        .and_then(|path| read_about_text_for_executable(&path))
        .unwrap_or_else(|| DEFAULT_ABOUT_TEXT.to_owned())
}

fn read_about_text_for_executable(executable_path: &Path) -> Option<String> {
    about_text_path_for_executable(executable_path).and_then(|path| fs::read_to_string(path).ok())
}

fn about_text_path_for_executable(executable_path: &Path) -> Option<PathBuf> {
    executable_path
        .parent()
        .filter(|directory| !directory.as_os_str().is_empty())
        .map(|directory| directory.join(ABOUT_TEXT_FILE))
}

fn win32_edit_multiline_text(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', "\r\n")
}

fn handle_print_completed(hwnd: HWND) {
    stop_worker_completion_poll(hwnd);

    let result = {
        let Some(state) = window_state_mut(hwnd) else {
            trace_gui("print completion ignored because window state is unavailable");
            return;
        };

        let language = state.app_settings.ui.language;
        let worker_was_running = state.workflow_state.is_worker_running();
        match state.worker_handle.take() {
            Some(worker_handle) => join_worker_result(worker_handle, language),
            None if worker_was_running => WorkerResult {
                message: UserMessage::error(
                    localized(language, "Print Failed", "출력 실패"),
                    localized(
                        language,
                        "The print worker state could not be read.",
                        "출력 작업 상태를 읽을 수 없습니다.",
                    ),
                ),
            },
            None => {
                trace_gui("print completion ignored because no worker is pending");
                return;
            }
        }
    };

    if let Some(state) = window_state_mut(hwnd) {
        state.set_workflow_state(PrintWorkflowState::Idle);
    }

    show_message(hwnd, &result.message);
}

fn handle_worker_completion_poll(hwnd: HWND) {
    let action = {
        let Some(state) = window_state_mut(hwnd) else {
            trace_gui("worker completion poll stopped because window state is unavailable");
            stop_worker_completion_poll(hwnd);
            return;
        };

        let has_worker_handle = state.worker_handle.is_some();
        let worker_finished = state
            .worker_handle
            .as_ref()
            .map(|worker_handle| worker_handle.is_finished())
            .unwrap_or(false);
        worker_completion_poll_action(has_worker_handle, worker_finished)
    };

    match action {
        WorkerCompletionPollAction::Wait => {}
        WorkerCompletionPollAction::Complete => {
            trace_gui("worker completion poll detected a finished worker");
            handle_print_completed(hwnd);
        }
        WorkerCompletionPollAction::Stop => {
            trace_gui("worker completion poll stopped because worker handle is unavailable");
            stop_worker_completion_poll(hwnd);
        }
    }
}

fn worker_result_from_app_result(
    result: Result<(), AppError>,
    language: UiLanguage,
) -> WorkerResult {
    let message = match result {
        Ok(()) => UserMessage::info(
            localized(language, "Success", "성공"),
            localized(language, "Printing is complete.", "출력이 완료되었습니다."),
        ),
        Err(error) => user_message_for_app_error(&error, language),
    };

    WorkerResult { message }
}

fn join_worker_result(
    worker_handle: thread::JoinHandle<WorkerResult>,
    language: UiLanguage,
) -> WorkerResult {
    match worker_handle.join() {
        Ok(result) => result,
        Err(_) => {
            trace_gui("print worker panicked");
            WorkerResult {
                message: UserMessage::error(
                    localized(language, "Print Failed", "출력 실패"),
                    localized(
                        language,
                        "The print worker exited unexpectedly.",
                        "출력 작업이 비정상적으로 종료되었습니다.",
                    ),
                ),
            }
        }
    }
}

fn start_worker_completion_poll(hwnd: HWND) {
    // SAFETY: hwnd is the live top-level window that owns the worker state. A null timer callback
    // routes WM_TIMER back to the window procedure on the GUI thread.
    let timer_id = unsafe {
        SetTimer(
            hwnd,
            WORKER_COMPLETION_TIMER_ID,
            WORKER_COMPLETION_POLL_MS,
            None,
        )
    };
    if timer_id == 0 {
        trace_gui("failed to start worker completion poll timer");
    }
}

fn stop_worker_completion_poll(hwnd: HWND) {
    // SAFETY: hwnd is the top-level window used to create the timer. It is valid while handling
    // messages for this window; KillTimer is harmless when the timer is already gone.
    let _ = unsafe { KillTimer(hwnd, WORKER_COMPLETION_TIMER_ID) };
}

fn post_worker_completed(hwnd_value: isize) {
    let hwnd = hwnd_value as HWND;

    // SAFETY: hwnd is the top-level window handle captured as an integer when the worker was
    // created. The worker result is owned by the JoinHandle and is collected on the GUI thread.
    let posted = unsafe { PostMessageW(hwnd, WM_PRINT_COMPLETED, 0, 0) };
    if worker_completion_post_state(posted) == WorkerCompletionPostState::FallbackPending {
        trace_gui(
            "failed to post worker completion message to GUI thread; waiting for poll fallback",
        );
    }
}

fn open_url_in_default_browser(url: &str) -> Result<(), GuiError> {
    let url = wide_null(url);

    // SAFETY: url is a null-terminated UTF-16 string that lives for the duration of the call.
    // Null verb/directory/parameters asks Windows to use the default open action.
    let result =
        unsafe { ShellExecuteW(null_mut(), null(), url.as_ptr(), null(), null(), SW_SHOW) };

    if shell_execute_open_succeeded(result as isize) {
        Ok(())
    } else {
        Err(GuiError::Win32CallFailed("ShellExecuteW(open URL)"))
    }
}

const fn shell_execute_open_succeeded(result: isize) -> bool {
    result >= SHELL_EXECUTE_SUCCESS_MIN
}

fn fill_window_background(hwnd: HWND, hdc: HDC) -> bool {
    if hdc.is_null() {
        return false;
    }

    let Some(state) = window_state_mut(hwnd) else {
        return false;
    };
    let mut rect = RECT::default();

    // SAFETY: hwnd is the window being painted and rect is a valid out pointer.
    if unsafe { GetClientRect(hwnd, &mut rect) } == 0 {
        return false;
    }

    // SAFETY: hdc is supplied by WM_ERASEBKGND, rect is initialized, and the brush is either a
    // system brush or an owned brush that lives with WindowState.
    unsafe { FillRect(hdc, &rect, state.theme_resources.background_brush()) != 0 }
}

fn fill_about_window_background(hwnd: HWND, hdc: HDC) -> bool {
    if hdc.is_null() {
        return false;
    }

    let Some(state) = about_window_state_mut(hwnd) else {
        return false;
    };
    let mut rect = RECT::default();

    // SAFETY: hwnd is the window being painted and rect is a valid out pointer.
    if unsafe { GetClientRect(hwnd, &mut rect) } == 0 {
        return false;
    }

    // SAFETY: hdc is supplied by WM_ERASEBKGND, rect is initialized, and the brush is either a
    // system brush or an owned brush that lives with AboutWindowState.
    unsafe { FillRect(hdc, &rect, state.theme_resources.background_brush()) != 0 }
}

fn control_color_brush(hwnd: HWND, message: u32, hdc: HDC, control: HWND) -> Option<HBRUSH> {
    if hdc.is_null() {
        return None;
    }

    let state = window_state_mut(hwnd)?;
    match message {
        WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
            state.theme_resources.apply_field_colors(hdc);
            Some(state.theme_resources.field_brush())
        }
        WM_CTLCOLORBTN => {
            state.theme_resources.apply_button_colors(hdc);
            Some(state.theme_resources.button_brush())
        }
        WM_CTLCOLORSTATIC => {
            if control == state.controls.github_link || control == state.controls.about_link {
                state.theme_resources.apply_link_colors(hdc);
            } else {
                state.theme_resources.apply_background_colors(hdc);
            }
            Some(state.theme_resources.background_brush())
        }
        _ => None,
    }
}

fn about_control_color_brush(hwnd: HWND, message: u32, hdc: HDC, control: HWND) -> Option<HBRUSH> {
    if hdc.is_null() {
        return None;
    }

    let state = about_window_state_mut(hwnd)?;
    match message {
        WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
            state.theme_resources.apply_field_colors(hdc);
            Some(state.theme_resources.field_brush())
        }
        WM_CTLCOLORBTN => {
            state.theme_resources.apply_button_colors(hdc);
            Some(state.theme_resources.button_brush())
        }
        WM_CTLCOLORSTATIC => {
            if control == state.controls.project_link {
                state.theme_resources.apply_link_colors(hdc);
            } else {
                state.theme_resources.apply_background_colors(hdc);
            }
            Some(state.theme_resources.background_brush())
        }
        _ => None,
    }
}

fn redraw_theme(hwnd: HWND) {
    invalidate_window(hwnd);
    if let Some(state) = window_state_mut(hwnd) {
        state.controls.invalidate();
    }

    // SAFETY: hwnd is a live top-level window while called from the GUI thread.
    unsafe {
        UpdateWindow(hwnd);
    }
}

fn redraw_about_theme(hwnd: HWND) {
    invalidate_window(hwnd);
    if let Some(state) = about_window_state_mut(hwnd) {
        state.controls.invalidate();
    }

    // SAFETY: hwnd is a live About window while called from the GUI thread.
    unsafe {
        UpdateWindow(hwnd);
    }
}

fn invalidate_window(hwnd: HWND) {
    if hwnd.is_null() {
        return;
    }

    // SAFETY: hwnd is a live window/control handle; null rect invalidates the full client area.
    unsafe {
        InvalidateRect(hwnd, null_mut(), 1);
    }
}

fn show_child_window(hwnd: HWND, visible: bool) {
    if hwnd.is_null() {
        return;
    }

    let command = if visible { SW_SHOW } else { SW_HIDE };

    // SAFETY: hwnd is a live child control created by this GUI while the window state is alive.
    unsafe {
        ShowWindow(hwnd, command);
    }
}

fn move_child_window(hwnd: HWND, rect: ControlRect) -> Result<(), GuiError> {
    if hwnd.is_null() {
        return Err(GuiError::Win32CallFailed("MoveWindow(null)"));
    }

    // SAFETY: hwnd is a live child control created by this GUI while the window state is alive.
    let moved = unsafe { MoveWindow(hwnd, rect.x, rect.y, rect.width, rect.height, 1) };
    if moved == 0 {
        return Err(GuiError::Win32CallFailed("MoveWindow"));
    }

    Ok(())
}

fn trace_gui(message: impl AsRef<str>) {
    eprintln!("[j3ecs-netprint gui] {}", message.as_ref());
}

fn user_message_for_app_error(error: &AppError, language: UiLanguage) -> UserMessage {
    match error {
        AppError::Input(InputValidationError::NumericFieldsMustBeNumbers) => UserMessage::error(
            localized(language, "Input Error", "입력 오류"),
            localized(
                language,
                "Port, font size, and paper width must be numbers.",
                "포트, 폰트 크기, 용지 폭은 숫자여야 합니다.",
            ),
        ),
        AppError::Domain(DomainError::EmptyText) => UserMessage::warning(
            localized(language, "Warning", "경고"),
            localized(
                language,
                "Enter text to print.",
                "출력할 내용을 입력해주세요.",
            ),
        ),
        AppError::Domain(DomainError::EmptyFontFaceName) => UserMessage::warning(
            localized(language, "Warning", "경고"),
            localized(language, "Select a font.", "폰트를 선택해주세요."),
        ),
        AppError::Domain(DomainError::EmptyPrinterHost) => UserMessage::warning(
            localized(language, "Input Error", "입력 오류"),
            localized(
                language,
                "Enter an IP address or host name.",
                "IP 주소 또는 호스트명을 입력해주세요.",
            ),
        ),
        AppError::Domain(DomainError::InvalidPrinterHost { .. }) => UserMessage::error(
            localized(language, "Input Error", "입력 오류"),
            format!(
                "{}\n\n{error}",
                localized(
                    language,
                    "The IP address or host name is invalid.",
                    "IP 주소 또는 호스트명 형식이 올바르지 않습니다.",
                )
            ),
        ),
        AppError::Domain(DomainError::InvalidPrinterPort { .. }) => UserMessage::error(
            localized(language, "Input Error", "입력 오류"),
            format!(
                "{}\n\n{error}",
                localized(language, "Check the Port value.", "Port 값을 확인해주세요.",)
            ),
        ),
        AppError::Domain(
            DomainError::FontSizeTooSmall { .. }
            | DomainError::PaperWidthTooSmall { .. }
            | DomainError::PrintableWidthTooSmall { .. },
        ) => UserMessage::error(
            localized(language, "Input Error", "입력 오류"),
            format!(
                "{}\n\n{error}",
                localized(
                    language,
                    "Check the font size or paper width.",
                    "폰트 크기 또는 용지 폭 값을 확인해주세요.",
                )
            ),
        ),
        AppError::Infra(InfraError::TextRendering { details }) if is_font_error(details) => {
            UserMessage::error(
                localized(language, "Font Error", "폰트 오류"),
                format!(
                    "{}\n\n{details}",
                    localized(
                        language,
                        "The selected font cannot be used.",
                        "폰트를 사용할 수 없습니다.",
                    )
                ),
            )
        }
        AppError::Infra(InfraError::TextRendering { details }) => UserMessage::error(
            localized(language, "Print Failed", "출력 실패"),
            format!(
                "{}\n\n{details}",
                localized(
                    language,
                    "An error occurred while creating the image.",
                    "이미지 생성 중 오류가 발생했습니다.",
                )
            ),
        ),
        AppError::Infra(InfraError::NetworkIo { .. }) => UserMessage::error(
            localized(language, "Print Failed", "출력 실패"),
            format!(
                "{}\n\n{error}",
                localized(
                    language,
                    "An error occurred while communicating with the printer.",
                    "프린터 통신 중 오류가 발생했습니다.",
                )
            ),
        ),
        AppError::Infra(InfraError::EscPosEncoding(_)) => UserMessage::error(
            localized(language, "Print Failed", "출력 실패"),
            format!(
                "{}\n\n{error}",
                localized(
                    language,
                    "An error occurred while creating ESC/POS output data.",
                    "ESC/POS 출력 데이터 생성 중 오류가 발생했습니다.",
                )
            ),
        ),
        AppError::Infra(
            InfraError::InvalidPrinterTarget(_) | InfraError::UnsupportedPlatform(_),
        ) => UserMessage::error(
            localized(language, "Print Failed", "출력 실패"),
            format!("{error}"),
        ),
        AppError::Settings(_) => UserMessage::error(
            localized(language, "Settings Save Failed", "설정 저장 실패"),
            format!(
                "{}\n\n{error}",
                localized(
                    language,
                    "The settings file could not be saved.",
                    "설정파일을 저장할 수 없습니다.",
                )
            ),
        ),
        AppError::Ui(details) => {
            UserMessage::error(localized(language, "Error", "오류"), details.as_str())
        }
    }
}

fn user_message_for_gui_error(error: &GuiError, language: UiLanguage) -> UserMessage {
    match error {
        GuiError::App(error) => user_message_for_app_error(error, language),
        _ => UserMessage::error(localized(language, "Error", "오류"), format!("{error}")),
    }
}

fn is_font_error(details: &str) -> bool {
    details.contains("font face") || details.contains("installed font")
}

fn register_window_class(hinstance: HINSTANCE) -> Result<(), GuiError> {
    let class_name = wide_null(WINDOW_CLASS_NAME);
    let icon = load_app_icon(hinstance, 0, 0, LR_DEFAULTSIZE | LR_SHARED)?;

    // SAFETY: passing a null instance with IDC_ARROW loads the predefined arrow cursor.
    let cursor = unsafe { LoadCursorW(null_mut(), IDC_ARROW) };
    if cursor.is_null() {
        return Err(GuiError::Win32CallFailed("LoadCursorW"));
    }

    let wnd_class = WNDCLASSW {
        lpfnWndProc: Some(window_proc),
        hInstance: hinstance,
        hIcon: icon,
        hCursor: cursor,
        hbrBackground: system_color_brush(COLOR_WINDOW),
        lpszClassName: class_name.as_ptr(),
        ..WNDCLASSW::default()
    };

    // SAFETY: wnd_class points to a valid class definition and class_name remains alive for this
    // call.
    let atom = unsafe { RegisterClassW(&wnd_class) };
    if atom == 0 {
        return Err(GuiError::Win32CallFailed("RegisterClassW"));
    }

    Ok(())
}

fn register_about_window_class(hinstance: HINSTANCE) -> Result<(), GuiError> {
    let class_name = wide_null(ABOUT_WINDOW_CLASS_NAME);
    let icon = load_app_icon(hinstance, 0, 0, LR_DEFAULTSIZE | LR_SHARED)?;

    // SAFETY: passing a null instance with IDC_ARROW loads the predefined arrow cursor.
    let cursor = unsafe { LoadCursorW(null_mut(), IDC_ARROW) };
    if cursor.is_null() {
        return Err(GuiError::Win32CallFailed("LoadCursorW"));
    }

    let wnd_class = WNDCLASSW {
        lpfnWndProc: Some(about_window_proc),
        hInstance: hinstance,
        hIcon: icon,
        hCursor: cursor,
        hbrBackground: system_color_brush(COLOR_WINDOW),
        lpszClassName: class_name.as_ptr(),
        ..WNDCLASSW::default()
    };

    // SAFETY: wnd_class points to a valid class definition and class_name remains alive for this
    // call.
    let atom = unsafe { RegisterClassW(&wnd_class) };
    if atom == 0 {
        return Err(GuiError::Win32CallFailed("RegisterClassW(about)"));
    }

    Ok(())
}

fn set_window_icons(hwnd: HWND, hinstance: HINSTANCE) -> Result<(), GuiError> {
    let big_icon = load_app_icon(
        hinstance,
        system_metric(SM_CXICON),
        system_metric(SM_CYICON),
        LR_SHARED,
    )?;
    let small_icon = load_app_icon(
        hinstance,
        system_metric(SM_CXSMICON),
        system_metric(SM_CYSMICON),
        LR_SHARED,
    )?;

    // SAFETY: hwnd is a live top-level window. The icon handles are shared module resources and
    // remain valid for the process lifetime.
    unsafe {
        SendMessageW(hwnd, WM_SETICON, ICON_BIG as WPARAM, big_icon as LPARAM);
        SendMessageW(hwnd, WM_SETICON, ICON_SMALL as WPARAM, small_icon as LPARAM);
    }

    Ok(())
}

fn load_app_icon(
    hinstance: HINSTANCE,
    width: i32,
    height: i32,
    flags: u32,
) -> Result<HICON, GuiError> {
    // SAFETY: the icon resource is embedded in this module with integer resource ID 1.
    let handle = unsafe {
        LoadImageW(
            hinstance,
            integer_resource(APP_ICON_RESOURCE_ID),
            IMAGE_ICON,
            width,
            height,
            flags,
        )
    };

    if handle.is_null() {
        return Err(GuiError::Win32CallFailed("LoadImageW(app icon)"));
    }

    Ok(handle as HICON)
}

fn integer_resource(resource_id: u16) -> *const u16 {
    usize::from(resource_id) as *const u16
}

fn system_metric(index: i32) -> i32 {
    // SAFETY: index is a documented system metric constant.
    unsafe { GetSystemMetrics(index) }
}

fn message_loop() -> Result<(), GuiError> {
    let mut message = MSG::default();

    loop {
        // SAFETY: message is a valid out pointer. A null hwnd retrieves messages for the thread.
        let result = unsafe { GetMessageW(&mut message, null_mut(), 0, 0) };
        if result == -1 {
            return Err(GuiError::Win32CallFailed("GetMessageW"));
        }
        if result == 0 {
            return Ok(());
        }

        // SAFETY: message was returned by GetMessageW for this GUI thread.
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

fn module_handle() -> Result<HINSTANCE, GuiError> {
    // SAFETY: a null module name requests the current process module handle.
    let handle = unsafe { GetModuleHandleW(null_mut()) };
    if handle.is_null() {
        return Err(GuiError::Win32CallFailed("GetModuleHandleW"));
    }

    Ok(handle)
}

fn create_static(
    hinstance: HINSTANCE,
    parent: HWND,
    text: &str,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<HWND, GuiError> {
    create_child(
        hinstance,
        parent,
        "STATIC",
        text,
        WS_CHILD | WS_VISIBLE,
        x,
        y,
        width,
        height,
        0,
        0,
    )
}

fn create_clickable_static(
    hinstance: HINSTANCE,
    parent: HWND,
    text: &str,
    rect: ControlRect,
    control_id: i32,
) -> Result<HWND, GuiError> {
    create_child(
        hinstance,
        parent,
        "STATIC",
        text,
        WS_CHILD | WS_VISIBLE | STATIC_NOTIFY_STYLE,
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        control_id,
        0,
    )
}

#[allow(clippy::too_many_arguments)]
fn create_edit(
    hinstance: HINSTANCE,
    parent: HWND,
    text: &str,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    edit_style: i32,
    extra_style: u32,
) -> Result<HWND, GuiError> {
    create_child(
        hinstance,
        parent,
        "EDIT",
        text,
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_BORDER | extra_style | edit_style as u32,
        x,
        y,
        width,
        height,
        0,
        WS_EX_CLIENTEDGE,
    )
}

fn create_combo_box(
    hinstance: HINSTANCE,
    parent: HWND,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    control_id: i32,
) -> Result<HWND, GuiError> {
    create_child(
        hinstance,
        parent,
        "COMBOBOX",
        "",
        combo_box_style(false),
        x,
        y,
        width,
        height,
        control_id,
        0,
    )
}

fn create_sorted_combo_box(
    hinstance: HINSTANCE,
    parent: HWND,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    control_id: i32,
) -> Result<HWND, GuiError> {
    create_child(
        hinstance,
        parent,
        "COMBOBOX",
        "",
        combo_box_style(true),
        x,
        y,
        width,
        height,
        control_id,
        0,
    )
}

const fn combo_box_style(sorted: bool) -> u32 {
    let mut style = WS_CHILD
        | WS_VISIBLE
        | WS_TABSTOP
        | WS_VSCROLL
        | (CBS_DROPDOWNLIST as u32)
        | (CBS_HASSTRINGS as u32);
    if sorted {
        style |= CBS_SORT as u32;
    }
    style
}

#[allow(clippy::too_many_arguments)]
fn create_child(
    hinstance: HINSTANCE,
    parent: HWND,
    class_name: &str,
    text: &str,
    style: u32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    control_id: i32,
    ex_style: u32,
) -> Result<HWND, GuiError> {
    let class_name = wide_null(class_name);
    let text = wide_null(text);
    let menu = control_id_to_menu(control_id);

    // SAFETY: class_name and text are null-terminated and live for the call. parent is the owner
    // window, hinstance is the current module handle, and menu is either null or a child control ID.
    let hwnd = unsafe {
        CreateWindowExW(
            ex_style,
            class_name.as_ptr(),
            text.as_ptr(),
            style,
            x,
            y,
            width,
            height,
            parent,
            menu,
            hinstance,
            null_mut(),
        )
    };

    if hwnd.is_null() {
        return Err(GuiError::Win32CallFailed("CreateWindowExW(child)"));
    }

    Ok(hwnd)
}

fn control_id_to_menu(control_id: i32) -> HMENU {
    if control_id == 0 {
        null_mut()
    } else {
        control_id as isize as HMENU
    }
}

fn default_gui_font() -> HGDIOBJ {
    // SAFETY: DEFAULT_GUI_FONT is a valid stock object selector.
    unsafe { GetStockObject(DEFAULT_GUI_FONT) }
}

#[derive(Debug)]
struct OwnedGuiFont {
    handle: HFONT,
}

impl OwnedGuiFont {
    fn create(face_name: &str, height_px: i32) -> Result<Self, GuiError> {
        let wide_face = wide_null(face_name);

        // SAFETY: wide_face is null-terminated and the fixed font attributes are valid Win32
        // CreateFontW arguments. A negative height requests a character height in pixels.
        let handle = unsafe {
            CreateFontW(
                -height_px,
                0,
                0,
                0,
                FW_NORMAL as i32,
                0,
                0,
                0,
                u32::from(DEFAULT_CHARSET),
                u32::from(OUT_DEFAULT_PRECIS),
                u32::from(CLIP_DEFAULT_PRECIS),
                u32::from(ANTIALIASED_QUALITY),
                u32::from(DEFAULT_PITCH | FF_DONTCARE),
                wide_face.as_ptr(),
            )
        };

        if handle.is_null() {
            return Err(GuiError::Win32CallFailed("CreateFontW(text edit)"));
        }

        Ok(Self { handle })
    }

    fn handle(&self) -> HFONT {
        self.handle
    }
}

impl Drop for OwnedGuiFont {
    fn drop(&mut self) {
        if self.handle.is_null() {
            return;
        }

        // SAFETY: handle is an HFONT returned by CreateFontW and owned by this wrapper.
        let _ = unsafe { DeleteObject(self.handle as HGDIOBJ) };
    }
}

fn apply_font(hwnd: HWND, font: HGDIOBJ) {
    if hwnd.is_null() || font.is_null() {
        return;
    }

    // SAFETY: hwnd is a live control and font is either a stock font or an owned font kept alive
    // for the control lifetime.
    unsafe {
        SendMessageW(hwnd, WM_SETFONT, font as WPARAM, 1);
    }
}

fn populate_font_face_combo(hwnd: HWND, preferred_face: &str) -> Result<(), GuiError> {
    let font_faces = installed_font_face_names()?;
    if font_faces.is_empty() {
        return Err(GuiError::Win32CallFailed("EnumFontFamiliesExW(no fonts)"));
    }

    for font_face in &font_faces {
        combo_box_add_string(hwnd, font_face)?;
    }

    for fallback_face in [
        preferred_face,
        crate::domain::DEFAULT_FONT_FACE_NAME,
        "Segoe UI",
        "Arial",
        "Consolas",
    ] {
        if select_combo_box_exact(hwnd, fallback_face)? {
            return Ok(());
        }
    }

    let selected = unsafe { SendMessageW(hwnd, CB_SETCURSEL, 0, 0) };
    if is_combo_error(selected) {
        return Err(GuiError::Win32CallFailed("SendMessageW(CB_SETCURSEL)"));
    }

    Ok(())
}

fn populate_theme_combo(
    hwnd: HWND,
    selected_theme: UiTheme,
    language: UiLanguage,
) -> Result<(), GuiError> {
    for theme in [UiTheme::Light, UiTheme::Dark] {
        combo_box_add_string(hwnd, ui_theme_label(theme, language))?;
    }

    select_theme_combo(hwnd, selected_theme, language)
}

fn select_theme_combo(hwnd: HWND, theme: UiTheme, language: UiLanguage) -> Result<(), GuiError> {
    if select_combo_box_exact(hwnd, ui_theme_label(theme, language))? {
        return Ok(());
    }

    Err(GuiError::Win32CallFailed("select UI theme"))
}

fn theme_combo_selected_theme(hwnd: HWND) -> Result<UiTheme, GuiError> {
    let selected = combo_box_selected_text(hwnd, "테마")?;
    ui_theme_from_label(&selected).ok_or(GuiError::Win32CallFailed("selected UI theme"))
}

fn populate_language_combo(
    hwnd: HWND,
    selected_language: UiLanguage,
    display_language: UiLanguage,
) -> Result<(), GuiError> {
    for language in [UiLanguage::English, UiLanguage::Korean] {
        combo_box_add_string(hwnd, ui_language_label(language, display_language))?;
    }

    select_language_combo(hwnd, selected_language, display_language)
}

fn select_language_combo(
    hwnd: HWND,
    language: UiLanguage,
    display_language: UiLanguage,
) -> Result<(), GuiError> {
    if select_combo_box_exact(hwnd, ui_language_label(language, display_language))? {
        return Ok(());
    }

    Err(GuiError::Win32CallFailed("select UI language"))
}

fn language_combo_selected_language(hwnd: HWND) -> Result<UiLanguage, GuiError> {
    let selected = combo_box_selected_text(hwnd, "Language")?;
    ui_language_from_label(&selected).ok_or(GuiError::Win32CallFailed("selected UI language"))
}

const fn toggled_settings_panel_visibility(visible: bool) -> bool {
    !visible
}

const fn settings_panel_layout(settings_panel_visible: bool) -> SettingsPanelLayout {
    if settings_panel_visible {
        SettingsPanelLayout {
            text_group: TEXT_GROUP_EXPANDED_RECT,
            text_edit: TEXT_EDIT_EXPANDED_RECT,
        }
    } else {
        SettingsPanelLayout {
            text_group: TEXT_GROUP_COLLAPSED_RECT,
            text_edit: TEXT_EDIT_COLLAPSED_RECT,
        }
    }
}

fn settings_toggle_button_label(settings_panel_visible: bool, text: &UiText) -> &'static str {
    if settings_panel_visible {
        text.settings_hide_button_label
    } else {
        text.settings_show_button_label
    }
}

fn program_version_text(text: &UiText) -> String {
    format!("{} {APP_VERSION}", text.version_label)
}

fn installed_font_face_names() -> Result<Vec<String>, GuiError> {
    let dc = FontEnumerationDc::create()?;
    let logfont = LOGFONTW {
        lfCharSet: DEFAULT_CHARSET,
        ..LOGFONTW::default()
    };
    let mut font_faces: Vec<String> = Vec::new();

    // SAFETY: dc is a live memory DC, logfont is initialized for default charset enumeration,
    // and lparam points to font_faces for the duration of EnumFontFamiliesExW.
    unsafe {
        EnumFontFamiliesExW(
            dc.handle(),
            &logfont,
            Some(collect_font_face),
            (&mut font_faces as *mut Vec<String>) as LPARAM,
            0,
        );
    }

    font_faces.sort_by_key(|font_face| normalize_font_face_name(font_face));
    font_faces
        .dedup_by(|left, right| normalize_font_face_name(left) == normalize_font_face_name(right));

    Ok(font_faces)
}

unsafe extern "system" fn collect_font_face(
    logfont: *const LOGFONTW,
    _text_metric: *const TEXTMETRICW,
    _font_type: u32,
    lparam: LPARAM,
) -> i32 {
    if logfont.is_null() || lparam == 0 {
        return 1;
    }

    // SAFETY: lparam is the Vec<String> pointer supplied by installed_font_face_names.
    let font_faces = unsafe { &mut *(lparam as *mut Vec<String>) };
    // SAFETY: logfont is supplied by GDI during enumeration and is valid for this callback.
    let face_name = fixed_wide_to_string(unsafe { &(*logfont).lfFaceName });
    if face_name.is_empty() || face_name.starts_with('@') {
        return 1;
    }

    if !font_faces
        .iter()
        .any(|existing| normalize_font_face_name(existing) == normalize_font_face_name(&face_name))
    {
        font_faces.push(face_name);
    }

    1
}

fn combo_box_add_string(hwnd: HWND, text: &str) -> Result<(), GuiError> {
    let wide_text = wide_null(text);

    // SAFETY: hwnd is a live combo box and wide_text is null-terminated for this call.
    let result = unsafe { SendMessageW(hwnd, CB_ADDSTRING, 0, wide_text.as_ptr() as LPARAM) };
    if is_combo_error(result) || result == CB_ERRSPACE as LRESULT {
        return Err(GuiError::Win32CallFailed("SendMessageW(CB_ADDSTRING)"));
    }

    Ok(())
}

fn reset_combo_box(hwnd: HWND) -> Result<(), GuiError> {
    // SAFETY: hwnd is a live combo box created by this GUI.
    let result = unsafe { SendMessageW(hwnd, CB_RESETCONTENT, 0, 0) };
    if is_combo_error(result) {
        return Err(GuiError::Win32CallFailed("SendMessageW(CB_RESETCONTENT)"));
    }

    Ok(())
}

fn select_combo_box_exact(hwnd: HWND, text: &str) -> Result<bool, GuiError> {
    let wide_text = wide_null(text);

    // SAFETY: hwnd is a live combo box and wide_text is null-terminated for this call.
    let index = unsafe {
        SendMessageW(
            hwnd,
            CB_FINDSTRINGEXACT,
            usize::MAX,
            wide_text.as_ptr() as LPARAM,
        )
    };
    if is_combo_error(index) {
        return Ok(false);
    }

    // SAFETY: index came from this combo box and selects an existing item.
    let selected = unsafe { SendMessageW(hwnd, CB_SETCURSEL, index as WPARAM, 0) };
    if is_combo_error(selected) {
        return Err(GuiError::Win32CallFailed("SendMessageW(CB_SETCURSEL)"));
    }

    Ok(true)
}

fn combo_box_selected_text(hwnd: HWND, control: &'static str) -> Result<String, GuiError> {
    // SAFETY: hwnd is a live combo box created by this GUI.
    let index = unsafe { SendMessageW(hwnd, CB_GETCURSEL, 0, 0) };
    if is_combo_error(index) {
        return Err(GuiError::Win32CallFailed("SendMessageW(CB_GETCURSEL)"));
    }

    // SAFETY: index identifies the selected item in this combo box.
    let text_len = unsafe { SendMessageW(hwnd, CB_GETLBTEXTLEN, index as WPARAM, 0) };
    if is_combo_error(text_len) {
        return Err(GuiError::Win32CallFailed("SendMessageW(CB_GETLBTEXTLEN)"));
    }

    let buffer_len = usize::try_from(text_len)
        .ok()
        .and_then(|len| len.checked_add(1))
        .ok_or(GuiError::WindowTextTooLong { control })?;
    let mut buffer = vec![0u16; buffer_len];

    // SAFETY: buffer is writable for the selected item's UTF-16 text plus a null terminator.
    let copied = unsafe {
        SendMessageW(
            hwnd,
            CB_GETLBTEXT,
            index as WPARAM,
            buffer.as_mut_ptr() as LPARAM,
        )
    };
    if is_combo_error(copied) {
        return Err(GuiError::Win32CallFailed("SendMessageW(CB_GETLBTEXT)"));
    }

    let copied = usize::try_from(copied).map_err(|_| GuiError::WindowTextTooLong { control })?;
    Ok(String::from_utf16_lossy(&buffer[..copied]))
}

fn is_combo_error(result: LRESULT) -> bool {
    result == CB_ERR as LRESULT
}

fn fixed_wide_to_string(buffer: &[u16; LF_FACESIZE as usize]) -> String {
    let end = first_nul_or_buffer_len(buffer);
    String::from_utf16_lossy(&buffer[..end]).trim().to_owned()
}

fn first_nul_or_buffer_len(buffer: &[u16]) -> usize {
    buffer
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(buffer.len())
}

fn normalize_font_face_name(value: &str) -> String {
    value.trim().to_lowercase()
}

struct FontEnumerationDc {
    handle: HDC,
}

impl FontEnumerationDc {
    fn create() -> Result<Self, GuiError> {
        // SAFETY: passing null requests a memory DC compatible with the current screen.
        let handle = unsafe { CreateCompatibleDC(null_mut()) };
        if handle.is_null() {
            return Err(GuiError::Win32CallFailed(
                "CreateCompatibleDC(font enumeration)",
            ));
        }

        Ok(Self { handle })
    }

    fn handle(&self) -> HDC {
        self.handle
    }
}

impl Drop for FontEnumerationDc {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            // SAFETY: handle is an HDC returned by CreateCompatibleDC and owned by this wrapper.
            let _ = unsafe { DeleteDC(self.handle) };
        }
    }
}

fn window_text(hwnd: HWND, control: &'static str) -> Result<String, GuiError> {
    // SAFETY: hwnd is a live edit control created by this GUI.
    let len = unsafe { GetWindowTextLengthW(hwnd) };
    if len < 0 {
        return Err(GuiError::Win32CallFailed("GetWindowTextLengthW"));
    }

    let buffer_len = usize::try_from(len)
        .ok()
        .and_then(|len| len.checked_add(1))
        .ok_or(GuiError::WindowTextTooLong { control })?;
    let mut buffer = vec![0u16; buffer_len];
    let max_count =
        i32::try_from(buffer.len()).map_err(|_| GuiError::WindowTextTooLong { control })?;

    // SAFETY: buffer is writable for max_count UTF-16 units and hwnd is a live window handle.
    let copied = unsafe { GetWindowTextW(hwnd, buffer.as_mut_ptr(), max_count) };
    if copied < 0 {
        return Err(GuiError::Win32CallFailed("GetWindowTextW"));
    }

    let copied = usize::try_from(copied).map_err(|_| GuiError::WindowTextTooLong { control })?;
    Ok(String::from_utf16_lossy(&buffer[..copied]))
}

fn set_window_text(hwnd: HWND, text: &str) -> Result<(), GuiError> {
    let wide_text = wide_null(text);

    // SAFETY: wide_text is null-terminated and lives for the duration of the call.
    let ok = unsafe { SetWindowTextW(hwnd, wide_text.as_ptr()) };
    if ok == 0 {
        return Err(GuiError::Win32CallFailed("SetWindowTextW"));
    }

    Ok(())
}

fn show_message(owner: HWND, message: &UserMessage) {
    let title = wide_null(&message.title);
    let body = wide_null(&message.body);

    // SAFETY: title and body are null-terminated and live for the duration of the call. owner is
    // either null or a live top-level HWND owned by this GUI.
    unsafe {
        MessageBoxW(owner, body.as_ptr(), title.as_ptr(), message.icon.flags());
    }
}

fn window_state_mut(hwnd: HWND) -> Option<&'static mut WindowState> {
    // SAFETY: GWLP_USERDATA stores a Box<WindowState> pointer while the window is alive.
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut WindowState;
    if raw.is_null() {
        return None;
    }

    // SAFETY: raw is non-null and uniquely accessed on the GUI thread while handling a message.
    Some(unsafe { &mut *raw })
}

fn about_window_state_mut(hwnd: HWND) -> Option<&'static mut AboutWindowState> {
    // SAFETY: GWLP_USERDATA stores a Box<AboutWindowState> pointer while the window is alive.
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut AboutWindowState;
    if raw.is_null() {
        return None;
    }

    // SAFETY: raw is non-null and uniquely accessed on the GUI thread while handling a message.
    Some(unsafe { &mut *raw })
}

fn drop_window_state(hwnd: HWND) {
    stop_worker_completion_poll(hwnd);

    // SAFETY: GWLP_USERDATA stores a Box<WindowState> pointer while the window is alive.
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut WindowState;
    if raw.is_null() {
        return;
    }

    // SAFETY: clearing user data prevents later double drops.
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
    }

    // SAFETY: raw was allocated with Box::into_raw in handle_create and is owned by the window.
    let mut state = unsafe { Box::from_raw(raw) };
    state.wait_for_worker_before_destroy();
}

fn drop_about_window_state(hwnd: HWND) {
    // SAFETY: GWLP_USERDATA stores a Box<AboutWindowState> pointer while the window is alive.
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut AboutWindowState;
    if raw.is_null() {
        return;
    }

    // SAFETY: clearing user data prevents later double drops.
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
    }

    // SAFETY: raw was allocated with Box::into_raw in handle_about_create and is owned by the
    // About window.
    let _state = unsafe { Box::from_raw(raw) };
}

fn create_solid_brush(color: u32) -> Result<HBRUSH, GuiError> {
    // SAFETY: color is a COLORREF value.
    let brush = unsafe { CreateSolidBrush(color) };
    if brush.is_null() {
        return Err(GuiError::Win32CallFailed("CreateSolidBrush"));
    }

    Ok(brush)
}

fn delete_owned_brush(brush: HBRUSH) {
    if brush.is_null() {
        return;
    }

    // SAFETY: callers pass only brushes returned by CreateSolidBrush and still owned by them.
    let _ = unsafe { DeleteObject(brush as HGDIOBJ) };
}

fn set_dc_colors(hdc: HDC, text_color: u32, background_color: u32) {
    if hdc.is_null() {
        return;
    }

    // SAFETY: hdc is supplied by a WM_CTLCOLOR* message and remains valid during that message.
    unsafe {
        let _ = SetTextColor(hdc, text_color) != CLR_INVALID;
        let _ = SetBkColor(hdc, background_color) != CLR_INVALID;
    }
}

fn system_color(index: i32) -> u32 {
    // SAFETY: index is a documented system color index.
    unsafe { GetSysColor(index) }
}

fn system_color_brush(index: i32) -> HBRUSH {
    (index + 1) as isize as HBRUSH
}

fn loword(value: WPARAM) -> i32 {
    (value & 0xFFFF) as i32
}

fn hiword(value: WPARAM) -> u16 {
    ((value >> 16) & 0xFFFF) as u16
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::EscPosEncodingError;

    #[test]
    fn gui_contract_matches_python_window_and_defaults() {
        let defaults = PrintJobInput::default();
        let english = ui_text(UiLanguage::English);
        let korean = ui_text(UiLanguage::Korean);

        assert_eq!(WINDOW_TITLE, "ESC/POS Printer Text to Image");
        assert_eq!(APP_ICON_RESOURCE_ID, 1);
        assert_eq!(english.settings_group_label, "Printer and Font Settings");
        assert_eq!(english.text_group_label, "Print Text");
        assert_eq!(
            [
                english.ip_label,
                english.port_label,
                english.font_size_label,
                english.paper_width_label,
                english.font_face_label,
                english.theme_label,
                english.language_label
            ],
            [
                "IP/Host:",
                "Port:",
                "Font Size:",
                "Paper Width:",
                "Font:",
                "Theme:",
                "Language:"
            ]
        );
        assert_eq!(ui_theme_label(UiTheme::Light, UiLanguage::English), "Light");
        assert_eq!(ui_theme_label(UiTheme::Dark, UiLanguage::English), "Dark");
        assert_eq!(
            ui_language_label(UiLanguage::English, UiLanguage::English),
            "English"
        );
        assert_eq!(
            ui_language_label(UiLanguage::Korean, UiLanguage::English),
            "Korean"
        );
        assert_eq!(english.settings_hide_button_label, "Hide Settings");
        assert_eq!(english.settings_show_button_label, "Show Settings");
        assert_eq!(english.version_label, "Version");
        assert_eq!(english.about_link_label, "About");
        assert_eq!(english.print_button_label, "Convert to Image and Print");
        assert_eq!(
            program_version_text(english),
            format!("Version {APP_VERSION}")
        );
        assert_eq!(PROJECT_LINK_LABEL, "edgarp9/j3Ecs_NetPrint");
        assert_eq!(
            PROJECT_LINK_URL,
            "https://github.com/edgarp9/j3Ecs_NetPrint"
        );
        assert_eq!(korean.settings_group_label, "프린터 및 폰트 설정");
        assert_eq!(korean.text_group_label, "출력할 내용");
        assert_eq!(korean.version_label, "버전");
        assert_eq!(korean.about_link_label, "정보");
        assert_eq!(program_version_text(korean), format!("버전 {APP_VERSION}"));
        assert_eq!(ui_theme_label(UiTheme::Light, UiLanguage::Korean), "라이트");
        assert_eq!(ui_theme_label(UiTheme::Dark, UiLanguage::Korean), "다크");
        assert_eq!(ui_theme_from_label("라이트"), Some(UiTheme::Light));
        assert_eq!(ui_theme_from_label("다크"), Some(UiTheme::Dark));
        assert_eq!(
            ui_language_label(UiLanguage::English, UiLanguage::Korean),
            "영어"
        );
        assert_eq!(
            ui_language_label(UiLanguage::Korean, UiLanguage::Korean),
            "한글"
        );
        assert_eq!(ui_language_from_label("English"), Some(UiLanguage::English));
        assert_eq!(ui_language_from_label("영어"), Some(UiLanguage::English));
        assert_eq!(ui_language_from_label("Korean"), Some(UiLanguage::Korean));
        assert_eq!(ui_language_from_label("한글"), Some(UiLanguage::Korean));
        assert_eq!(TEXT_EDIT_FONT_FACE_NAME, "Malgun Gothic");
        assert_eq!(TEXT_EDIT_FONT_HEIGHT_PX, 16);
        assert_eq!(ABOUT_BODY_FONT_FACE_NAME, "Consolas");
        assert_eq!(ABOUT_BODY_FONT_HEIGHT_PX, 14);
        assert_eq!(ABOUT_TEXT_FILE, "about.txt");
        assert!(DEFAULT_ABOUT_TEXT.contains("GPL-3.0-or-later"));
        assert!(DEFAULT_ABOUT_TEXT.contains("LICENSE"));
        assert!(DEFAULT_ABOUT_TEXT.contains("THIRD_PARTY_NOTICES.txt"));
        assert_eq!(defaults.printer_ip, "192.168.0.1");
        assert_eq!(defaults.printer_port, "9100");
        assert_eq!(defaults.font_size_px, "42");
        assert_eq!(defaults.paper_width_px, "576");
        assert_eq!(defaults.font_face_name, "Malgun Gothic");
    }

    #[test]
    fn about_window_content_matches_required_dialog_contract() {
        assert_eq!(ABOUT_WINDOW_WIDTH, 620);
        assert_eq!(ABOUT_WINDOW_HEIGHT, 430);
        assert_eq!(ABOUT_BODY_EDIT_RECT.height, 298);

        let content = about_window_content();
        assert_eq!(content.title, "About j3Ecs NetPrint");
        assert_eq!(content.version_label, format!("{APP_NAME} {APP_VERSION}"));
        assert_eq!(content.project_url, PROJECT_LINK_URL);
        assert_eq!(content.ok_label, "OK");
        assert!(content.body_text.contains("j3Ecs NetPrint"));
        assert!(content.body_text.contains("Version: 0.2.0"));
        assert!(content.body_text.contains("GPL-3.0-or-later"));
        assert!(content.body_text.contains("WARRANTY"));
        assert!(content.body_text.contains("Full license text:\nLICENSE"));
        assert!(content.body_text.contains(PROJECT_LINK_URL));
        assert!(content.body_text.contains("THIRD_PARTY_NOTICES.txt"));
        assert!(
            content
                .body_text
                .contains("RUST_STANDARD_LIBRARY_NOTICES.html")
        );
    }

    #[test]
    fn about_text_path_is_next_to_executable() {
        let executable_path = Path::new("release").join("j3ecs-netprint.exe");
        let about_path = about_text_path_for_executable(&executable_path)
            .expect("executable with parent directory should resolve about.txt");

        assert_eq!(about_path, Path::new("release").join(ABOUT_TEXT_FILE));
        assert_eq!(
            about_text_path_for_executable(Path::new("j3ecs-netprint.exe")),
            None
        );
    }

    #[test]
    fn about_text_loader_reads_file_next_to_executable() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after Unix epoch")
            .as_nanos();
        let temp_dir = env::temp_dir().join(format!(
            "j3ecs-netprint-about-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&temp_dir).expect("test about directory should be created");
        let executable_path = temp_dir.join("j3ecs-netprint.exe");
        let about_path = temp_dir.join(ABOUT_TEXT_FILE);

        fs::write(&about_path, "disk about text")
            .expect("test about file should be writable next to executable");

        assert_eq!(
            read_about_text_for_executable(&executable_path),
            Some("disk about text".to_owned())
        );

        fs::remove_dir_all(&temp_dir).expect("test about directory should be removed");
    }

    #[test]
    fn win32_edit_multiline_text_uses_crlf_line_endings() {
        assert_eq!(
            win32_edit_multiline_text("Project\nLibrary\r\nLicense\rNotice"),
            "Project\r\nLibrary\r\nLicense\r\nNotice"
        );

        let content = about_window_content();
        let edit_text = win32_edit_multiline_text(&content.body_text);
        assert!(edit_text.contains("j3Ecs NetPrint\r\n\r\nVersion"));
        assert!(edit_text.contains("Full license text:\r\nLICENSE"));
        assert!(!edit_text.contains("j3Ecs NetPrintVersion"));
    }

    #[test]
    fn workflow_state_controls_print_button_contract() {
        let english = ui_text(UiLanguage::English);
        let korean = ui_text(UiLanguage::Korean);

        assert_eq!(PrintWorkflowState::Idle.print_button_enabled(), 1);
        assert_eq!(
            PrintWorkflowState::Idle.print_button_label(english),
            "Convert to Image and Print"
        );
        assert_eq!(
            PrintWorkflowState::Idle.print_button_label(korean),
            "이미지로 변환 및 출력"
        );
        assert!(!PrintWorkflowState::Idle.is_worker_running());

        assert_eq!(PrintWorkflowState::WorkerRunning.print_button_enabled(), 0);
        assert_eq!(
            PrintWorkflowState::WorkerRunning.print_button_label(english),
            "Printing..."
        );
        assert_eq!(
            PrintWorkflowState::WorkerRunning.print_button_label(korean),
            "출력 중..."
        );
        assert!(PrintWorkflowState::WorkerRunning.is_worker_running());
    }

    #[test]
    fn settings_panel_toggle_contract() {
        let english = ui_text(UiLanguage::English);
        let korean = ui_text(UiLanguage::Korean);
        let expanded_layout = settings_panel_layout(true);
        let collapsed_layout = settings_panel_layout(false);

        assert!(!toggled_settings_panel_visibility(true));
        assert!(toggled_settings_panel_visibility(false));
        assert_eq!(settings_toggle_button_label(true, english), "Hide Settings");
        assert_eq!(
            settings_toggle_button_label(false, english),
            "Show Settings"
        );
        assert_eq!(settings_toggle_button_label(true, korean), "설정 숨기기");
        assert_eq!(settings_toggle_button_label(false, korean), "설정 보이기");
        assert_eq!(
            expanded_layout,
            SettingsPanelLayout {
                text_group: TEXT_GROUP_EXPANDED_RECT,
                text_edit: TEXT_EDIT_EXPANDED_RECT,
            }
        );
        assert_eq!(
            collapsed_layout,
            SettingsPanelLayout {
                text_group: TEXT_GROUP_COLLAPSED_RECT,
                text_edit: TEXT_EDIT_COLLAPSED_RECT,
            }
        );
        assert!(collapsed_layout.text_group.y < expanded_layout.text_group.y);
        assert!(collapsed_layout.text_edit.height > expanded_layout.text_edit.height);
    }

    #[test]
    fn shell_execute_open_success_contract_matches_windows_api() {
        assert!(!shell_execute_open_succeeded(0));
        assert!(!shell_execute_open_succeeded(32));
        assert!(shell_execute_open_succeeded(33));
    }

    #[test]
    fn failed_completion_post_uses_poll_fallback_contract() {
        assert_eq!(
            worker_completion_post_state(0),
            WorkerCompletionPostState::FallbackPending
        );
        assert_eq!(
            worker_completion_post_state(1),
            WorkerCompletionPostState::Posted
        );
    }

    #[test]
    fn worker_completion_poll_completes_only_finished_workers() {
        assert_eq!(
            worker_completion_poll_action(true, false),
            WorkerCompletionPollAction::Wait
        );
        assert_eq!(
            worker_completion_poll_action(true, true),
            WorkerCompletionPollAction::Complete
        );
        assert_eq!(
            worker_completion_poll_action(false, true),
            WorkerCompletionPollAction::Stop
        );
        assert_eq!(
            worker_completion_poll_action(false, false),
            WorkerCompletionPollAction::Stop
        );
    }

    #[test]
    fn user_messages_cover_required_korean_validation_errors() {
        assert_user_message(
            AppError::Domain(DomainError::EmptyText),
            UiLanguage::Korean,
            "경고",
            "출력할 내용을 입력해주세요.",
        );
        assert_user_message(
            AppError::Domain(DomainError::EmptyFontFaceName),
            UiLanguage::Korean,
            "경고",
            "폰트를 선택해주세요.",
        );
        assert_user_message(
            AppError::Input(InputValidationError::NumericFieldsMustBeNumbers),
            UiLanguage::Korean,
            "입력 오류",
            "포트, 폰트 크기, 용지 폭은 숫자여야 합니다.",
        );
    }

    #[test]
    fn user_messages_cover_required_english_validation_errors() {
        assert_user_message(
            AppError::Domain(DomainError::EmptyText),
            UiLanguage::English,
            "Warning",
            "Enter text to print.",
        );
        assert_user_message(
            AppError::Domain(DomainError::EmptyFontFaceName),
            UiLanguage::English,
            "Warning",
            "Select a font.",
        );
        assert_user_message(
            AppError::Input(InputValidationError::NumericFieldsMustBeNumbers),
            UiLanguage::English,
            "Input Error",
            "Port, font size, and paper width must be numbers.",
        );
    }

    #[test]
    fn user_messages_cover_required_korean_runtime_errors() {
        let font_message = user_message_for_app_error(
            &AppError::Infra(InfraError::TextRendering {
                details: "failed to select installed font face: requested=Missing, selected=Arial"
                    .to_owned(),
            }),
            UiLanguage::Korean,
        );
        assert_eq!(font_message.title, "폰트 오류");
        assert!(font_message.body.contains("폰트를 사용할 수 없습니다."));

        let output_message = user_message_for_app_error(
            &AppError::Infra(InfraError::NetworkIo {
                target: "127.0.0.1:9100".to_owned(),
                operation: "connect",
                details: "connection refused".to_owned(),
            }),
            UiLanguage::Korean,
        );
        assert_eq!(output_message.title, "출력 실패");
        assert!(
            output_message
                .body
                .contains("프린터 통신 중 오류가 발생했습니다.")
        );

        let encoding_message = user_message_for_app_error(
            &AppError::Infra(InfraError::EscPosEncoding(
                EscPosEncodingError::InvalidImageDimensions {
                    width_px: 0,
                    height_px: 1,
                },
            )),
            UiLanguage::Korean,
        );
        assert_eq!(encoding_message.title, "출력 실패");
        assert!(
            encoding_message
                .body
                .contains("ESC/POS 출력 데이터 생성 중 오류가 발생했습니다.")
        );
    }

    #[test]
    fn user_messages_cover_required_english_runtime_errors() {
        let font_message = user_message_for_app_error(
            &AppError::Infra(InfraError::TextRendering {
                details: "failed to select installed font face: requested=Missing, selected=Arial"
                    .to_owned(),
            }),
            UiLanguage::English,
        );
        assert_eq!(font_message.title, "Font Error");
        assert!(
            font_message
                .body
                .contains("The selected font cannot be used.")
        );

        let output_message = user_message_for_app_error(
            &AppError::Infra(InfraError::NetworkIo {
                target: "127.0.0.1:9100".to_owned(),
                operation: "connect",
                details: "connection refused".to_owned(),
            }),
            UiLanguage::English,
        );
        assert_eq!(output_message.title, "Print Failed");
        assert!(
            output_message
                .body
                .contains("An error occurred while communicating with the printer.")
        );

        let encoding_message = user_message_for_app_error(
            &AppError::Infra(InfraError::EscPosEncoding(
                EscPosEncodingError::InvalidImageDimensions {
                    width_px: 0,
                    height_px: 1,
                },
            )),
            UiLanguage::English,
        );
        assert_eq!(encoding_message.title, "Print Failed");
        assert!(
            encoding_message
                .body
                .contains("An error occurred while creating ESC/POS output data.")
        );
    }

    #[test]
    fn worker_result_maps_success_to_localized_completion_message() {
        let result = worker_result_from_app_result(Ok(()), UiLanguage::Korean);

        assert_eq!(result.message.title, "성공");
        assert_eq!(result.message.body, "출력이 완료되었습니다.");

        let result = worker_result_from_app_result(Ok(()), UiLanguage::English);

        assert_eq!(result.message.title, "Success");
        assert_eq!(result.message.body, "Printing is complete.");
    }

    fn assert_user_message(
        error: AppError,
        language: UiLanguage,
        expected_title: &str,
        expected_body: &str,
    ) {
        let message = user_message_for_app_error(&error, language);

        assert_eq!(message.title, expected_title);
        assert_eq!(message.body, expected_body);
    }
}
