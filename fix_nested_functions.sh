#!/bin/bash

# Fix the main render function
sed -i 's/fn render(fn render(&mut self, f: &mut Frame)mut self, f: fn render(&mut self, f: &mut Frame)mut Frame<'\''_>)/fn render(\&mut self, f: \&mut Frame<'\''_>)/' src/ui/app.rs

# Fix the render_help function
sed -i 's/fn render_help(fn render_help(&self, f: &mut Frame, area: Rect)self, f: fn render_help(&self, f: &mut Frame, area: Rect)mut Frame<'\''_>, area: ratatui::layout::Rect)/fn render_help(\&self, f: \&mut Frame<'\''_>, area: ratatui::layout::Rect)/' src/ui/app.rs

# Fix the render_tabs function
sed -i 's/fn render_tabs(fn render_tabs(&self, f: &mut Frame, area: Rect)self, f: fn render_tabs(&self, f: &mut Frame, area: Rect)mut Frame<'\''_>, area: ratatui::layout::Rect)/fn render_tabs(\&self, f: \&mut Frame<'\''_>, area: ratatui::layout::Rect)/' src/ui/app.rs

# Fix the render_overview function
sed -i 's/fn render_overview(fn render_overview(&self, f: &mut Frame, area: Rect)self, f: fn render_overview(&self, f: &mut Frame, area: Rect)mut Frame<'\''_>, area: ratatui::layout::Rect)/fn render_overview(\&self, f: \&mut Frame<'\''_>, area: ratatui::layout::Rect)/' src/ui/app.rs

# Fix the render_cpu function
sed -i 's/fn render_cpu(fn render_cpu(&self, f: &mut Frame, area: Rect)self, f: fn render_cpu(&self, f: &mut Frame, area: Rect)mut Frame<'\''_>, area: ratatui::layout::Rect)/fn render_cpu(\&self, f: \&mut Frame<'\''_>, area: ratatui::layout::Rect)/' src/ui/app.rs

# Fix the render_memory function
sed -i 's/fn render_memory(fn render_memory(&self, f: &mut Frame, area: Rect)self, f: fn render_memory(&self, f: &mut Frame, area: Rect)mut Frame<'\''_>, area: ratatui::layout::Rect)/fn render_memory(\&self, f: \&mut Frame<'\''_>, area: ratatui::layout::Rect)/' src/ui/app.rs

# Fix the render_network function
sed -i 's/fn render_network(fn render_network(&self, f: &mut Frame, area: Rect)self, f: fn render_network(&self, f: &mut Frame, area: Rect)mut Frame<'\''_>, area: ratatui::layout::Rect)/fn render_network(\&self, f: \&mut Frame<'\''_>, area: ratatui::layout::Rect)/' src/ui/app.rs

# Fix the render_disk function
sed -i 's/fn render_disk(fn render_disk(&self, f: &mut Frame, area: Rect)self, f: fn render_disk(&self, f: &mut Frame, area: Rect)mut Frame<'\''_>, area: ratatui::layout::Rect)/fn render_disk(\&self, f: \&mut Frame<'\''_>, area: ratatui::layout::Rect)/' src/ui/app.rs

# Fix the render_status_bar function
sed -i 's/fn render_status_bar(fn render_status_bar(&self, f: &mut Frame, area: Rect)self, f: fn render_status_bar(&self, f: &mut Frame, area: Rect)mut Frame<'\''_>, area: ratatui::layout::Rect)/fn render_status_bar(\&self, f: \&mut Frame<'\''_>, area: ratatui::layout::Rect)/' src/ui/app.rs
