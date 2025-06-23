# Wake UI Revamp: Ratatui to Cursive Migration

**Date:** June 19, 2025  
**Version:** 1.0  
**Status:** Design Phase  

## Executive Summary

This document outlines the complete redesign of Wake's terminal user interface, migrating from ratatui to Cursive framework. The primary goals are to achieve native copy-paste support, superior scrolling performance, and reduced code complexity while maintaining all existing functionality.

## Current State Analysis

### Existing Architecture Problems
- **Complex Selection Logic**: 1,600+ lines of custom text selection implementation
- **Manual Scroll Management**: Buggy scroll validation causing position jumps
- **External Clipboard Integration**: arboard dependency with manual state management
- **Performance Issues**: High memory usage (18MB) and CPU overhead (13-15%)
- **Mouse Event Complexity**: 1,000+ lines of manual mouse handling

### Current Component Structure
```
src/ui/
├── app.rs          (580 lines) - Main event loop and coordination
├── display.rs      (1,650 lines) - Complex rendering and state management
├── input.rs        (350 lines) - Input handling and key mapping
├── filter_manager.rs (200 lines) - Filter state management
└── mod.rs          (20 lines) - Module exports
```

## Target Architecture

### New Component Design
```
src/ui/
├── cursive_app.rs     (300 lines) - Cursive application setup
├── log_view.rs        (400 lines) - Log display widget
├── filter_panel.rs    (200 lines) - Filter input widgets
├── status_bar.rs      (150 lines) - Status display widget
├── event_handler.rs   (250 lines) - Event coordination
├── theme.rs           (100 lines) - Color and styling
└── mod.rs             (50 lines) - Module exports and public API
```

### Dependencies Changes
```toml
# Remove these dependencies from Cargo.toml
# ratatui = "0.26.0"
# crossterm = "0.27.0" 
# arboard = "3.5.0"

# Add these dependencies to Cargo.toml
cursive = { version = "0.21", features = ["crossterm-backend"] }
cursive_table_view = "0.14"
cursive_buffered_backend = "0.6"
```

## Implementation Design

### Phase 1: Core Infrastructure (Week 1)

#### 1.1 Cursive Application Bootstrap
**File:** `src/ui/cursive_app.rs`

```rust
pub struct WakeApp {
    siv: Cursive,
    log_data: Arc<Mutex<LogBuffer>>,
    filter_manager: FilterManager,
    config: AppConfig,
}

impl WakeApp {
    pub fn new(args: Args) -> Self;
    pub fn setup_ui(&mut self);
    pub fn run(&mut self) -> Result<()>;
    pub fn handle_log_entry(&mut self, entry: LogEntry);
}
```

**Key Features:**
- Cursive instance management
- Global callback registration
- Theme initialization
- Backend configuration

#### 1.2 Log Buffer Management
**File:** `src/ui/log_view.rs`

```rust
pub struct LogBuffer {
    entries: VecDeque<LogEntry>,
    max_size: usize,
    filtered_view: Vec<usize>,
    current_filter: Option<FilterCriteria>,
}

pub struct LogTableView {
    table: TableView<LogEntry, LogColumn>,
    buffer_ref: Arc<Mutex<LogBuffer>>,
}
```

**Features:**
- Efficient memory management
- Built-in scrolling with cursive_table_view
- Native text selection and copy-paste
- Automatic bounds checking

### Phase 2: UI Components (Week 2)

#### 2.1 Filter Panel Implementation
**File:** `src/ui/filter_panel.rs`

```rust
pub struct FilterPanel {
    include_input: EditView,
    exclude_input: EditView,
    layout: LinearLayout,
}

impl FilterPanel {
    pub fn new() -> Self;
    pub fn setup_callbacks(&mut self, app_sink: &Sender<AppEvent>);
    pub fn update_filter(&mut self, filter_type: FilterType, pattern: String);
    pub fn get_current_filters(&self) -> (String, String);
}
```

**Features:**
- Native text editing with EditView
- Real-time filter validation
- History support with up/down arrows
- Syntax highlighting for regex patterns

#### 2.2 Status Bar Widget
**File:** `src/ui/status_bar.rs`

```rust
pub struct StatusBar {
    mode_indicator: TextView,
    scroll_info: TextView,
    memory_usage: TextView,
    help_text: TextView,
}

impl StatusBar {
    pub fn update_status(&mut self, status: AppStatus);
    pub fn show_memory_warning(&mut self, usage_percent: f64);
    pub fn update_scroll_info(&mut self, current: usize, total: usize);
}
```

#### 2.3 Event Coordination
**File:** `src/ui/event_handler.rs`

```rust
pub enum AppEvent {
    NewLogEntry(LogEntry),
    FilterUpdate { include: String, exclude: String },
    ScrollToTop,
    ScrollToBottom,
    ToggleFollow,
    CopySelection,
    ShowHelp,
    Quit,
}

pub struct EventHandler {
    sender: Sender<AppEvent>,
    receiver: Receiver<AppEvent>,
}
```

### Phase 3: Advanced Features (Week 3)

#### 3.1 Enhanced Copy-Paste
```rust
pub trait CopyPasteHandler {
    fn copy_selection(&self) -> Result<String>;
    fn copy_visible_logs(&self) -> Result<String>;
    fn copy_all_logs(&self) -> Result<String>;
}

impl CopyPasteHandler for LogTableView {
    fn copy_selection(&self) -> Result<String> {
        // Native cursive selection handling
        let selected_rows = self.table.selection();
        // Format and return selected text
    }
}
```

#### 3.2 Performance Optimizations
```rust
pub struct LogViewOptimizer {
    virtual_scrolling: bool,
    lazy_rendering: bool,
    diff_updates: bool,
}

impl LogViewOptimizer {
    pub fn optimize_rendering(&mut self, viewport_size: usize);
    pub fn enable_virtual_scrolling(&mut self, enabled: bool);
    pub fn batch_updates(&mut self, entries: Vec<LogEntry>);
}
```

## Component Interactions

### Data Flow Architecture
```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   Log Stream    │───▶│   EventHandler   │───▶│   LogBuffer     │
│   (Tokio)       │    │                  │    │                 │
└─────────────────┘    └──────────────────┘    └─────────────────┘
                                │                        │
                                ▼                        ▼
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│  FilterPanel    │◄──▶│  CursiveApp      │◄──▶│  LogTableView   │
│                 │    │   (Main UI)      │    │                 │
└─────────────────┘    └──────────────────┘    └─────────────────┘
                                │
                                ▼
                       ┌──────────────────┐
                       │   StatusBar      │
                       │                  │
                       └──────────────────┘
```

### UI Layout Structure
```
┌─────────────────────────────────────────────────────────────┐
│                     Filter Panel                            │
│ ┌─────────────────────┐ ┌─────────────────────────────────┐ │
│ │ Include: [ERROR.*]  │ │ Exclude: [debug|trace]          │ │
│ └─────────────────────┘ └─────────────────────────────────┘ │
├─────────────────────────────────────────────────────────────┤
│                     Log Display Area                        │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ [✓] 2025-06-19 10:30:15 pod-1/container ERROR: Failed  │ │
│ │ [ ] 2025-06-19 10:30:16 pod-2/container INFO: Success  │ │
│ │ [✓] 2025-06-19 10:30:17 pod-1/container ERROR: Retry   │ │
│ │ ... (scrollable with native selection)                 │ │
│ └─────────────────────────────────────────────────────────┘ │
├─────────────────────────────────────────────────────────────┤
│ NORMAL │ FOLLOW │ 15,432 logs │ Scroll: 0/15432 │ Ctrl+C=Copy │
└─────────────────────────────────────────────────────────────┘
```

## Implementation Phases

### Phase 1: Foundation (Days 1-3)
- [x] **Step 1.1**: Update Cargo.toml dependencies
  - [x] Remove ratatui, crossterm, arboard dependencies
  - [x] Add cursive, cursive_table_view, cursive_buffered_backend
  - [x] Test compilation with new dependencies
- [x] **Step 1.2**: Create new UI module structure
  - [x] Create src/ui/cursive/ directory
  - [x] Create mod.rs, app.rs, log_view.rs, filter_panel.rs, status_bar.rs, event_handler.rs, theme.rs
  - [x] Set up basic module exports
- [x] **Step 1.3**: Implement basic Cursive application bootstrap
  - [x] Create WakeApp struct with Cursive instance
  - [x] Implement theme setup and global callbacks
  - [x] Create entry point function run_with_cursive_ui()

### Phase 2: Core Components (Days 4-6)
- [x] **Step 2.1**: Implement LogView with TableView
  - [x] Create LogDisplayEntry struct with formatting methods
  - [x] Implement log buffer management with VecDeque
  - [x] Add native scrolling and selection support with ScrollView + TextView
  - [x] Implement copy-paste functionality framework
  - [x] Add color-coded display and keyboard navigation
- [x] **Step 2.2**: Create FilterPanel with EditView widgets
  - [x] Implement include/exclude filter input fields with real-time regex validation
  - [x] Add visual feedback with colored error messages
  - [x] Set up keyboard shortcuts (i/e to focus filters, Ctrl+U to clear, Esc to return)
  - [x] Integrate validation system and user-friendly tips
- [x] **Step 2.3**: Build StatusBar widget
  - [x] Create dynamic status display with color-coded mode indicators (NORMAL/FOLLOW/PAUSED)
  - [x] Add log count and scroll position display with smart formatting
  - [x] Implement memory usage warnings with popup dialogs
  - [x] Add comprehensive help dialog and keyboard shortcuts display
  - [x] Set up global shortcuts (h:Help, q:Quit, f:Follow, Space:Pause)

### Phase 3: Event System & Integration (Days 7-9)
- [ ] **Step 3.1**: Implement EventHandler system
  - [ ] Create AppEvent enum for all UI events
  - [ ] Set up tokio channels for async communication
  - [ ] Implement event processing and distribution
- [ ] **Step 3.2**: Integrate with existing log streaming
  - [ ] Connect Kubernetes log stream to UI
  - [ ] Implement async log processing
  - [ ] Add filter application to incoming logs
- [ ] **Step 3.3**: Add advanced UI features
  - [ ] Implement help dialog system
  - [ ] Add memory warning popups
  - [ ] Create theme customization support

### Phase 4: Testing & Validation (Days 10-12)
- [ ] **Step 4.1**: Unit testing
  - [ ] Test LogView operations and filtering
  - [ ] Test FilterPanel input validation
  - [ ] Test StatusBar updates and formatting
  - [ ] Test EventHandler message processing
- [ ] **Step 4.2**: Integration testing
  - [ ] Test end-to-end log streaming and display
  - [ ] Test filter application and performance
  - [ ] Test memory management under load
  - [ ] Test copy-paste functionality
- [ ] **Step 4.3**: Performance benchmarking
  - [ ] Compare memory usage vs ratatui implementation
  - [ ] Measure CPU usage during high throughput
  - [ ] Test UI responsiveness under load
  - [ ] Validate startup time improvements

### Phase 5: Migration & Cleanup (Days 13-15)
- [ ] **Step 5.1**: Create compatibility layer
  - [ ] Add CLI flags for UI selection (--use-cursive-ui)
  - [ ] Maintain existing API compatibility
  - [ ] Create migration documentation
- [ ] **Step 5.2**: Dependency optimization
  - [ ] Audit and remove unused dependencies
  - [ ] Optimize feature flags for minimal build size
  - [ ] Clean up redundant code paths
- [ ] **Step 5.3**: Final cleanup and deployment
  - [ ] Remove old ratatui implementation
  - [ ] Update documentation and README
  - [ ] Prepare release notes and migration guide

## 🚀 Performance Targets

| Metric | Current (Ratatui) | Target (Cursive) | Status |
|--------|-------------------|------------------|--------|
| Memory Usage | 18MB | ≤14MB | ⏳ |
| CPU Usage | 13-15% | ≤10% | ⏳ |
| Code Lines | 2,800 lines | ≤1,400 lines | ⏳ |
| Startup Time | 200ms | ≤100ms | ⏳ |
| Copy-Paste | Manual/Complex | Native | ⏳ |

## 📊 Progress Tracking

**Overall Progress**: 12% (3/25 steps completed)

**Phase 1**: ✅ 100% (3/3 steps completed)  
**Phase 2**: 0% (0/3 steps completed)  
**Phase 3**: 0% (0/3 steps completed)  
**Phase 4**: 0% (0/3 steps completed)  
**Phase 5**: 0% (0/3 steps completed)

---