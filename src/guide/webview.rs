use std::path::PathBuf;
use anyhow::Result;
use crate::guide::{get_guide_html, get_asset};
use std::io::Write;
use std::fs::File;
use std::path::Path;

pub struct WebView {
    pub content_path: PathBuf,
}

impl WebView {
    pub fn new() -> Self {
        Self {
            content_path: PathBuf::from("src/guide/guide.html"),
        }
    }

    /// Prepare the guide HTML and images in temp, return the temp HTML path
    pub fn serve(&mut self) -> Option<PathBuf> {
        if let Some(html) = get_guide_html() {
            let tmp_dir = std::env::temp_dir();
            let html_path = tmp_dir.join("wake_guide.html");
            let tui_path = tmp_dir.join("tui.png");
            let web_path = tmp_dir.join("web.png");
            if let Some(tui_img) = get_asset("tui.png") {
                let mut f = File::create(&tui_path).ok()?;
                f.write_all(&tui_img).ok()?;
            }
            if let Some(web_img) = get_asset("web.png") {
                let mut f = File::create(&web_path).ok()?;
                f.write_all(&web_img).ok()?;
            }
            let html = html.replace("resources/tui.png", &tui_path.to_string_lossy())
                           .replace("resources/web.png", &web_path.to_string_lossy());
            let mut f = File::create(&html_path).ok()?;
            f.write_all(html.as_bytes()).ok()?;
            self.content_path = html_path.clone();
            Some(html_path)
        } else {
            eprintln!("Guide HTML not found in embedded assets.");
            None
        }
    }

    pub fn show(&mut self) -> Result<()> {
        // Always serve before show to ensure temp files are ready
        self.serve();
        if self.content_path.exists() {
            println!("📖 Opening guide at: {}", self.content_path.display());
            opener::open(&self.content_path)?;
            println!("✅ Guide opened in your default browser");
        } else {
            println!("❌ Guide file not found at: {}", self.content_path.display());
        }
        Ok(())
    }

    pub fn get_content(&self) -> Result<String> {
        std::fs::read_to_string(&self.content_path)
            .map_err(|e| anyhow::anyhow!("Failed to read guide content: {}", e))
    }
}
