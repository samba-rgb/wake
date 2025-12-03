//! Script Executor UI - TUI for running scripts and displaying output
//! Beautiful UI with progress tracking, live output, and save options

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, poll},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, Paragraph, Wrap},
    Frame, Terminal,
};
use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use std::process::Stdio;
use tokio::process::Command;
use tokio::io::{AsyncBufReadExt, BufReader};
use chrono::Local;

use super::manager::{Script, ScriptArg};
use crate::k8s::pod::PodInfo;

/// Black background style for dark mode
fn dark_style() -> Style {
    Style::default().bg(Color::Black).fg(Color::White)
}

/// Pod execution status with live output
#[derive(Debug, Clone)]
pub enum PodStatus {
    Pending,
    Running { live_output: String },
    Completed { success: bool, output: String },
    Failed { error: String },
}

/// Pod execution state
#[derive(Debug, Clone)]
pub struct PodExecutionState {
    pub pod: PodInfo,
    pub status: PodStatus,
}

/// Script executor state
pub struct ScriptExecutorState {
    pub script: Script,
    pub arguments: HashMap<String, String>,
    pub pods: Vec<PodExecutionState>,
    pub selected_pod_index: usize,
    pub execution_complete: bool,
    pub show_merge_dialog: bool,
    pub merge_choice: bool,
    pub output_dir: PathBuf,
    pub output_scroll: usize,
    pub current_executing: Option<usize>,
    pub saved_message: Option<String>,
}

impl ScriptExecutorState {
    pub fn new(script: Script, pods: Vec<PodInfo>) -> Self {
        let pod_states = pods.into_iter()
            .map(|pod| PodExecutionState {
                pod,
                status: PodStatus::Pending,
            })
            .collect();

        let timestamp = Local::now().format("%Y%m%d_%H%M%S");
        let output_dir = PathBuf::from(format!("wake_script_output_{}", timestamp));

        Self {
            script,
            arguments: HashMap::new(),
            pods: pod_states,
            selected_pod_index: 0,
            execution_complete: false,
            show_merge_dialog: false,
            merge_choice: true,
            output_dir,
            output_scroll: 0,
            current_executing: None,
            saved_message: None,
        }
    }

    fn completed_count(&self) -> usize {
        self.pods.iter().filter(|p| matches!(p.status, PodStatus::Completed { .. } | PodStatus::Failed { .. })).count()
    }

    fn success_count(&self) -> usize {
        self.pods.iter().filter(|p| matches!(p.status, PodStatus::Completed { success: true, .. })).count()
    }

    fn failed_count(&self) -> usize {
        self.pods.iter().filter(|p| matches!(p.status, PodStatus::Completed { success: false, .. } | PodStatus::Failed { .. })).count()
    }
}

/// Argument input state
struct ArgInputState {
    args: Vec<ScriptArg>,
    values: HashMap<String, String>,
    current_index: usize,
    current_input: String,
    cursor_pos: usize,
    done: bool,
}

impl ArgInputState {
    fn new(args: Vec<ScriptArg>) -> Self {
        Self {
            args,
            values: HashMap::new(),
            current_index: 0,
            current_input: String::new(),
            cursor_pos: 0,
            done: false,
        }
    }

    fn current_arg(&self) -> Option<&ScriptArg> {
        self.args.get(self.current_index)
    }

    fn submit_current(&mut self) {
        if let Some(arg) = self.current_arg() {
            let value = if self.current_input.is_empty() {
                arg.default_value.clone().unwrap_or_default()
            } else {
                self.current_input.clone()
            };
            self.values.insert(arg.name.clone(), value);
            self.current_input.clear();
            self.cursor_pos = 0;
            self.current_index += 1;

            if self.current_index >= self.args.len() {
                self.done = true;
            }
        }
    }
}

/// Run the script executor TUI
pub async fn run_script_executor(script: Script, pods: Vec<PodInfo>) -> Result<()> {
    let arguments = if !script.arguments.is_empty() {
        collect_arguments(&script.arguments).await?
    } else {
        HashMap::new()
    };

    let mut state = ScriptExecutorState::new(script, pods);
    state.arguments = arguments;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_executor_loop(&mut terminal, &mut state).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    if let Some(msg) = &state.saved_message {
        println!("{}", msg);
    }

    result
}

async fn collect_arguments(args: &[ScriptArg]) -> Result<HashMap<String, String>> {
    let mut state = ArgInputState::new(args.to_vec());

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    loop {
        terminal.draw(|f| draw_arg_input(f, &state))?;

        if state.done {
            break;
        }

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Enter => state.submit_current(),
                KeyCode::Char(c) => {
                    state.current_input.insert(state.cursor_pos, c);
                    state.cursor_pos += 1;
                }
                KeyCode::Backspace => {
                    if state.cursor_pos > 0 {
                        state.cursor_pos -= 1;
                        state.current_input.remove(state.cursor_pos);
                    }
                }
                KeyCode::Left if state.cursor_pos > 0 => state.cursor_pos -= 1,
                KeyCode::Right if state.cursor_pos < state.current_input.len() => state.cursor_pos += 1,
                KeyCode::Esc => {
                    disable_raw_mode()?;
                    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                    return Err(anyhow::anyhow!("Cancelled by user"));
                }
                _ => {}
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(state.values)
}

fn draw_arg_input(f: &mut Frame, state: &ArgInputState) {
    let area = f.size();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Length(8),
            Constraint::Length(5),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .margin(2)
        .split(area);

    let header_text = vec![
        Line::from(Span::styled("╔═══════════════════════════════════════╗", Style::default().fg(Color::Cyan))),
        Line::from(vec![
            Span::styled("║  ", Style::default().fg(Color::Cyan)),
            Span::styled("📝 SCRIPT ARGUMENTS", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("              ║", Style::default().fg(Color::Cyan)),
        ]),
        Line::from(Span::styled("╚═══════════════════════════════════════╝", Style::default().fg(Color::Cyan))),
    ];
    f.render_widget(Paragraph::new(header_text).alignment(Alignment::Center), chunks[0]);

    if let Some(arg) = state.current_arg() {
        let req_badge = if arg.required {
            Span::styled(" REQUIRED ", Style::default().bg(Color::Red).fg(Color::White).add_modifier(Modifier::BOLD))
        } else {
            Span::styled(" OPTIONAL ", Style::default().bg(Color::Blue).fg(Color::White))
        };

        let mut info_lines = vec![
            Line::from(vec![
                Span::styled("  📌 ", Style::default().fg(Color::Yellow)),
                Span::styled(&arg.name, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                Span::raw("  "),
                req_badge,
            ]),
            Line::from(""),
        ];

        if let Some(ref desc) = arg.description {
            info_lines.push(Line::from(vec![
                Span::styled("  📋 ", Style::default().fg(Color::Gray)),
                Span::styled(desc, Style::default().fg(Color::Gray)),
            ]));
        }

        if let Some(ref default) = arg.default_value {
            info_lines.push(Line::from(vec![
                Span::styled("  💡 Default: ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("\"{}\"", default), Style::default().fg(Color::Green)),
            ]));
        }

        let info_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(Span::styled(
                format!(" Argument {}/{} ", state.current_index + 1, state.args.len()),
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            ));
        f.render_widget(Paragraph::new(info_lines).block(info_block), chunks[1]);
    }

    let input_with_cursor = {
        let (before, after) = state.current_input.split_at(state.cursor_pos);
        format!("{}▌{}", before, after)
    };
    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(Span::styled(" ✏️  Enter Value ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));
    f.render_widget(Paragraph::new(input_with_cursor).style(Style::default().fg(Color::White)).block(input_block), chunks[2]);

    let progress_dots: String = (0..state.args.len())
        .map(|i| if i < state.current_index { "●" } else if i == state.current_index { "◉" } else { "○" })
        .collect::<Vec<_>>()
        .join(" ");
    let progress = Paragraph::new(Line::from(vec![
        Span::styled("  ", Style::default()),
        Span::styled(progress_dots, Style::default().fg(Color::Cyan)),
        Span::styled("  │  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Enter", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        Span::styled(" Submit  ", Style::default().fg(Color::Gray)),
        Span::styled("Esc", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
        Span::styled(" Cancel", Style::default().fg(Color::Gray)),
    ])).alignment(Alignment::Center);
    f.render_widget(progress, chunks[3]);
}

async fn run_executor_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut ScriptExecutorState,
) -> Result<()> {
    let script_content = substitute_args(&state.script.content, &state.arguments);
    
    // Create shared state for parallel execution with live output
    let pod_statuses: Arc<RwLock<Vec<PodStatus>>> = Arc::new(RwLock::new(
        vec![PodStatus::Running { live_output: String::new() }; state.pods.len()]
    ));
    
    // Mark all pods as running
    for pod_state in &mut state.pods {
        pod_state.status = PodStatus::Running { live_output: String::new() };
    }
    
    terminal.draw(|f| draw_executor(f, state))?;
    
    // Spawn parallel tasks for all pods with live streaming
    let mut handles = Vec::new();
    for (i, pod_state) in state.pods.iter().enumerate() {
        let script = script_content.clone();
        let pod = pod_state.pod.clone();
        let statuses = Arc::clone(&pod_statuses);
        
        let handle = tokio::spawn(async move {
            execute_script_on_pod_streaming(&script, &pod, statuses, i).await
        });
        handles.push(handle);
    }
    
    // Poll for completion while updating UI AND handling keyboard input
    loop {
        // Update state from shared results
        if let Ok(guard) = pod_statuses.read() {
            for (i, status) in guard.iter().enumerate() {
                state.pods[i].status = status.clone();
            }
        }
        
        let all_done = state.pods.iter().all(|p| {
            matches!(p.status, PodStatus::Completed { .. } | PodStatus::Failed { .. })
        });
        
        terminal.draw(|f| draw_executor(f, state))?;
        
        if all_done {
            break;
        }
        
        // Non-blocking keyboard input - allows navigation during execution
        if poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        if state.selected_pod_index > 0 {
                            state.selected_pod_index -= 1;
                            state.output_scroll = 0;
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if state.selected_pod_index < state.pods.len().saturating_sub(1) {
                            state.selected_pod_index += 1;
                            state.output_scroll = 0;
                        }
                    }
                    KeyCode::PageUp => state.output_scroll = state.output_scroll.saturating_sub(10),
                    KeyCode::PageDown => state.output_scroll += 10,
                    KeyCode::Char('q') | KeyCode::Esc => {
                        for handle in &handles {
                            handle.abort();
                        }
                        return Ok(());
                    }
                    _ => {}
                }
            }
        }
        
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    
    for handle in handles {
        let _ = handle.await;
    }

    state.execution_complete = true;
    state.current_executing = None;
    state.show_merge_dialog = true;

    // Post-execution interaction loop
    loop {
        terminal.draw(|f| draw_executor(f, state))?;

        if let Event::Key(key) = event::read()? {
            if state.show_merge_dialog {
                match key.code {
                    KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right | KeyCode::Tab => {
                        state.merge_choice = !state.merge_choice;
                    }
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        state.merge_choice = true;
                        save_outputs(state).await?;
                        state.show_merge_dialog = false;
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') => {
                        state.merge_choice = false;
                        save_outputs(state).await?;
                        state.show_merge_dialog = false;
                    }
                    KeyCode::Enter => {
                        save_outputs(state).await?;
                        state.show_merge_dialog = false;
                    }
                    KeyCode::Esc => state.show_merge_dialog = false,
                    _ => {}
                }
            } else {
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        if state.selected_pod_index > 0 {
                            state.selected_pod_index -= 1;
                            state.output_scroll = 0;
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if state.selected_pod_index < state.pods.len().saturating_sub(1) {
                            state.selected_pod_index += 1;
                            state.output_scroll = 0;
                        }
                    }
                    KeyCode::PageUp => state.output_scroll = state.output_scroll.saturating_sub(10),
                    KeyCode::PageDown => state.output_scroll += 10,
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    _ => {}
                }
            }
        }
    }
}

fn substitute_args(script: &str, args: &HashMap<String, String>) -> String {
    let mut result = script.to_string();
    for (name, value) in args {
        result = result.replace(&format!("${{{}}}", name), value);
        result = result.replace(&format!("${}", name), value);
    }
    result
}

/// Execute script on pod with live streaming output
async fn execute_script_on_pod_streaming(
    script: &str,
    pod: &PodInfo,
    statuses: Arc<RwLock<Vec<PodStatus>>>,
    index: usize,
) {
    let container = pod.containers.first().map(|c| c.as_str()).unwrap_or("default");
    let escaped_script = script.replace("'", "'\''");
    let exec_cmd = format!("echo '{}' | sh", escaped_script);

    let result = Command::new("kubectl")
        .arg("exec")
        .arg("-n")
        .arg(&pod.namespace)
        .arg(&pod.name)
        .arg("-c")
        .arg(container)
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg(&exec_cmd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    let mut child = match result {
        Ok(c) => c,
        Err(e) => {
            if let Ok(mut guard) = statuses.write() {
                guard[index] = PodStatus::Failed { error: format!("Failed to spawn: {}", e) };
            }
            return;
        }
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    
    let mut output = String::new();
    
    // Read stdout with live updates
    if let Some(stdout) = stdout {
        let mut reader = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            output.push_str(&line);
            output.push('\n');
            
            // Update live output
            if let Ok(mut guard) = statuses.write() {
                guard[index] = PodStatus::Running { live_output: output.clone() };
            }
        }
    }
    
    // Read any remaining stderr
    if let Some(stderr) = stderr {
        let mut reader = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            output.push_str(&line);
            output.push('\n');
        }
    }
    
    // Wait for process to complete
    let status = child.wait().await;
    let success = status.map(|s| s.success()).unwrap_or(false);
    
    if let Ok(mut guard) = statuses.write() {
        guard[index] = PodStatus::Completed { success, output };
    }
}

async fn save_outputs(state: &mut ScriptExecutorState) -> Result<()> {
    std::fs::create_dir_all(&state.output_dir)?;

    if state.merge_choice {
        let mut merged = String::new();
        let separator = "═".repeat(58);
        let line_sep = "─".repeat(60);
        
        merged.push_str(&format!("╔{}╗\n", separator));
        merged.push_str(&format!("║  🚀 Wake Script Execution Report{:>24}║\n", ""));
        merged.push_str(&format!("╚{}╝\n\n", separator));
        merged.push_str(&format!("📜 Script: {}\n", state.script.name));
        merged.push_str(&format!("🕐 Executed: {}\n", Local::now().format("%Y-%m-%d %H:%M:%S")));
        merged.push_str(&format!("📊 Total Pods: {}\n", state.pods.len()));
        merged.push_str(&format!("✅ Success: {} | ❌ Failed: {}\n\n", state.success_count(), state.failed_count()));

        for (i, pod_state) in state.pods.iter().enumerate() {
            merged.push_str(&format!("{}\n", line_sep));
            merged.push_str(&format!("📦 Pod {}: {}/{}\n", i + 1, pod_state.pod.namespace, pod_state.pod.name));
            merged.push_str(&format!("{}\n", line_sep));

            match &pod_state.status {
                PodStatus::Completed { success, output } => {
                    merged.push_str(&format!("Status: {}\n\n", if *success { "✅ SUCCESS" } else { "⚠️ COMPLETED WITH ERRORS" }));
                    merged.push_str(output);
                }
                PodStatus::Failed { error } => {
                    merged.push_str(&format!("Status: ❌ FAILED\nError: {}\n", error));
                }
                _ => {
                    merged.push_str("Status: ⏳ Not executed\n");
                }
            }
            merged.push_str("\n\n");
        }

        let merged_path = state.output_dir.join("merged_output.txt");
        std::fs::write(&merged_path, merged)?;
        state.saved_message = Some(format!("✅ Output saved to: {}", merged_path.display()));
    } else {
        for pod_state in &state.pods {
            let filename = format!("{}_{}.txt", pod_state.pod.namespace, pod_state.pod.name);
            let filepath = state.output_dir.join(&filename);

            let content = match &pod_state.status {
                PodStatus::Completed { output, .. } => output.clone(),
                PodStatus::Failed { error } => format!("ERROR: {}", error),
                _ => "(no output)".to_string(),
            };

            std::fs::write(&filepath, content)?;
        }
        state.saved_message = Some(format!("✅ Outputs saved to: {}/", state.output_dir.display()));
    }

    Ok(())
}

fn draw_executor(f: &mut Frame, state: &ScriptExecutorState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Length(4),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(f.size());

    let status_icon = if state.execution_complete {
        if state.failed_count() == 0 { "✅" } else { "⚠️" }
    } else {
        "🔄"
    };
    
    let header_lines = vec![
        Line::from(Span::styled("╔══════════════════════════════════════════════════════════════╗", Style::default().fg(Color::Cyan))),
        Line::from(vec![
            Span::styled("║  ", Style::default().fg(Color::Cyan)),
            Span::styled(format!("{} EXECUTING: ", status_icon), Style::default().fg(Color::White)),
            Span::styled(&state.script.name, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:>width$}║", "", width = 45usize.saturating_sub(state.script.name.len())), Style::default().fg(Color::Cyan)),
        ]),
        Line::from(Span::styled("╚══════════════════════════════════════════════════════════════╝", Style::default().fg(Color::Cyan))),
    ];
    f.render_widget(Paragraph::new(header_lines).alignment(Alignment::Center), chunks[0]);

    let progress = state.completed_count() as f64 / state.pods.len().max(1) as f64;
    let progress_color = if state.execution_complete {
        if state.failed_count() == 0 { Color::Green } else { Color::Yellow }
    } else {
        Color::Cyan
    };
    
    let progress_label = format!(
        " {} / {} pods  │  ✅ {}  ❌ {} ",
        state.completed_count(), state.pods.len(), state.success_count(), state.failed_count()
    );
    
    let gauge = Gauge::default()
        .block(Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(progress_color))
            .title(Span::styled(" Progress ", Style::default().fg(progress_color).add_modifier(Modifier::BOLD))))
        .gauge_style(Style::default().fg(progress_color).bg(Color::DarkGray))
        .ratio(progress)
        .label(Span::styled(progress_label, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)));
    f.render_widget(gauge, chunks[1]);

    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(chunks[2]);

    let pod_items: Vec<ListItem> = state.pods.iter().enumerate().map(|(i, pod_state)| {
        let (icon, status_color) = match &pod_state.status {
            PodStatus::Pending => ("⏳", Color::Gray),
            PodStatus::Running { .. } => ("🔄", Color::Yellow),
            PodStatus::Completed { success: true, .. } => ("✅", Color::Green),
            PodStatus::Completed { success: false, .. } => ("⚠️", Color::Yellow),
            PodStatus::Failed { .. } => ("❌", Color::Red),
        };

        let is_selected = i == state.selected_pod_index;
        let line_style = if is_selected {
            Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let pod_name = if pod_state.pod.name.len() > 25 {
            format!("{}...", &pod_state.pod.name[..22])
        } else {
            pod_state.pod.name.clone()
        };

        ListItem::new(Line::from(vec![
            Span::styled(if is_selected { "▶ " } else { "  " }, Style::default().fg(Color::Cyan)),
            Span::styled(format!("{} ", icon), Style::default().fg(status_color)),
            Span::styled(pod_name, line_style.fg(Color::White)),
        ])).style(line_style)
    }).collect();

    let pod_list = List::new(pod_items)
        .block(Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Blue))
            .title(Span::styled(format!(" 📦 Pods ({}) ", state.pods.len()), Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD))));
    f.render_widget(pod_list, content_chunks[0]);

    let (output_title, output_content, output_color) = if let Some(pod_state) = state.pods.get(state.selected_pod_index) {
        match &pod_state.status {
            PodStatus::Pending => (" ⏳ Waiting... ".to_string(), "Script execution pending...".to_string(), Color::Gray),
            PodStatus::Running { live_output } => {
                let display = if live_output.is_empty() {
                    "Running script on pod...\n\n⏳ Please wait...".to_string()
                } else {
                    live_output.clone()
                };
                (" 🔄 Live Output... ".to_string(), display, Color::Yellow)
            }
            PodStatus::Completed { success, output } => (
                if *success { " ✅ Output ".to_string() } else { " ⚠️ Output (with errors) ".to_string() },
                output.clone(),
                if *success { Color::Green } else { Color::Yellow }
            ),
            PodStatus::Failed { error } => (" ❌ Error ".to_string(), format!("Execution failed!\n\n{}", error), Color::Red),
        }
    } else {
        (" Output ".to_string(), "No pod selected".to_string(), Color::Gray)
    };

    let output_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(output_color))
        .title(Span::styled(output_title, Style::default().fg(output_color).add_modifier(Modifier::BOLD)));
    
    f.render_widget(
        Paragraph::new(output_content)
            .block(output_block)
            .wrap(Wrap { trim: false })
            .scroll((state.output_scroll as u16, 0)),
        content_chunks[1]
    );

    let help_text = if state.execution_complete && !state.show_merge_dialog {
        vec![
            Span::styled(" ↑↓ ", Style::default().fg(Color::Black).bg(Color::Cyan)),
            Span::styled(" Select Pod ", Style::default().fg(Color::Gray)),
            Span::styled(" PgUp/PgDn ", Style::default().fg(Color::Black).bg(Color::Cyan)),
            Span::styled(" Scroll ", Style::default().fg(Color::Gray)),
            Span::styled(" q ", Style::default().fg(Color::Black).bg(Color::Red)),
            Span::styled(" Quit ", Style::default().fg(Color::Gray)),
        ]
    } else {
        vec![
            Span::styled(" ↑↓ ", Style::default().fg(Color::Black).bg(Color::Cyan)),
            Span::styled(" Navigate ", Style::default().fg(Color::Gray)),
            Span::styled(" 🔄 ", Style::default().fg(Color::Yellow)),
            Span::styled(" Executing... ", Style::default().fg(Color::Gray)),
            Span::styled(" q ", Style::default().fg(Color::Black).bg(Color::Red)),
            Span::styled(" Cancel ", Style::default().fg(Color::Gray)),
        ]
    };
    
    f.render_widget(
        Paragraph::new(Line::from(help_text))
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray))),
        chunks[3]
    );

    if state.show_merge_dialog {
        draw_merge_dialog(f, state);
    }
}

fn draw_merge_dialog(f: &mut Frame, state: &ScriptExecutorState) {
    let area = centered_rect(55, 45, f.size());
    f.render_widget(Clear, area);
    
    let block = Block::default()
        .title(Span::styled(" 💾 Save Execution Results ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));
    f.render_widget(block, area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(4),
            Constraint::Length(4),
            Constraint::Length(2),
        ])
        .split(area);

    let summary = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("📊 Execution Complete: ", Style::default().fg(Color::White)),
            Span::styled(format!("{} pods", state.pods.len()), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("✅ {}", state.success_count()), Style::default().fg(Color::Green)),
            Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("❌ {}", state.failed_count()), Style::default().fg(Color::Red)),
        ]),
    ]).alignment(Alignment::Center);
    f.render_widget(summary, inner[0]);

    let opt1_style = if state.merge_choice { Style::default().fg(Color::Green).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::Gray) };
    let opt1_border = if state.merge_choice { Color::Green } else { Color::DarkGray };
    let opt1_icon = if state.merge_choice { "◉" } else { "○" };
    
    f.render_widget(
        Paragraph::new(vec![
            Line::from(vec![Span::styled(format!(" {} ", opt1_icon), opt1_style), Span::styled("Merge into single file", opt1_style)]),
            Line::from(Span::styled("   → merged_output.txt", Style::default().fg(Color::DarkGray))),
        ]).block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(opt1_border))),
        inner[2]
    );

    let opt2_style = if !state.merge_choice { Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::Gray) };
    let opt2_border = if !state.merge_choice { Color::Cyan } else { Color::DarkGray };
    let opt2_icon = if !state.merge_choice { "◉" } else { "○" };
    
    f.render_widget(
        Paragraph::new(vec![
            Line::from(vec![Span::styled(format!(" {} ", opt2_icon), opt2_style), Span::styled("Save separate files", opt2_style)]),
            Line::from(Span::styled(format!("   → {}/", state.output_dir.display()), Style::default().fg(Color::DarkGray))),
        ]).block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(opt2_border))),
        inner[3]
    );

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("↑↓", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled(" Switch  ", Style::default().fg(Color::Gray)),
            Span::styled("Enter/Y/N", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled(" Confirm  ", Style::default().fg(Color::Gray)),
            Span::styled("Esc", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::styled(" Skip", Style::default().fg(Color::Gray)),
        ])).alignment(Alignment::Center),
        inner[4]
    );
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
