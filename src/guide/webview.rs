use std::path::PathBuf;
use anyhow::Result;

pub struct WebView {
    pub content_path: PathBuf,
}

impl WebView {
    pub fn new() -> Self {
        Self {
            content_path: PathBuf::from("src/guide/guide.html"),
        }
    }

    pub fn show(&self) -> Result<()> {
        if self.content_path.exists() {
            println!("📖 Opening guide at: {}", self.content_path.display());
            std::process::Command::new("xdg-open")
                .arg(&self.content_path)
                .spawn()?;
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
