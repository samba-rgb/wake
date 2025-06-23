use cursive::views::{EditView, LinearLayout, Panel, TextView};
use cursive::traits::*;
use cursive::View;
use cursive::event::{Event, EventResult};
use tracing::{debug, error};

pub struct FilterPanel {
    current_filter: String,
    filter_callback: Option<Box<dyn Fn(&str) + Send + Sync>>,
    is_editing: bool,
    default_filter: String,
}

impl FilterPanel {
    pub fn new() -> Self {
        Self {
            current_filter: String::new(),
            filter_callback: None,
            is_editing: false,
            default_filter: String::new(),
        }
    }

    pub fn new_with_default(default_filter: String) -> Self {
        Self {
            current_filter: default_filter.clone(),
            filter_callback: None,
            is_editing: false,
            default_filter,
        }
    }

    pub fn set_filter_callback<F>(&mut self, callback: F) 
    where 
        F: Fn(&str) + Send + Sync + 'static 
    {
        self.filter_callback = Some(Box::new(callback));
    }

    pub fn get_current_filter(&self) -> &str {
        &self.current_filter
    }

    pub fn is_editing(&self) -> bool {
        self.is_editing
    }

    pub fn start_editing(&mut self) {
        debug!("FilterPanel: Starting edit mode");
        self.is_editing = true;
        // Trigger a UI rebuild to show the EditView
    }

    pub fn stop_editing(&mut self) {
        debug!("FilterPanel: Stopping edit mode");
        self.is_editing = false;
        // Trigger a UI rebuild to show the read-only view
    }

    pub fn clear_filter(&mut self) {
        self.current_filter.clear();
        if let Some(ref callback) = self.filter_callback {
            callback("");
        }
    }

    fn apply_filter(&mut self, filter_text: &str) {
        self.current_filter = filter_text.to_string();
        debug!("Applying filter: '{}'", filter_text);
        
        if let Some(ref callback) = self.filter_callback {
            callback(filter_text);
        } else {
            error!("No filter callback set");
        }
    }

    pub fn build_view(&self) -> impl View {
        if !self.is_editing {
            // Show read-only filter display
            LinearLayout::vertical()
                .child(TextView::new("Include Filter:"))
                .child(
                    TextView::new(if self.current_filter.is_empty() { 
                        "[none] - Press 'i' to edit" 
                    } else { 
                        &self.current_filter 
                    })
                    .style(cursive::theme::ColorStyle::secondary())
                    .with_name("filter_display")
                )
        } else {
            // Show editable filter input
            LinearLayout::vertical()
                .child(TextView::new("Include Filter (Enter to apply, Esc to finish):"))
                .child(
                    EditView::new()
                        .content(self.current_filter.clone())
                        .on_submit(|s, text| {
                            s.call_on_name("filter_panel", |panel: &mut FilterPanel| {
                                panel.apply_filter(text);
                                panel.stop_editing();
                            });
                        })
                        .on_edit(|s, text, _cursor| {
                            // Real-time filtering as user types
                            s.call_on_name("filter_panel", |panel: &mut FilterPanel| {
                                panel.apply_filter(text);
                            });
                        })
                        .with_name("filter_input")
                        .fixed_width(50)
                )
                .child(TextView::new("Enter: Apply & Exit | Esc: Exit editing"))
        }
    }
}

impl View for FilterPanel {
    fn draw(&self, printer: &cursive::Printer) {
        // FIXED: Always rebuild view based on current state
        let view = if self.is_editing {
            // Show editable filter input
            LinearLayout::vertical()
                .child(TextView::new("Include Filter (Enter to apply, Esc to finish):"))
                .child(
                    EditView::new()
                        .content(self.current_filter.clone())
                        .on_submit(|s, text| {
                            s.call_on_name("filter_panel", |panel: &mut FilterPanel| {
                                panel.apply_filter(text);
                                panel.stop_editing();
                            });
                        })
                        .on_edit(|s, text, _cursor| {
                            // Real-time filtering as user types
                            s.call_on_name("filter_panel", |panel: &mut FilterPanel| {
                                panel.apply_filter(text);
                            });
                        })
                        .with_name("filter_input")
                        .fixed_width(50)
                )
                .child(TextView::new("Enter: Apply & Exit | Esc: Exit editing"))
        } else {
            // Show read-only filter display
            LinearLayout::vertical()
                .child(TextView::new("Include Filter:"))
                .child(
                    TextView::new(if self.current_filter.is_empty() { 
                        "[none] - Press 'i' to edit" 
                    } else { 
                        &self.current_filter 
                    })
                    .style(cursive::theme::ColorStyle::secondary())
                    .with_name("filter_display")
                )
        };
        
        let panel = Panel::new(view)
            .title("Filter")
            .title_position(cursive::align::HAlign::Left);
        
        panel.draw(printer);
    }

    fn on_event(&mut self, event: Event) -> EventResult {
        match event {
            Event::Key(cursive::event::Key::Esc) => {
                if self.is_editing {
                    self.stop_editing();
                    EventResult::Consumed(None)
                } else {
                    self.clear_filter();
                    EventResult::Consumed(None)
                }
            }
            Event::Char('i') if !self.is_editing => {
                self.start_editing();
                EventResult::Consumed(None)
            }
            _ => EventResult::Ignored,
        }
    }

    fn required_size(&mut self, constraint: cursive::Vec2) -> cursive::Vec2 {
        // FIXED: Always recalculate size based on current state
        let mut view = if self.is_editing {
            LinearLayout::vertical()
                .child(TextView::new("Include Filter (Enter to apply, Esc to finish):"))
                .child(EditView::new().fixed_width(50))
                .child(TextView::new("Enter: Apply & Exit | Esc: Exit editing"))
        } else {
            LinearLayout::vertical()
                .child(TextView::new("Include Filter:"))
                .child(TextView::new("[none] - Press 'i' to edit"))
        };
        
        view.required_size(constraint)
    }

    fn layout(&mut self, _size: cursive::Vec2) {
        // Layout is handled by the dynamically created view in draw()
        // This ensures proper refresh when editing state changes
    }
}