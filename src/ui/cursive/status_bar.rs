use cursive::views::{LinearLayout, TextView};
use cursive::traits::*;
use cursive::utils::markup::StyledString;
use cursive::theme::{Color, Effect};
use cursive::Cursive;

pub struct StatusBar {
    layout: LinearLayout,
    mode_display: String,
    follow_mode: bool,
    log_count: usize,
    scroll_info: String,
    memory_usage: f64,
    filter_active: bool,
}

#[derive(Debug, Clone)]
pub struct StatusInfo {
    pub mode: String,
    pub follow_mode: bool,
    pub log_count: usize,
    pub scroll_current: usize,
    pub scroll_total: usize,
    pub memory_usage: f64,
    pub filter_active: bool,
}

impl StatusBar {
    pub fn new() -> Self {
        let layout = LinearLayout::horizontal()
            .child(TextView::new("").with_name("status_left"))
            .child(cursive::views::DummyView.full_width())
            .child(TextView::new("").with_name("status_right"));
        
        Self {
            layout,
            mode_display: "NORMAL".to_string(),
            follow_mode: true,
            log_count: 0,
            scroll_info: "0/0".to_string(),
            memory_usage: 0.0,
            filter_active: false,
        }
    }
    
    pub fn update_mode(&mut self, mode: &str) {
        self.mode_display = mode.to_string();
    }
    
    pub fn update_follow_mode(&mut self, follow: bool) {
        self.follow_mode = follow;
        self.refresh_status();
    }
    
    pub fn update_log_count(&mut self, count: usize) {
        self.log_count = count;
    }
    
    pub fn update_scroll_info(&mut self, current: usize, total: usize) {
        self.scroll_info = format!("{}/{}", current, total);
    }
    
    pub fn update_memory_usage(&mut self, usage_percent: f64) {
        self.memory_usage = usage_percent;
    }
    
    pub fn update_filter_status(&mut self, active: bool) {
        self.filter_active = active;
    }
    
    pub fn update_status_info(&mut self, info: StatusInfo) {
        self.mode_display = info.mode;
        self.follow_mode = info.follow_mode;
        self.log_count = info.log_count;
        self.scroll_info = format!("{}/{}", info.scroll_current, info.scroll_total);
        self.memory_usage = info.memory_usage;
        self.filter_active = info.filter_active;
    }
    
    pub fn refresh_display(&self, siv: &mut Cursive) {
        // Update left side status
        let left_content = self.create_left_status();
        siv.call_on_name("status_left", |view: &mut TextView| {
            view.set_content(left_content);
        });
        
        // Update right side status 
        let right_content = self.create_right_status();
        siv.call_on_name("status_right", |view: &mut TextView| {
            view.set_content(right_content);
        });
    }
    
    fn create_left_status(&self) -> StyledString {
        let mut content = StyledString::new();
        
        // Mode indicator with color
        match self.mode_display.as_str() {
            "NORMAL" => content.append_styled("NORMAL", Color::Dark(cursive::theme::BaseColor::Green)),
            "FOLLOW" => content.append_styled("FOLLOW", Color::Dark(cursive::theme::BaseColor::Blue)),
            "PAUSED" => content.append_styled("PAUSED", Color::Dark(cursive::theme::BaseColor::Yellow)),
            _ => content.append_plain(&self.mode_display),
        }
        
        content.append_plain(" | ");
        
        // Follow mode indicator
        if self.follow_mode {
            content.append_styled("FOLLOW", Color::Dark(cursive::theme::BaseColor::Blue));
        } else {
            content.append_styled("MANUAL", Color::Dark(cursive::theme::BaseColor::White));
        }
        
        content.append_plain(" | ");
        
        // Log count with formatting
        let count_text = if self.log_count < 1000 {
            format!("{} logs", self.log_count)
        } else if self.log_count < 1_000_000 {
            format!("{:.1}K logs", self.log_count as f64 / 1000.0)
        } else {
            format!("{:.1}M logs", self.log_count as f64 / 1_000_000.0)
        };
        content.append_plain(&count_text);
        
        content.append_plain(" | ");
        
        // Scroll position
        content.append_plain(&format!("Scroll: {}", self.scroll_info));
        
        // Filter indicator
        if self.filter_active {
            content.append_plain(" | ");
            content.append_styled("FILTER", Color::Dark(cursive::theme::BaseColor::Cyan));
        }
        
        // Memory warning
        if self.memory_usage > 80.0 {
            content.append_plain(" | ");
            content.append_styled(
                &format!("MEM: {:.1}%", self.memory_usage),
                if self.memory_usage > 95.0 {
                    Color::Dark(cursive::theme::BaseColor::Red)
                } else {
                    Color::Dark(cursive::theme::BaseColor::Yellow)
                }
            );
        }
        
        content
    }
    
    fn create_right_status(&self) -> StyledString {
        let mut content = StyledString::new();
        
        // Help shortcuts - color-coded
        content.append_styled("h", Effect::Bold);
        content.append_plain(":Help ");
        
        content.append_styled("q", Effect::Bold);
        content.append_plain(":Quit ");
        
        content.append_styled("f", Effect::Bold);
        content.append_plain(":Follow ");
        
        content.append_styled("i/e", Effect::Bold);
        content.append_plain(":Filter ");
        
        content.append_styled("Ctrl+C", Effect::Bold);
        content.append_plain(":Copy");
        
        content
    }
    
    pub fn show_memory_warning(&self, siv: &mut Cursive, usage_percent: f64) {
        let warning_msg = if usage_percent > 95.0 {
            format!("CRITICAL: Memory usage at {:.1}%! Consider reducing log buffer size.", usage_percent)
        } else {
            format!("WARNING: High memory usage at {:.1}%", usage_percent)
        };
        
        // Create a popup dialog for memory warnings
        siv.add_layer(
            cursive::views::Dialog::info(warning_msg)
                .title("Memory Warning")
                .button("OK", |s| { s.pop_layer(); })
        );
    }
    
    pub fn show_help_dialog(siv: &mut Cursive) {
        let help_text = r#"Wake - Kubernetes Log Viewer

NAVIGATION:
  ↑/↓       Navigate log entries
  PgUp/PgDn Page up/down
  Home/End  Go to top/bottom
  
FILTERING:
  i         Focus include filter
  e         Focus exclude filter
  Ctrl+U    Clear all filters
  Esc       Return to log view
  
MODES:
  f         Toggle follow mode
  Space     Pause/resume streaming
  
COPY/EXPORT:
  Ctrl+C    Copy selected entry
  Ctrl+A    Copy all visible logs
  
OTHER:
  h         Show this help
  q         Quit application
  
TIPS:
- Use regex patterns in filters
- Follow mode auto-scrolls to new logs
- Memory warnings appear at >80% usage"#;
        
        siv.add_layer(
            cursive::views::Dialog::text(help_text)
                .title("Help - Keyboard Shortcuts")
                .button("OK", |s| { s.pop_layer(); })
                .min_width(60)
        );
    }
    
    pub fn setup_global_shortcuts(siv: &mut Cursive) {
        // Help dialog
        siv.add_global_callback('h', |s| {
            Self::show_help_dialog(s);
        });
        
        // Quit application
        siv.add_global_callback('q', |s| {
            s.quit();
        });
        
        // Toggle follow mode
        siv.add_global_callback('f', |_s| {
            // Toggle follow mode - handled by the main app
            tracing::info!("Follow mode toggle requested");
        });
        
        // Space bar for pause/resume
        siv.add_global_callback(' ', |_s| {
            // Pause/resume functionality - handled by the main app
            tracing::info!("Pause/resume requested");
        });
    }
    
    fn refresh_status(&mut self) {
        // Update the layout with current status
        let follow_status = if self.follow_mode { "FOLLOW" } else { "PAUSED" };
        let status_text = format!("Mode: {} | Logs: {} | Memory: {:.1}%", 
                                follow_status, self.log_count, self.memory_usage);
        
        // Update the layout (this is a simplified approach)
        self.layout = LinearLayout::horizontal()
            .child(TextView::new(status_text));
    }
}

impl cursive::view::View for StatusBar {
    fn draw(&self, printer: &cursive::Printer) {
        // Create a comprehensive status line
        let follow_status = if self.follow_mode { "FOLLOW" } else { "PAUSED" };
        let memory_status = if self.memory_usage > 80.0 {
            format!(" | MEM: {:.1}%", self.memory_usage)
        } else {
            String::new()
        };
        
        let status_text = format!(
            "Mode: {} | Logs: {} | {} | q:Quit i:Filter f:Follow h:Help{}",
            follow_status,
            self.log_count,
            if self.filter_active { "FILTERED" } else { "ALL" },
            memory_status
        );
        
        let status_view = TextView::new(status_text)
            .style(cursive::theme::ColorStyle::primary());
        
        status_view.draw(printer);
    }
    
    fn on_event(&mut self, _event: cursive::event::Event) -> cursive::event::EventResult {
        // Status bar doesn't handle events directly
        cursive::event::EventResult::Ignored
    }
    
    fn required_size(&mut self, constraint: cursive::Vec2) -> cursive::Vec2 {
        // Single line height
        cursive::Vec2::new(constraint.x, 1)
    }
}