use std::{
    error::Error,
    ffi::c_void,
    fmt,
    mem::size_of,
    ptr::{NonNull, null_mut},
    slice,
};

use windows_sys::Win32::{
    Foundation::SIZE,
    Graphics::Gdi::{
        ANTIALIASED_QUALITY, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CLIP_DEFAULT_PRECIS,
        CLR_INVALID, CreateCompatibleDC, CreateDIBSection, CreateFontW, DEFAULT_CHARSET,
        DEFAULT_PITCH, DIB_RGB_COLORS, DeleteDC, DeleteObject, FF_DONTCARE, FW_BLACK, FW_BOLD,
        FW_DEMIBOLD, FW_LIGHT, FW_NORMAL, FW_SEMIBOLD, GdiFlush, GetTextExtentPoint32W,
        GetTextFaceW, HBITMAP, HDC, HFONT, HGDIOBJ, LF_FACESIZE, OUT_DEFAULT_PRECIS, SelectObject,
        SetBkColor, SetBkMode, SetTextColor, TRANSPARENT, TextOutW,
    },
};

use crate::domain::{
    DomainError, FallibleTextWidthMeasurer, PrintJob, RenderedReceiptImage, TextImageLayout,
    TextWrapError, try_wrap_text_for_layout,
};

const BYTES_PER_BGRA_PIXEL: usize = 4;
const BYTES_PER_RGB_PIXEL: usize = 3;
const WHITE_COLORREF: u32 = 0x00FF_FFFF;
const BLACK_COLORREF: u32 = 0x0000_0000;

pub fn render_receipt_text(job: &PrintJob) -> Result<RenderedReceiptImage, GdiRenderError> {
    let context = GdiTextContext::new(&job.settings.layout)?;
    let lines = try_wrap_text_for_layout(&job.text, &job.settings.layout, &context)
        .map_err(GdiRenderError::from)?;

    render_lines(&job.settings.layout, &lines, &context)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GdiRenderError {
    Layout(DomainError),
    FontFaceUnavailable {
        requested_face: String,
        selected_face: Option<String>,
    },
    InvalidWin32Dimension {
        name: &'static str,
        value: u32,
    },
    ImageTooLarge {
        width_px: u32,
        height_px: u32,
    },
    TextTooLongForWin32 {
        utf16_units: usize,
    },
    Win32CallFailed {
        function: &'static str,
    },
}

impl From<DomainError> for GdiRenderError {
    fn from(error: DomainError) -> Self {
        Self::Layout(error)
    }
}

impl From<TextWrapError<GdiRenderError>> for GdiRenderError {
    fn from(error: TextWrapError<GdiRenderError>) -> Self {
        match error {
            TextWrapError::Layout(error) => Self::Layout(error),
            TextWrapError::Measure(error) => error,
        }
    }
}

impl fmt::Display for GdiRenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Layout(error) => write!(formatter, "{error}"),
            Self::FontFaceUnavailable {
                requested_face,
                selected_face,
            } => {
                let selected = selected_face.as_deref().unwrap_or("none");
                write!(
                    formatter,
                    "failed to select installed font face: requested={requested_face}, selected={selected}"
                )
            }
            Self::InvalidWin32Dimension { name, value } => {
                write!(
                    formatter,
                    "{name} value {value}px is too large for Win32 GDI"
                )
            }
            Self::ImageTooLarge {
                width_px,
                height_px,
            } => write!(
                formatter,
                "rendered image is too large: {width_px}x{height_px}px"
            ),
            Self::TextTooLongForWin32 { utf16_units } => write!(
                formatter,
                "text is too long for Win32 GDI call: {utf16_units} UTF-16 units"
            ),
            Self::Win32CallFailed { function } => write!(formatter, "{function} failed"),
        }
    }
}

impl Error for GdiRenderError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Layout(error) => Some(error),
            _ => None,
        }
    }
}

struct GdiTextContext {
    _font_selection: SelectedObject,
    font: OwnedFont,
    dc: MemoryDeviceContext,
}

impl GdiTextContext {
    fn new(layout: &TextImageLayout) -> Result<Self, GdiRenderError> {
        layout.validate()?;

        let requested_face = layout.font_face_name.trim();
        let dc = MemoryDeviceContext::create()?;
        let style = FontStyle::from_name(requested_face);
        let (font, font_selection) =
            create_selected_font(dc.handle(), requested_face, layout.font_size_px, style)?;

        Ok(Self {
            _font_selection: font_selection,
            font,
            dc,
        })
    }

    fn font_handle(&self) -> HFONT {
        self.font.handle()
    }

    fn measure_width_px(&self, text: &str) -> Result<u32, GdiRenderError> {
        if text.is_empty() {
            return Ok(0);
        }

        let (wide_text, text_len) = encode_utf16_for_win32(text)?;
        let mut size = SIZE::default();

        // SAFETY: self.dc and _font_selection keep a valid HDC with the requested HFONT
        // selected; wide_text points to text_len UTF-16 code units for the duration of the call.
        let ok = unsafe {
            GetTextExtentPoint32W(self.dc.handle(), wide_text.as_ptr(), text_len, &mut size)
        };

        if ok == 0 {
            return Err(GdiRenderError::Win32CallFailed {
                function: "GetTextExtentPoint32W",
            });
        }

        if size.cx < 0 {
            return Err(GdiRenderError::Win32CallFailed {
                function: "GetTextExtentPoint32W",
            });
        }

        Ok(size.cx as u32)
    }
}

impl FallibleTextWidthMeasurer for GdiTextContext {
    type Error = GdiRenderError;

    fn try_measure_text_width_px(&self, text: &str) -> Result<u32, Self::Error> {
        self.measure_width_px(text)
    }
}

fn render_lines(
    layout: &TextImageLayout,
    lines: &[String],
    context: &GdiTextContext,
) -> Result<RenderedReceiptImage, GdiRenderError> {
    let width_px = layout.paper_width_px;
    let line_spacing_px = layout.font_size_px / 5;
    let line_height_px =
        layout
            .font_size_px
            .checked_add(line_spacing_px)
            .ok_or(GdiRenderError::ImageTooLarge {
                width_px,
                height_px: u32::MAX,
            })?;
    let total_lines = u32::try_from(lines.len()).map_err(|_| GdiRenderError::ImageTooLarge {
        width_px,
        height_px: u32::MAX,
    })?;
    let content_height_px =
        line_height_px
            .checked_mul(total_lines.max(1))
            .ok_or(GdiRenderError::ImageTooLarge {
                width_px,
                height_px: u32::MAX,
            })?;
    let vertical_margin_px =
        layout
            .margin_px
            .checked_mul(2)
            .ok_or(GdiRenderError::ImageTooLarge {
                width_px,
                height_px: u32::MAX,
            })?;
    let height_px =
        content_height_px
            .checked_add(vertical_margin_px)
            .ok_or(GdiRenderError::ImageTooLarge {
                width_px,
                height_px: u32::MAX,
            })?;

    let dib = DibSection::create(width_px, height_px)?;
    let render_dc = MemoryDeviceContext::create()?;
    let _bitmap_selection = SelectedObject::select(
        render_dc.handle(),
        dib.bitmap_handle(),
        "SelectObject(HBITMAP)",
    )?;
    let _font_selection = SelectedObject::select(
        render_dc.handle(),
        context.font_handle().cast::<c_void>(),
        "SelectObject(HFONT)",
    )?;

    configure_text_dc(render_dc.handle())?;

    let x = dimension_to_i32("left margin", layout.margin_px)?;
    let mut y_px = layout.margin_px;
    for line in lines {
        if !line.is_empty() {
            draw_text_line(
                render_dc.handle(),
                x,
                dimension_to_i32("text y", y_px)?,
                line,
            )?;
        }

        y_px = y_px
            .checked_add(line_height_px)
            .ok_or(GdiRenderError::ImageTooLarge {
                width_px,
                height_px,
            })?;
    }

    // SAFETY: GdiFlush has no preconditions and flushes the calling thread's GDI batch.
    let flushed = unsafe { GdiFlush() };
    if flushed == 0 {
        return Err(GdiRenderError::Win32CallFailed {
            function: "GdiFlush",
        });
    }

    dib.to_rgb_image()
}

fn configure_text_dc(hdc: HDC) -> Result<(), GdiRenderError> {
    // SAFETY: hdc is a live memory device context owned by MemoryDeviceContext.
    let previous_background = unsafe { SetBkColor(hdc, WHITE_COLORREF) };
    if previous_background == CLR_INVALID {
        return Err(GdiRenderError::Win32CallFailed {
            function: "SetBkColor",
        });
    }

    // SAFETY: hdc is a live memory device context owned by MemoryDeviceContext.
    let previous_text = unsafe { SetTextColor(hdc, BLACK_COLORREF) };
    if previous_text == CLR_INVALID {
        return Err(GdiRenderError::Win32CallFailed {
            function: "SetTextColor",
        });
    }

    // SAFETY: hdc is a live memory device context owned by MemoryDeviceContext.
    let previous_mode = unsafe { SetBkMode(hdc, TRANSPARENT as i32) };
    if previous_mode == 0 {
        return Err(GdiRenderError::Win32CallFailed {
            function: "SetBkMode",
        });
    }

    Ok(())
}

fn draw_text_line(hdc: HDC, x: i32, y: i32, text: &str) -> Result<(), GdiRenderError> {
    let (wide_text, text_len) = encode_utf16_for_win32(text)?;

    // SAFETY: hdc is a live memory DC and wide_text contains text_len UTF-16 code units.
    let ok = unsafe { TextOutW(hdc, x, y, wide_text.as_ptr(), text_len) };
    if ok == 0 {
        return Err(GdiRenderError::Win32CallFailed {
            function: "TextOutW",
        });
    }

    Ok(())
}

fn create_selected_font(
    hdc: HDC,
    requested_face: &str,
    font_size_px: u32,
    style: FontStyle,
) -> Result<(OwnedFont, SelectedObject), GdiRenderError> {
    let font = OwnedFont::create(requested_face, font_size_px, style)?;
    let selection =
        SelectedObject::select(hdc, font.handle().cast::<c_void>(), "SelectObject(HFONT)")?;
    let actual_face = selected_text_face(hdc)?;

    if font_face_matches(&actual_face, requested_face) {
        return Ok((font, selection));
    }

    Err(GdiRenderError::FontFaceUnavailable {
        requested_face: requested_face.to_owned(),
        selected_face: Some(actual_face),
    })
}

fn selected_text_face(hdc: HDC) -> Result<String, GdiRenderError> {
    let mut buffer = [0u16; LF_FACESIZE as usize];
    let buffer_len = i32::try_from(buffer.len()).map_err(|_| GdiRenderError::Win32CallFailed {
        function: "GetTextFaceW",
    })?;

    // SAFETY: buffer is writable for buffer_len UTF-16 units and hdc is a live memory DC.
    let copied = unsafe { GetTextFaceW(hdc, buffer_len, buffer.as_mut_ptr()) };
    if copied == 0 {
        return Err(GdiRenderError::Win32CallFailed {
            function: "GetTextFaceW",
        });
    }

    let copied = usize::try_from(copied).map_err(|_| GdiRenderError::Win32CallFailed {
        function: "GetTextFaceW",
    })?;
    let end = first_nul_or_len(&buffer, copied.min(buffer.len()));

    Ok(String::from_utf16_lossy(&buffer[..end]))
}

fn font_face_matches(selected_face: &str, requested_face: &str) -> bool {
    normalize_font_name(selected_face) == normalize_font_name(requested_face)
}

struct MemoryDeviceContext {
    handle: HDC,
}

impl MemoryDeviceContext {
    fn create() -> Result<Self, GdiRenderError> {
        // SAFETY: passing a null HDC requests a memory DC compatible with the current screen.
        let handle = unsafe { CreateCompatibleDC(null_mut()) };
        if handle.is_null() {
            return Err(GdiRenderError::Win32CallFailed {
                function: "CreateCompatibleDC",
            });
        }

        Ok(Self { handle })
    }

    fn handle(&self) -> HDC {
        self.handle
    }
}

impl Drop for MemoryDeviceContext {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            // SAFETY: handle is an HDC returned by CreateCompatibleDC and owned by this wrapper.
            let _ = unsafe { DeleteDC(self.handle) };
        }
    }
}

#[derive(Debug)]
struct OwnedFont {
    handle: HFONT,
}

impl OwnedFont {
    fn create(
        face_name: &str,
        font_size_px: u32,
        style: FontStyle,
    ) -> Result<Self, GdiRenderError> {
        let font_height = dimension_to_i32("font size", font_size_px)?;
        let wide_face = wide_null_from_str(face_name);
        // SAFETY: wide_face is null-terminated and all scalar arguments are within Win32 ranges.
        let handle = unsafe {
            CreateFontW(
                -font_height,
                0,
                0,
                0,
                style.weight,
                u32::from(style.italic),
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
            return Err(GdiRenderError::Win32CallFailed {
                function: "CreateFontW",
            });
        }

        Ok(Self { handle })
    }

    fn handle(&self) -> HFONT {
        self.handle
    }
}

impl Drop for OwnedFont {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            // SAFETY: handle is an HFONT returned by CreateFontW and owned by this wrapper.
            let _ = unsafe { DeleteObject(self.handle.cast::<c_void>()) };
        }
    }
}

#[derive(Debug)]
struct OwnedBitmap {
    handle: HBITMAP,
}

impl OwnedBitmap {
    fn handle(&self) -> HBITMAP {
        self.handle
    }
}

impl Drop for OwnedBitmap {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            // SAFETY: handle is an HBITMAP returned by CreateDIBSection and owned by this wrapper.
            let _ = unsafe { DeleteObject(self.handle.cast::<c_void>()) };
        }
    }
}

struct SelectedObject {
    hdc: HDC,
    previous: HGDIOBJ,
}

impl SelectedObject {
    fn select(hdc: HDC, object: HGDIOBJ, function: &'static str) -> Result<Self, GdiRenderError> {
        // SAFETY: hdc is live and object is a live GDI object compatible with the caller's DC.
        let previous = unsafe { SelectObject(hdc, object) };
        if previous.is_null() {
            return Err(GdiRenderError::Win32CallFailed { function });
        }

        Ok(Self { hdc, previous })
    }
}

impl Drop for SelectedObject {
    fn drop(&mut self) {
        if !self.hdc.is_null() && !self.previous.is_null() {
            // SAFETY: previous is the object returned by SelectObject for this HDC.
            let _ = unsafe { SelectObject(self.hdc, self.previous) };
        }
    }
}

struct DibSection {
    bitmap: OwnedBitmap,
    bits: NonNull<u8>,
    byte_len: usize,
    width_px: u32,
    height_px: u32,
}

impl DibSection {
    fn create(width_px: u32, height_px: u32) -> Result<Self, GdiRenderError> {
        let width = dimension_to_i32("image width", width_px)?;
        let height = dimension_to_i32("image height", height_px)?;
        let pixel_count =
            checked_pixel_count(width_px, height_px).ok_or(GdiRenderError::ImageTooLarge {
                width_px,
                height_px,
            })?;
        let byte_len =
            pixel_count
                .checked_mul(BYTES_PER_BGRA_PIXEL)
                .ok_or(GdiRenderError::ImageTooLarge {
                    width_px,
                    height_px,
                })?;
        let size_image = u32::try_from(byte_len).map_err(|_| GdiRenderError::ImageTooLarge {
            width_px,
            height_px,
        })?;

        let bitmap_info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                biSizeImage: size_image,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            ..BITMAPINFO::default()
        };

        let mut bits = null_mut::<c_void>();
        // SAFETY: bitmap_info describes a top-down 32-bit BI_RGB DIB; bits is an out pointer.
        let handle = unsafe {
            CreateDIBSection(
                null_mut(),
                &bitmap_info,
                DIB_RGB_COLORS,
                &mut bits,
                null_mut(),
                0,
            )
        };

        if handle.is_null() {
            return Err(GdiRenderError::Win32CallFailed {
                function: "CreateDIBSection",
            });
        }

        let bits = NonNull::new(bits.cast::<u8>()).ok_or(GdiRenderError::Win32CallFailed {
            function: "CreateDIBSection",
        })?;

        // SAFETY: CreateDIBSection returned a writable buffer of byte_len bytes for this DIB.
        unsafe {
            std::ptr::write_bytes(bits.as_ptr(), 0xFF, byte_len);
        }

        Ok(Self {
            bitmap: OwnedBitmap { handle },
            bits,
            byte_len,
            width_px,
            height_px,
        })
    }

    fn bitmap_handle(&self) -> HGDIOBJ {
        self.bitmap.handle().cast::<c_void>()
    }

    fn to_rgb_image(&self) -> Result<RenderedReceiptImage, GdiRenderError> {
        let pixel_count = checked_pixel_count(self.width_px, self.height_px).ok_or(
            GdiRenderError::ImageTooLarge {
                width_px: self.width_px,
                height_px: self.height_px,
            },
        )?;
        let rgb_len =
            pixel_count
                .checked_mul(BYTES_PER_RGB_PIXEL)
                .ok_or(GdiRenderError::ImageTooLarge {
                    width_px: self.width_px,
                    height_px: self.height_px,
                })?;
        let mut rgb_pixels = Vec::new();
        rgb_pixels
            .try_reserve_exact(rgb_len)
            .map_err(|_| GdiRenderError::ImageTooLarge {
                width_px: self.width_px,
                height_px: self.height_px,
            })?;

        // SAFETY: bits points to this DIB section's byte_len-byte BGRA buffer.
        let bgra_pixels = unsafe { slice::from_raw_parts(self.bits.as_ptr(), self.byte_len) };
        for pixel in bgra_pixels.chunks_exact(BYTES_PER_BGRA_PIXEL) {
            rgb_pixels.push(pixel[2]);
            rgb_pixels.push(pixel[1]);
            rgb_pixels.push(pixel[0]);
        }

        Ok(RenderedReceiptImage {
            width_px: self.width_px,
            height_px: self.height_px,
            rgb_pixels,
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct FontStyle {
    weight: i32,
    italic: bool,
}

impl FontStyle {
    fn from_name(requested_name: &str) -> Self {
        let text = requested_name.to_lowercase();

        let weight = if text.contains("black") || text.contains("heavy") {
            FW_BLACK as i32
        } else if text.contains("semibold") {
            FW_SEMIBOLD as i32
        } else if text.contains("demibold") {
            FW_DEMIBOLD as i32
        } else if text.contains("bold") {
            FW_BOLD as i32
        } else if text.contains("light") {
            FW_LIGHT as i32
        } else {
            FW_NORMAL as i32
        };

        Self {
            weight,
            italic: text.contains("italic") || text.contains("oblique"),
        }
    }
}

fn normalize_font_name(name: &str) -> String {
    name.trim().to_lowercase()
}

fn encode_utf16_for_win32(text: &str) -> Result<(Vec<u16>, i32), GdiRenderError> {
    let wide_text: Vec<u16> = text.encode_utf16().collect();
    let text_len =
        i32::try_from(wide_text.len()).map_err(|_| GdiRenderError::TextTooLongForWin32 {
            utf16_units: wide_text.len(),
        })?;

    Ok((wide_text, text_len))
}

fn wide_null_from_str(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn first_nul_or_len(buffer: &[u16], max_len: usize) -> usize {
    for (index, unit) in buffer.iter().take(max_len).enumerate() {
        if *unit == 0 {
            return index;
        }
    }

    max_len
}

fn dimension_to_i32(name: &'static str, value: u32) -> Result<i32, GdiRenderError> {
    i32::try_from(value).map_err(|_| GdiRenderError::InvalidWin32Dimension { name, value })
}

fn checked_pixel_count(width_px: u32, height_px: u32) -> Option<usize> {
    let width = usize::try_from(width_px).ok()?;
    let height = usize::try_from(height_px).ok()?;
    width.checked_mul(height)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::{
        env,
        fs::{self, File},
        io::{self, Write},
        path::PathBuf,
        process,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use crate::domain::{NetworkPrinterTarget, PrintSettings};

    const SMOKE_RENDER_TEXT: &str = "Hi";
    const SMOKE_MEASURE_TEXT: &str = "Hello GDI";

    static TEST_FONT_COUNTER: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn missing_system_font_is_reported_without_fallback_font() -> Result<(), Box<dyn Error>> {
        let font_face_name = format!("J3Ecs Missing Font {}", process::id());
        let job = print_job(
            layout_with_font_face(font_face_name.clone(), 24, 180),
            SMOKE_RENDER_TEXT,
        )?;
        let error = render_receipt_text(&job)
            .expect_err("missing system font should stop rendering before font fallback");

        assert!(
            matches!(
                error,
                GdiRenderError::FontFaceUnavailable {
                    ref requested_face,
                    ..
                }
                    if requested_face == &font_face_name
            ),
            "unexpected error for missing font face: {error:?}"
        );
        Ok(())
    }

    #[test]
    fn selected_font_face_accepts_surrounding_whitespace() -> Result<(), Box<dyn Error>> {
        let Some(font_face_name) = preferred_installed_font_face() else {
            eprintln!("skipping GDI font face trim test: no preferred system font found");
            return Ok(());
        };

        let layout = layout_with_font_face(format!("  {font_face_name}  "), 24, 180);
        let context = GdiTextContext::new(&layout)?;

        assert!(
            context.measure_width_px(SMOKE_MEASURE_TEXT)? > 0,
            "trimmed installed font face should measure text"
        );
        Ok(())
    }

    #[test]
    fn renders_white_background_black_text_bitmap_with_requested_font_size_and_paper_width()
    -> Result<(), Box<dyn Error>> {
        let Some(font_face_name) = preferred_installed_font_face() else {
            eprintln!("skipping GDI render smoke test: no preferred system font found");
            return Ok(());
        };

        let small_layout = layout_with_font_face(font_face_name.clone(), 24, 180);
        let large_layout = layout_with_font_face(font_face_name, 48, 360);
        let small_context = GdiTextContext::new(&small_layout)?;
        let large_context = GdiTextContext::new(&large_layout)?;
        let small_width = small_context.measure_width_px(SMOKE_MEASURE_TEXT)?;
        let large_width = large_context.measure_width_px(SMOKE_MEASURE_TEXT)?;

        assert!(
            small_width > 0,
            "GDI should measure a positive width for smoke text"
        );
        assert!(
            large_width > small_width,
            "larger requested font_size px should measure wider text: small={small_width}, large={large_width}"
        );
        assert!(
            small_layout
                .safe_width_px()
                .is_some_and(|safe_width| small_width <= safe_width),
            "smoke text should fit on one rendered line: measured={small_width}, safe={:?}",
            small_layout.safe_width_px()
        );

        let job = print_job(small_layout.clone(), SMOKE_RENDER_TEXT)?;
        let image = render_receipt_text(&job)?;
        let stats = match validate_rendered_smoke_image(&small_layout, &image) {
            Ok(stats) => stats,
            Err(details) => {
                let dump_details = match write_debug_bmp(&image, "gdi-render-smoke-failed") {
                    Ok(path) => format!("BMP dump: {}", path.display()),
                    Err(error) => format!("BMP dump failed: {error}"),
                };
                panic!(
                    "{details}\nwidth_px={}, height_px={}, line_height_px={}\n{dump_details}",
                    image.width_px,
                    image.height_px,
                    small_layout.font_size_px + small_layout.font_size_px / 5
                );
            }
        };

        if env::var_os("J3ECS_NETPRINT_DUMP_GDI_SMOKE").is_some() {
            let path = write_debug_bmp(&image, "gdi-render-smoke")?;
            eprintln!(
                "GDI render smoke dump: {}; stats: {:?}; measured_width={small_width}",
                path.display(),
                stats
            );
        }

        Ok(())
    }

    #[derive(Debug)]
    struct ImageStats {
        non_white_pixels: usize,
        dark_pixels: usize,
        min_non_white_x: u32,
        min_non_white_y: u32,
        max_non_white_x: u32,
        max_non_white_y: u32,
    }

    fn validate_rendered_smoke_image(
        layout: &TextImageLayout,
        image: &RenderedReceiptImage,
    ) -> Result<ImageStats, String> {
        let expected_line_height = layout
            .font_size_px
            .checked_add(layout.font_size_px / 5)
            .ok_or_else(|| "line height overflowed".to_owned())?;
        let expected_height = expected_line_height
            .checked_add(
                layout
                    .margin_px
                    .checked_mul(2)
                    .ok_or_else(|| "margin height overflowed".to_owned())?,
            )
            .ok_or_else(|| "image height overflowed".to_owned())?;

        if image.width_px != layout.paper_width_px {
            return Err(format!(
                "rendered width should match paper_width: expected={}, actual={}",
                layout.paper_width_px, image.width_px
            ));
        }
        if image.height_px != expected_height {
            return Err(format!(
                "rendered height should be one line plus margins: expected={}, actual={}",
                expected_height, image.height_px
            ));
        }

        let expected_len = checked_pixel_count(image.width_px, image.height_px)
            .and_then(|pixels| pixels.checked_mul(BYTES_PER_RGB_PIXEL))
            .ok_or_else(|| "RGB buffer length overflowed".to_owned())?;
        if image.rgb_pixels.len() != expected_len {
            return Err(format!(
                "RGB buffer length mismatch: expected={}, actual={}",
                expected_len,
                image.rgb_pixels.len()
            ));
        }

        for (x, y) in [
            (0, 0),
            (image.width_px - 1, 0),
            (0, image.height_px - 1),
            (image.width_px - 1, image.height_px - 1),
        ] {
            let pixel = pixel_at(image, x, y)
                .ok_or_else(|| format!("failed to inspect corner pixel at x={x}, y={y}"))?;
            if pixel != [255, 255, 255] {
                return Err(format!(
                    "corner pixel should stay white at x={x}, y={y}: rgb={pixel:?}"
                ));
            }
        }

        let Some(stats) = image_stats(image) else {
            return Err("rendered image has no non-white text pixels".to_owned());
        };

        if stats.dark_pixels == 0 {
            return Err(format!(
                "rendered image has non-white pixels but no black/dark foreground pixels: {stats:?}"
            ));
        }

        if stats.min_non_white_x.saturating_add(2) < layout.margin_px {
            return Err(format!(
                "text starts before the expected left margin: margin={}, stats={stats:?}",
                layout.margin_px
            ));
        }
        if stats.min_non_white_y.saturating_add(2) < layout.margin_px {
            return Err(format!(
                "text starts before the expected top margin: margin={}, stats={stats:?}",
                layout.margin_px
            ));
        }

        let right_limit = layout.paper_width_px.saturating_sub(layout.margin_px);
        if stats.max_non_white_x >= right_limit {
            return Err(format!(
                "text reaches into the expected right margin: right_limit={}, stats={stats:?}",
                right_limit
            ));
        }

        let bottom_limit = image.height_px.saturating_sub(layout.margin_px / 2);
        if stats.max_non_white_y >= bottom_limit {
            return Err(format!(
                "text reaches too close to the bottom edge: bottom_limit={}, stats={stats:?}",
                bottom_limit
            ));
        }

        Ok(stats)
    }

    fn image_stats(image: &RenderedReceiptImage) -> Option<ImageStats> {
        let mut stats = None::<ImageStats>;

        for y in 0..image.height_px {
            for x in 0..image.width_px {
                let pixel = pixel_at(image, x, y)?;
                if pixel == [255, 255, 255] {
                    continue;
                }

                let next = match stats {
                    Some(mut current) => {
                        current.non_white_pixels += 1;
                        current.dark_pixels += usize::from(pixel_brightness(pixel) < 128);
                        current.min_non_white_x = current.min_non_white_x.min(x);
                        current.min_non_white_y = current.min_non_white_y.min(y);
                        current.max_non_white_x = current.max_non_white_x.max(x);
                        current.max_non_white_y = current.max_non_white_y.max(y);
                        current
                    }
                    None => ImageStats {
                        non_white_pixels: 1,
                        dark_pixels: usize::from(pixel_brightness(pixel) < 128),
                        min_non_white_x: x,
                        min_non_white_y: y,
                        max_non_white_x: x,
                        max_non_white_y: y,
                    },
                };

                stats = Some(next);
            }
        }

        stats
    }

    fn pixel_at(image: &RenderedReceiptImage, x: u32, y: u32) -> Option<[u8; 3]> {
        let width = usize::try_from(image.width_px).ok()?;
        let x = usize::try_from(x).ok()?;
        let y = usize::try_from(y).ok()?;
        let offset = y
            .checked_mul(width)?
            .checked_add(x)?
            .checked_mul(BYTES_PER_RGB_PIXEL)?;
        let pixel = image.rgb_pixels.get(offset..offset + BYTES_PER_RGB_PIXEL)?;

        Some([pixel[0], pixel[1], pixel[2]])
    }

    fn pixel_brightness(rgb: [u8; 3]) -> u16 {
        let red = u32::from(rgb[0]);
        let green = u32::from(rgb[1]);
        let blue = u32::from(rgb[2]);

        ((red * 299 + green * 587 + blue * 114) / 1000) as u16
    }

    fn write_debug_bmp(image: &RenderedReceiptImage, label: &str) -> io::Result<PathBuf> {
        let width = usize::try_from(image.width_px)
            .map_err(|_| io_invalid_data("image width does not fit usize"))?;
        let height = usize::try_from(image.height_px)
            .map_err(|_| io_invalid_data("image height does not fit usize"))?;
        let row_len = width
            .checked_mul(BYTES_PER_RGB_PIXEL)
            .ok_or_else(|| io_invalid_data("BMP row length overflowed"))?;
        let row_stride = row_len
            .checked_add(3)
            .map(|value| value / 4 * 4)
            .ok_or_else(|| io_invalid_data("BMP row stride overflowed"))?;
        let pixel_data_len = row_stride
            .checked_mul(height)
            .ok_or_else(|| io_invalid_data("BMP pixel data length overflowed"))?;
        let file_size = 14usize
            .checked_add(40)
            .and_then(|header_len| header_len.checked_add(pixel_data_len))
            .ok_or_else(|| io_invalid_data("BMP file size overflowed"))?;
        let file_size = u32::try_from(file_size)
            .map_err(|_| io_invalid_data("BMP file size does not fit u32"))?;
        let pixel_data_len = u32::try_from(pixel_data_len)
            .map_err(|_| io_invalid_data("BMP pixel data length does not fit u32"))?;
        let bmp_width = i32::try_from(image.width_px)
            .map_err(|_| io_invalid_data("BMP width does not fit i32"))?;
        let bmp_height = i32::try_from(image.height_px)
            .map_err(|_| io_invalid_data("BMP height does not fit i32"))?;

        let dump_dir = PathBuf::from("target").join("gdi-render-smoke");
        fs::create_dir_all(&dump_dir)?;
        let path = dump_dir.join(format!(
            "{label}-{}-{}.bmp",
            process::id(),
            TEST_FONT_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = File::create(&path)?;

        file.write_all(b"BM")?;
        file.write_all(&file_size.to_le_bytes())?;
        file.write_all(&[0; 4])?;
        file.write_all(&54u32.to_le_bytes())?;
        file.write_all(&40u32.to_le_bytes())?;
        file.write_all(&bmp_width.to_le_bytes())?;
        file.write_all(&bmp_height.to_le_bytes())?;
        file.write_all(&1u16.to_le_bytes())?;
        file.write_all(&24u16.to_le_bytes())?;
        file.write_all(&0u32.to_le_bytes())?;
        file.write_all(&pixel_data_len.to_le_bytes())?;
        file.write_all(&0i32.to_le_bytes())?;
        file.write_all(&0i32.to_le_bytes())?;
        file.write_all(&0u32.to_le_bytes())?;
        file.write_all(&0u32.to_le_bytes())?;

        let padding = vec![0; row_stride - row_len];
        for y in (0..height).rev() {
            let row_start = y
                .checked_mul(row_len)
                .ok_or_else(|| io_invalid_data("BMP source row offset overflowed"))?;
            let row = image
                .rgb_pixels
                .get(row_start..row_start + row_len)
                .ok_or_else(|| io_invalid_data("RGB row is out of bounds"))?;

            for pixel in row.chunks_exact(BYTES_PER_RGB_PIXEL) {
                file.write_all(&[pixel[2], pixel[1], pixel[0]])?;
            }
            file.write_all(&padding)?;
        }

        Ok(path)
    }

    fn io_invalid_data(message: &'static str) -> io::Error {
        io::Error::new(io::ErrorKind::InvalidData, message)
    }

    fn print_job(layout: TextImageLayout, text: &str) -> Result<PrintJob, DomainError> {
        PrintJob::new(
            PrintSettings {
                printer: NetworkPrinterTarget::default(),
                layout,
            },
            text,
        )
    }

    fn layout_with_font_face(
        font_face_name: String,
        font_size_px: u32,
        paper_width_px: u32,
    ) -> TextImageLayout {
        TextImageLayout {
            font_size_px,
            paper_width_px,
            margin_px: crate::domain::DEFAULT_MARGIN_PX,
            font_face_name,
        }
    }

    fn preferred_installed_font_face() -> Option<String> {
        for font_face_name in ["Arial", "Segoe UI", "Malgun Gothic", "Consolas"] {
            let layout = layout_with_font_face(font_face_name.to_owned(), 24, 180);
            if GdiTextContext::new(&layout).is_ok() {
                return Some(font_face_name.to_owned());
            }
        }

        None
    }
}
