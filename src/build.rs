use std::error::Error;

const APP_ICON_RESOURCE_ID: &str = "1";
const APP_ICON_PATH: &str = "icon.ico";

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed={APP_ICON_PATH}");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon_with_id(APP_ICON_PATH, APP_ICON_RESOURCE_ID);
        resource.compile()?;
    }

    Ok(())
}
