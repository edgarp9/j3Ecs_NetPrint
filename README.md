# j3Ecs NetPrint

A Windows-native ESC/POS network receipt printing utility that converts text into a raster image and sends it directly to network printers.

This project was built as an in-house tool with AI assistance. It is useful for the workflow it was created for, but the test coverage is still limited. Please review and test carefully before using it in a production environment.

<img width="253" height="313" alt="j3Ecs_NetPrint" src="https://github.com/user-attachments/assets/331a23e8-4c2b-461f-a8ad-5e68f00beaa9" />


## Features

- Native Win32 desktop GUI written in Rust.
- Prints text to ESC/POS-compatible network printers over TCP.
- Converts input text into a black-and-white raster receipt image before printing.
- Supports configurable printer host/IP, port, font size, paper width, margin, and installed Windows font face.
- Saves printer, layout, theme, and language preferences as a TOML file next to the executable.
- Includes English and Korean UI text.
- Sends feed and cut commands after image output.

## Repository Description

Windows-native ESC/POS network receipt printer GUI that converts text to image output and sends it to TCP printers.

## Project Layout

```text
src/
  src/
    app.rs        Application workflow
    domain.rs     Validation, settings, and receipt layout rules
    infra.rs      Settings files, ESC/POS raster encoding, and network output
    platform/     Windows GUI and GDI rendering
  docs/           Domain notes and Windows GUI test procedure
  Cargo.toml      Rust package manifest
```

## Build

From the Rust project directory:

```powershell
cd src
cargo build --release
```

The release executable is created under:

```text
src/target/release/j3ecs-netprint.exe
```

## Test

From the Rust project directory:

```powershell
cd src
cargo fmt --check
cargo check
cargo test
```

The automated tests cover core validation, settings behavior, ESC/POS raster output, and GUI boundary logic. They do not replace manual testing with your actual printer model, network, font selection, and receipt layout.

## Usage

1. Start `j3ecs-netprint.exe`.
2. Enter the printer host/IP and port.
3. Choose the receipt font, font size, and paper width.
4. Enter the text to print.
5. Click `Convert to Image and Print`.

The app renders the text into an image, sends the ESC/POS raster data to the configured network printer, feeds the receipt, and sends the cut command.

## License

This project is distributed under the license included in [LICENSE](LICENSE).

## Third-Party Notices

This project uses icons from [Google Fonts Icons](https://fonts.google.com/icons), also known as Material Symbols / Material Icons. Google makes these icons available under the [Apache License Version 2.0](https://www.apache.org/licenses/LICENSE-2.0).

Thank you to Google and the Material Symbols / Material Icons contributors for making these icons available.
