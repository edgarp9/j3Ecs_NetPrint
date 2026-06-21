# j3Ecs NetPrint Domain Notes

## Requirements

- The application targets a Windows-native Rust implementation of the current `ecs_print_net.py` workflow.
- User input consists of printer IP/hostname, printer port, font size in pixels, paper width in pixels, installed system font face, and print text.
- The implemented Windows workflow is: validate GUI input, select an installed system font face, wrap text by measured pixel width, render a white-background/black-text bitmap, send it as an ESC/POS raster bit image over TCP, feed to the cutter position, then send a cut command.
- The Rust network output layer resolves the configured printer host with `std::net::ToSocketAddrs`, connects with `std::net::TcpStream` using the `python-escpos` default 60 second socket timeout, sends ESC/POS GS v 0 raster bit image command fragments using the `python-escpos` default 960px fragment height, sends `ESC d 6` feed before cut like `python-escpos` default `cut(feed=True)`, and then sends a GS V cut command.
- The Rust raster encoder owns the conversion that `python-escpos` performed for the original script: RGB image data is converted with Pillow-compatible luma, inverted, Floyd-Steinberg dithered to 1-bit pixels, packed MSB-first, and sent as `bitImageRaster` data.
- This Rust step keeps Windows-native Win32/GDI text measurement and RGB bitmap rendering separate from ESC/POS network transport.
- The Windows-native GUI replaces the Tkinter UI and remains a platform I/O boundary. It collects raw strings, delegates validation to the app layer, and delegates rendering/output to the existing infra flow.
- Windows release binaries use the GUI subsystem so launching the executable does not create an extra console window; debug binaries keep console diagnostics available.
- Release artifacts include `LICENSE`, `about.txt`, `SOURCE_NOTICE.md`, `THIRD_PARTY_NOTICES.txt`, and `RUST_STANDARD_LIBRARY_NOTICES.html` so the project GPL-3.0-or-later terms, About display text, corresponding source location, third-party dependency notices, and exact Rust standard library notices are distributed with the binary.
- GUI printer/layout settings and UI preferences are persisted as TOML next to the running executable.
- The Windows build embeds `icon.ico` as application icon resource ID `1`; the Win32 GUI loads that resource for the window class and applies big and small window icons. The icon notice records project-provided provenance as Google Fonts Icons from `https://fonts.google.com/icons` under SIL Open Font License 1.1.
- Long-running render/output work is dispatched to a worker thread. Completion is reported to the GUI thread with `PostMessageW`.
- The GUI emits lightweight stderr diagnostics for button clicks, validated job metadata, worker start/completion/failure, and workflow state transitions without logging the print text itself.
- The GUI lets the user show or hide the printer/font settings panel with a button; hiding the panel expands the print-text input area into the freed space and keeps current settings values available for printing.
- The GUI shows the program version and the `edgarp9/j3Ecs_NetPrint` project link at the bottom of the settings panel. Selecting the link opens `https://github.com/edgarp9/j3Ecs_NetPrint` with the Windows default browser.
- The GUI shows a localized `About` link in the settings panel. Selecting it opens a fixed-size native About window titled `About j3Ecs NetPrint`, with `j3Ecs NetPrint {version}` at the top, read-only scrollable `about.txt` text in the body, the project URL link at bottom left, and an `OK` button at bottom right. The text area displays `about.txt` loaded from the executable directory, falling back to the same text embedded at build time if the file is missing.
- The print-text editor uses a larger UI font for readability; this affects only on-screen editing and does not change receipt image rendering.
- Automated GUI-boundary tests pin the visible window contract, Python-compatible defaults, worker button state contract, and English/Korean user message mapping.

## Terms

- `NetworkPrinterTarget`: ESC/POS printer network endpoint, currently IP address or DNS-style hostname plus port.
- `TextImageLayout`: font size, paper width, margin, and installed font face name used for image rendering.
- `PrintJob`: validated settings plus non-empty print text.
- `PrintJobInput`: raw application input where numeric values are still strings.
- `Settings file`: TOML file stored in the executable directory, named after the executable stem with a `.toml` extension.
- `TextWidthMeasurer`: pure domain boundary for measuring rendered text width without depending on Win32, GDI, or a font library.
- `FallibleTextWidthMeasurer`: domain wrapping boundary for platform renderers whose width measurement can fail.
- `RenderedReceiptImage`: RGB raster image, width, and height that will be converted to ESC/POS raster output.
- `Win32GdiTextImageRenderer`: infrastructure renderer backed by `windows-sys`, installed system font selection, GDI text measurement, and a top-down 32-bit DIB section.
- `NetworkEscPosPrinter`: infrastructure printer that connects to a `NetworkPrinterTarget`, writes encoded raster bytes, and writes the cut command.
- `Python-compatible raster encoder`: infrastructure encoder that mirrors the original `python-escpos` `bitImageRaster` path for RGB image conversion and GS v 0 byte layout.
- `Mock printer`: local `TcpListener` test double that accepts one printer connection and captures transmitted bytes without requiring printer hardware.
- `Win32 GUI`: native `windows-sys` window with printer/layout inputs, print text input, and a print button.
- `UI theme`: user-selected visual theme for the Win32 GUI, currently `light` or `dark`; it does not affect receipt image rendering.
- `UI language`: user-selected display language for the Win32 GUI, currently `english` or `korean`; it does not affect receipt image rendering or printed text.
- `GS v 0 raster command`: ESC/POS raster bit image command with byte-rounded image width and 16-bit little-endian fragment dimensions.
- `Cut command`: ESC/POS command sent after image output to cut the receipt.

## Rules

- Default printer host/IP is `192.168.0.1`; default port is `9100`.
- Default font size is `42px`; default paper width is `576px`; default font face is `Malgun Gothic`.
- The settings file path is resolved from the current executable path. For `j3ecs-netprint.exe`, the settings file is `j3ecs-netprint.toml` in the same directory.
- If the settings file is missing, default settings are written to that path and used for the GUI.
- A valid print request saves the current printer host/IP, port, font size, paper width, margin, and font face name to the settings file before output starts. Print text is not persisted as a setting.
- Default UI theme is `light`.
- Default UI language is `english`.
- UI theme selection is persisted in the settings file under `[ui].theme`.
- UI language selection is persisted in the settings file under `[ui].language`.
- The GUI exposes a theme selector with light and dark options, and applying the theme updates the visible UI without changing print output colors.
- The GUI exposes a language selector with English and Korean options, and applying the language updates visible UI labels, buttons, theme labels, and user messages without changing print output.
- The GUI exposes a settings panel toggle button. The expanded state shows the printer/font settings controls; the collapsed state hides those controls, expands the print-text area upward, and keeps the toggle button visible so the panel can be shown again.
- Input text is trimmed before job creation and must not be empty.
- Font face name is trimmed before job creation and must not be empty.
- The GUI font selector is populated from fonts installed on the local Windows system.
- On Windows, the renderer creates an `HFONT` from the selected installed font face and rejects silent GDI fallback when the requested face is unavailable.
- Raw port, font size, and paper width inputs must parse as numbers before domain settings are created.
- The GUI window title is `ESC/POS Printer Text to Image`.
- The GUI window and executable use the root `icon.ico` icon through Windows resource ID `1`.
- The release helper copies the root `LICENSE`, `about.txt`, `SOURCE_NOTICE.md`, and `THIRD_PARTY_NOTICES.txt` files, plus the current Rust toolchain's standard library notice as `RUST_STANDARD_LIBRARY_NOTICES.html`, into the Cargo release output directory after a successful release build.
- The GUI exposes IP/hostname, port, font size, paper width, installed font face selection, UI theme selection, UI language selection, program version information, project link, About link, print text, and a localized print button. The default English button text is `Convert to Image and Print`.
- The About link shows the `about.txt` release notice in the About window's scrollable read-only text area. Detailed third-party license texts remain in `THIRD_PARTY_NOTICES.txt`, the project license remains in `LICENSE`, and release-time Rust standard library notices remain in `RUST_STANDARD_LIBRARY_NOTICES.html`.
- The print-text edit control uses a 16px `Malgun Gothic` UI font so the entered text is easier to read while editing.
- Empty text, empty font selection, numeric input errors, unavailable font failures, and output failures are shown to the user with `MessageBoxW` messages in the selected UI language.
- The print button is disabled while a worker print job is running and re-enabled after the completion message is handled on the GUI thread.
- The GUI state model treats `Idle` and `WorkerRunning` as explicit states. `WorkerRunning` is the only state that disables the print button.
- Printer host must be non-empty and must be either an IPv4/IPv6 literal or a DNS-style hostname; port `0` is invalid.
- Font size must be at least `5px`; paper width must be at least `100px`.
- Horizontal margins are `10px` on the left and right.
- Safe printable width is `paper_width_px - 20`; with the default paper width, safe width is `556px`.
- Safe printable width must be positive and representable.
- Text wrapping splits paragraphs on `\n`; empty paragraphs are preserved as empty output lines.
- Wrapping evaluates one Unicode scalar value at a time. For each candidate line plus next character, `TextWidthMeasurer` returns the pixel width. If that width exceeds safe width, the current line is emitted and the character starts the next line. This intentionally matches the original Python behavior, including emitting a leading empty line when the first single character is wider than the safe width.
- Rendering preserves the Python behavior: white RGB background, black text, 10px top/left/right/bottom margins, `line_spacing = font_size_px / 5`, `line_height = font_size_px + line_spacing`, and `image_height = line_count * line_height + 20`.
- ESC/POS raster output uses GS v 0 normal mode: `1D 76 30 00 xL xH yL yH` followed by packed raster bytes for each fragment.
- Rendered images taller than 960px are split into multiple independent GS v 0 fragments, matching the `python-escpos` default `image(..., fragment_height=960)` behavior.
- Raster width is rounded up to whole bytes: `width_bytes = ceil(width_px / 8)`.
- RGB pixels are converted to 1-bit raster data by the original `python-escpos`/Pillow path: compute Pillow RGB luma, invert it, apply Pillow-style integer Floyd-Steinberg error diffusion, then treat dithered non-zero pixels as black ESC/POS bits.
- Eight horizontal pixels are packed into one byte MSB-first. Padding bits beyond the image width are white.
- Cut uses the `python-escpos` default behavior: print and feed 6 lines with `ESC d 06`, then use GS V full cut `1D 56 00`.
- Network connection, stream writes, raster encoding failures, and invalid rendered image dimensions are returned as `Result` errors.
- Network output tests verify the exact transmitted byte stream with a local mock printer: raster command fragment(s) first, then feed-before-cut command, then cut command, with no retry or sleep-based masking.
