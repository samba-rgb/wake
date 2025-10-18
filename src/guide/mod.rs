use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "resources"]
pub struct Asset;

pub fn get_asset(path: &str) -> Option<Vec<u8>> {
    Asset::get(path).map(|data| data.data.into())
}

pub fn get_guide_html() -> Option<String> {
    Asset::get("guide.html").map(|data| String::from_utf8_lossy(&data.data).to_string())
}

pub mod webview;