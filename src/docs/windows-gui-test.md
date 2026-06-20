# Windows GUI Test Procedure

## Automated Checks

Run these from the repository root on Windows:

```powershell
cargo fmt --check
cargo check
cargo test
cargo build
```

`cargo test` includes GUI-boundary tests for:

- app workflow order: render text image, then send that image to the configured printer target with cut
- Python-compatible ESC/POS raster packing, including Pillow-style grayscale dithering for antialiased pixels
- window title and default input values
- default English labels, Korean labels, and localized print button text
- program version text and the `https://github.com/edgarp9` project link
- `Idle`/`WorkerRunning` button state behavior
- English and Korean user messages for empty text, empty font selection, numeric input errors, font errors, output errors, and ESC/POS encoding errors

## Manual Smoke Test

1. Start `target\debug\j3ecs-netprint.exe`.
2. Confirm the window title is `ESC/POS Printer Text to Image`.
3. Confirm these fields and defaults:
   - IP/Host: `192.168.0.1`
   - Port: `9100`
   - Font Size: `42`
   - Paper Width: `576`
   - Font: `Malgun Gothic` or the first available fallback selected from installed system fonts
   - Theme: `Light`
   - Language: `English`
   - Version: the package version from `Cargo.toml`
   - Project link: `https://github.com/edgarp9`
4. Click `https://github.com/edgarp9` and confirm the default browser opens that URL.
5. Click `Convert to Image and Print` with empty text and confirm the English warning `Enter text to print.`.
6. Change Language to `Korean` and confirm labels switch to Korean, including `IP/호스트`, `폰트 크기`, `용지 폭`, `테마`, `언어`, `버전`, and the `이미지로 변환 및 출력` button.
7. Click `이미지로 변환 및 출력` with empty text and confirm the Korean warning `출력할 내용을 입력해주세요.`.
8. Enter text, confirm the font dropdown contains installed system fonts, and choose a different font.
9. Enter a non-numeric value into Port, font size, or paper width using an automation tool or paste path if the edit control blocks typing, then confirm the selected-language numeric error message.
10. Change the font size value and confirm the selected size is persisted after a valid print attempt.
11. Set IP/host and Port to an unreachable printer target, click the button with valid text, and confirm a selected-language network failure message.
12. While output is running, confirm the button changes to `Printing...` in English or `출력 중...` in Korean and the UI remains responsive. Completion must restore the localized print button text.

Diagnostic progress is printed to stderr with the `[j3ecs-netprint gui]` prefix. It includes state transitions and worker outcome, but not the print body.
