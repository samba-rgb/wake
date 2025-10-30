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
            let icon_path = tmp_dir.join("wakeicon.png");
            if let Some(tui_img) = get_asset("tui.png") {
                let mut f = File::create(&tui_path).ok()?;
                f.write_all(&tui_img).ok()?;
            }
            if let Some(web_img) = get_asset("web.png") {
                let mut f = File::create(&web_path).ok()?;
                f.write_all(&web_img).ok()?;
            }
            if let Some(icon_img) = get_asset("wakeicon.png") {
                let mut f = File::create(&icon_path).ok()?;
                f.write_all(&icon_img).ok()?;
            }
            // Use only the file name for image src in HTML
            let html = html.replace("tui.png", "tui.png")
                           .replace("web.png", "web.png")
                           .replace("wakeicon.png", "wakeicon.png");
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
        // Try the online GitHub Pages URL first. If opening it fails, fall back to the local embedded guide.
        // Open the site root (simple domain) instead of the /guide.html path
        let online_url = "https://samba-rgb.github.io/wake/";

        // Simply try to open the online URL. If opener::open returns an error, we'll fall back.
        println!("📖 Attempting to open online guide: {}", online_url);
        if opener::open(online_url).is_ok() {
            println!("✅ Online guide opened in your default browser");
            return Ok(());
        }

        // Fallback: serve local embedded guide and open it
        if let Some(path) = self.serve() {
            if path.exists() {
                println!("📖 Opening local guide at: {}", path.display());
                opener::open(&path)?;
                println!("✅ Local guide opened in your default browser");
                return Ok(());
            }
        }

        eprintln!("❌ Guide not available online or locally.");
        Ok(())
    }

    pub fn get_content(&self) -> Result<String> {
        std::fs::read_to_string(&self.content_path)
            .map_err(|e| anyhow::anyhow!("Failed to read guide content: {}", e))
    }
}
