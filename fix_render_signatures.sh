#!/bin/bash

# Edit app.rs to fix the render method signatures
sed -i 's/fn render(&mut self, f: &mut Frame)/fn render(&mut self, f: &mut Frame<'\''_>)/' src/ui/app.rs
sed -i 's/fn render_help(&self, f: &mut Frame, area: Rect)/fn render_help(&self, f: &mut Frame<'\''_>, area: ratatui::layout::Rect)/' src/ui/app.rs
sed -i 's/fn render_tabs(&self, f: &mut Frame, area: Rect)/fn render_tabs(&self, f: &mut Frame<'\''_>, area: ratatui::layout::Rect)/' src/ui/app.rs
sed -i 's/fn render_overview(&self, f: &mut Frame, area: Rect)/fn render_overview(&self, f: &mut Frame<'\''_>, area: ratatui::layout::Rect)/' src/ui/app.rs
sed -i 's/fn render_cpu(&self, f: &mut Frame, area: Rect)/fn render_cpu(&self, f: &mut Frame<'\''_>, area: ratatui::layout::Rect)/' src/ui/app.rs
sed -i 's/fn render_memory(&self, f: &mut Frame, area: Rect)/fn render_memory(&self, f: &mut Frame<'\''_>, area: ratatui::layout::Rect)/' src/ui/app.rs
sed -i 's/fn render_network(&self, f: &mut Frame, area: Rect)/fn render_network(&self, f: &mut Frame<'\''_>, area: ratatui::layout::Rect)/' src/ui/app.rs
sed -i 's/fn render_disk(&self, f: &mut Frame, area: Rect)/fn render_disk(&self, f: &mut Frame<'\''_>, area: ratatui::layout::Rect)/' src/ui/app.rs
sed -i 's/fn render_status_bar(&self, f: &mut Frame, area: Rect)/fn render_status_bar(&self, f: &mut Frame<'\''_>, area: ratatui::layout::Rect)/' src/ui/app.rs
