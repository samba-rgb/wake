use crate::templates::*;
use crate::templates::registry::TemplateRegistry;
use crate::k8s::pod::PodInfo;
use crate::cli::Args;
use anyhow::{anyhow, Result};
use chrono::{DateTime, Local, Utc};
use futures::future::try_join_all;
use glob::glob;
use kube::Client;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::process::Command as AsyncCommand;
use tokio::sync::{mpsc, Mutex};
use tokio::time::sleep;
use uuid::Uuid;

/// Template executor with interactive UI support
pub struct TemplateExecutor {
    registry: TemplateRegistry,
    ui_enabled: bool,
}

/// UI update messages for real-time progress
#[derive(Debug, Clone)]
pub enum UIUpdate {
    PodStatusChanged {
        pod_index: usize,
        status: PodStatus,
    },
    CommandStarted {
        pod_index: usize,
        command_index: usize,
        description: String,
    },
    CommandOutput {
        pod_index: usize,
        command_index: usize,
        output: String,
    },
    CommandCompleted {
        pod_index: usize,
        command_index: usize,
        success: bool,
    },
    FileDownloaded {
        pod_index: usize,
        file: DownloadedFile,
    },
    ExecutionCompleted,
}

/// Pod execution status for UI
#[derive(Debug, Clone)]
pub enum PodStatus {
    Starting,
    Running { command_index: usize },
    WaitingLocal { duration: String, progress: f64 },
    DownloadingFiles { current: usize, total: usize },
    Completed,
    Failed { error: String },
}

/// Command execution status
#[derive(Debug, Clone)]
pub enum CommandStatus {
    Running,
    Completed,
    Failed,
    Waiting,
}

/// Command log entry for UI
#[derive(Debug, Clone)]
pub struct CommandLog {
    pub timestamp: DateTime<Local>,
    pub command_index: usize,
    pub description: String,
    pub output: Option<String>,
    pub status: CommandStatus,
}

/// Pod execution state for UI
#[derive(Debug, Clone)]
pub struct PodExecutionState {
    pub pod_info: PodInfo,
    pub status: PodStatus,
    pub current_command_index: usize,
    pub total_commands: usize,
    pub command_logs: Vec<CommandLog>,
    pub downloaded_files: Vec<DownloadedFile>,
    pub error_message: Option<String>,
}

impl TemplateExecutor {
    pub fn new(registry: TemplateRegistry) -> Self {
        Self {
            registry,
            ui_enabled: true, // Default to UI enabled
        }
    }

    /// Create with specific UI settings
    pub fn new_with_ui_enabled(registry: TemplateRegistry, ui_enabled: bool) -> Self {
        Self {
            registry,
            ui_enabled,
        }
    }

    /// Execute a template with the given arguments using existing k8s client
    pub async fn execute_template(
        &self,
        template_name: &str,
        arguments: Vec<String>,
        pods: &[PodInfo],
        output_dir: Option<PathBuf>,
        args: &Args,
    ) -> Result<TemplateExecutionResult> {
        // Use the existing k8s client creation logic
        let client = crate::k8s::client::create_client(args).await?;
        
        // Get template
        let template = self
            .registry
            .get_template(template_name)
            .ok_or_else(|| anyhow!("Template not found: {}", template_name))?;

        // Parse and validate arguments
        let parsed_args = self.parse_template_arguments(template, &arguments)?;

        // Create execution context
        let execution = TemplateExecution {
            template_name: template_name.to_string(),
            arguments: parsed_args,
            execution_id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            output_dir: output_dir.unwrap_or_else(|| {
                PathBuf::from(format!(
                    "./wake-templates-{}",
                    Local::now().format("%Y%m%d_%H%M%S")
                ))
            }),
        };

        if self.ui_enabled {
            self.execute_template_with_ui(&execution, pods, template)
                .await
        } else {
            self.execute_template_console(&execution, pods, template)
                .await
        }
    }

    /// Execute template with interactive UI
    async fn execute_template_with_ui(
        &self,
        execution: &TemplateExecution,
        pods: &[PodInfo],
        template: &Template,
    ) -> Result<TemplateExecutionResult> {
        // Create UI state
        let ui_state = Arc::new(Mutex::new(crate::ui::template_ui::TemplateUIState::new(
            execution.clone(),
            pods.to_vec(),
            template.clone(),
        )));

        // Create channels for UI updates
        let (ui_tx, ui_rx) = mpsc::channel::<UIUpdate>(1000);

        // Start UI in separate task
        let ui_state_clone = ui_state.clone();
        let ui_task = tokio::spawn(async move {
            if let Err(e) = crate::ui::template_ui::run_template_ui(ui_state_clone, ui_rx).await {
                eprintln!("UI error: {}", e);
            }
        });

        // Execute template with UI updates
        let result = self
            .execute_template_parallel_with_ui(execution, pods, template, ui_tx)
            .await;

        // Wait for UI to finish
        let _ = ui_task.await;

        result
    }

    /// Execute template with simple console output
    async fn execute_template_console(
        &self,
        execution: &TemplateExecution,
        pods: &[PodInfo],
        template: &Template,
    ) -> Result<TemplateExecutionResult> {
        println!("🚀 Executing template '{}' on {} pods", execution.template_name, pods.len());
        println!("📁 Output directory: {}", execution.output_dir.display());
        
        // Create output directory
        std::fs::create_dir_all(&execution.output_dir)?;

        // Execute on all pods in parallel
        let pod_futures: Vec<_> = pods
            .iter()
            .enumerate()
            .map(|(index, pod)| {
                let template = template.clone();
                let execution = execution.clone();
                let pod = pod.clone();

                async move {
                    println!("⚡ Starting execution on pod {}/{}", pod.namespace, pod.name);
                    let result = self
                        .execute_template_on_pod(&template, &execution, &pod)
                        .await;

                    match &result {
                        Ok(_) => println!("✅ Completed pod {}/{}", pod.namespace, pod.name),
                        Err(e) => println!("❌ Failed pod {}/{}: {}", pod.namespace, pod.name, e),
                    }

                    result
                }
            })
            .collect();

        let pod_results = try_join_all(pod_futures).await?;

        // Print summary
        let successful = pod_results.iter().filter(|r| r.success).count();
        let failed = pod_results.len() - successful;
        
        println!("\n📊 Execution Summary:");
        println!("  ✅ Successful: {}", successful);
        println!("  ❌ Failed: {}", failed);
        println!("  📁 Output: {}", execution.output_dir.display());

        Ok(TemplateExecutionResult {
            execution_id: execution.execution_id.clone(),
            template_name: execution.template_name.clone(),
            pod_results,
            output_dir: execution.output_dir.clone(),
        })
    }

    /// Execute template on all pods with UI updates
    async fn execute_template_parallel_with_ui(
        &self,
        execution: &TemplateExecution,
        pods: &[PodInfo],
        template: &Template,
        ui_tx: mpsc::Sender<UIUpdate>,
    ) -> Result<TemplateExecutionResult> {
        // Create output directory
        std::fs::create_dir_all(&execution.output_dir)?;

        // Execute all pods concurrently with UI updates
        let pod_futures: Vec<_> = pods
            .iter()
            .enumerate()
            .map(|(index, pod)| {
                let template = template.clone();
                let execution = execution.clone();
                let pod = pod.clone();
                let ui_tx = ui_tx.clone();

                async move {
                    // Send starting status
                    let _ = ui_tx
                        .send(UIUpdate::PodStatusChanged {
                            pod_index: index,
                            status: PodStatus::Starting,
                        })
                        .await;

                    let result = self
                        .execute_template_on_pod_with_ui(&template, &execution, &pod, index, ui_tx.clone())
                        .await;

                    // Send completion status
                    let status = match &result {
                        Ok(_) => PodStatus::Completed,
                        Err(e) => PodStatus::Failed {
                            error: e.to_string(),
                        },
                    };

                    let _ = ui_tx
                        .send(UIUpdate::PodStatusChanged {
                            pod_index: index,
                            status,
                        })
                        .await;

                    result
                }
            })
            .collect();

        let pod_results = try_join_all(pod_futures).await?;

        // Send completion signal to UI
        let _ = ui_tx.send(UIUpdate::ExecutionCompleted).await;

        Ok(TemplateExecutionResult {
            execution_id: execution.execution_id.clone(),
            template_name: execution.template_name.clone(),
            pod_results,
            output_dir: execution.output_dir.clone(),
        })
    }

    /// Execute template on a single pod with UI updates
    async fn execute_template_on_pod_with_ui(
        &self,
        template: &Template,
        execution: &TemplateExecution,
        pod: &PodInfo,
        pod_index: usize,
        ui_tx: mpsc::Sender<UIUpdate>,
    ) -> Result<PodExecutionResult> {
        let mut command_results = Vec::new();
        let mut downloaded_files = Vec::new();

        // Execute each command in sequence with UI updates
        for (cmd_index, template_cmd) in template.commands.iter().enumerate() {
            // Update UI with current command
            let _ = ui_tx
                .send(UIUpdate::PodStatusChanged {
                    pod_index,
                    status: PodStatus::Running {
                        command_index: cmd_index,
                    },
                })
                .await;

            let _ = ui_tx
                .send(UIUpdate::CommandStarted {
                    pod_index,
                    command_index: cmd_index,
                    description: template_cmd.description.clone(),
                })
                .await;

            let result = self
                .execute_command_on_pod_with_ui(
                    template_cmd,
                    execution,
                    pod,
                    pod_index,
                    cmd_index,
                    ui_tx.clone(),
                )
                .await;

            match result {
                Ok(cmd_result) => {
                    // Send command output to UI
                    if template_cmd.capture_output && !cmd_result.stdout.is_empty() {
                        let _ = ui_tx
                            .send(UIUpdate::CommandOutput {
                                pod_index,
                                command_index: cmd_index,
                                output: cmd_result.stdout.clone(),
                            })
                            .await;
                    }

                    let _ = ui_tx
                        .send(UIUpdate::CommandCompleted {
                            pod_index,
                            command_index: cmd_index,
                            success: true,
                        })
                        .await;

                    command_results.push(cmd_result);
                }
                Err(e) => {
                    let _ = ui_tx
                        .send(UIUpdate::CommandCompleted {
                            pod_index,
                            command_index: cmd_index,
                            success: false,
                        })
                        .await;

                    if template_cmd.ignore_failure {
                        let _ = ui_tx
                            .send(UIUpdate::CommandOutput {
                                pod_index,
                                command_index: cmd_index,
                                output: format!("⚠️ Command failed (ignored): {}", e),
                            })
                            .await;

                        command_results.push(CommandResult {
                            success: false,
                            stdout: String::new(),
                            stderr: e.to_string(),
                            exit_code: -1,
                        });
                    } else {
                        return Err(anyhow!("Command failed on pod {}: {}", pod.name, e));
                    }
                }
            }
        }

        // Download output files with UI updates
        let _ = ui_tx
            .send(UIUpdate::PodStatusChanged {
                pod_index,
                status: PodStatus::DownloadingFiles {
                    current: 0,
                    total: template.output_files.len(),
                },
            })
            .await;

        for (file_index, output_pattern) in template.output_files.iter().enumerate() {
            let _ = ui_tx
                .send(UIUpdate::PodStatusChanged {
                    pod_index,
                    status: PodStatus::DownloadingFiles {
                        current: file_index + 1,
                        total: template.output_files.len(),
                    },
                })
                .await;

            let files = self
                .download_files_from_pod(output_pattern, execution, pod)
                .await?;

            // Update UI with downloaded files
            for file in &files {
                let _ = ui_tx
                    .send(UIUpdate::FileDownloaded {
                        pod_index,
                        file: file.clone(),
                    })
                    .await;
            }

            downloaded_files.extend(files);
        }

        Ok(PodExecutionResult {
            pod_name: pod.name.clone(),
            pod_namespace: pod.namespace.clone(),
            command_results,
            downloaded_files,
            success: true,
        })
    }

    /// Execute template on a single pod (console version)
    async fn execute_template_on_pod(
        &self,
        template: &Template,
        execution: &TemplateExecution,
        pod: &PodInfo,
    ) -> Result<PodExecutionResult> {
        let mut command_results = Vec::new();
        let mut downloaded_files = Vec::new();

        // Execute each command in sequence
        for template_cmd in &template.commands {
            let result = self
                .execute_command_on_pod(template_cmd, execution, pod)
                .await;

            match result {
                Ok(cmd_result) => {
                    command_results.push(cmd_result);
                }
                Err(e) => {
                    if template_cmd.ignore_failure {
                        command_results.push(CommandResult {
                            success: false,
                            stdout: String::new(),
                            stderr: e.to_string(),
                            exit_code: -1,
                        });
                    } else {
                        return Err(e);
                    }
                }
            }
        }

        // Download output files
        for output_pattern in &template.output_files {
            let files = self
                .download_files_from_pod(output_pattern, execution, pod)
                .await?;
            downloaded_files.extend(files);
        }

        Ok(PodExecutionResult {
            pod_name: pod.name.clone(),
            pod_namespace: pod.namespace.clone(),
            command_results,
            downloaded_files,
            success: true,
        })
    }

    /// Execute a single command on a pod with UI updates
    async fn execute_command_on_pod_with_ui(
        &self,
        template_cmd: &TemplateCommand,
        execution: &TemplateExecution,
        pod: &PodInfo,
        pod_index: usize,
        command_index: usize,
        ui_tx: mpsc::Sender<UIUpdate>,
    ) -> Result<CommandResult> {
        // Substitute template variables in command
        let resolved_command = self.resolve_command_variables(template_cmd, execution)?;

        // Handle built-in wait command with UI progress updates
        if resolved_command[0] == "wait" && resolved_command.len() == 2 {
            return self
                .handle_wait_command_with_ui(&resolved_command[1], pod_index, command_index, ui_tx)
                .await;
        }

        // Execute command using kubectl exec
        let mut kubectl_cmd = AsyncCommand::new("kubectl");
        kubectl_cmd
            .arg("exec")
            .arg("-n")
            .arg(&pod.namespace)
            .arg(&pod.name)
            .arg("--");

        kubectl_cmd.args(&resolved_command);

        let output = kubectl_cmd.output().await?;

        Ok(CommandResult {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    /// Execute a single command on a pod (console version)
    async fn execute_command_on_pod(
        &self,
        template_cmd: &TemplateCommand,
        execution: &TemplateExecution,
        pod: &PodInfo,
    ) -> Result<CommandResult> {
        // Substitute template variables in command
        let resolved_command = self.resolve_command_variables(template_cmd, execution)?;

        // Handle built-in wait command
        if resolved_command[0] == "wait" && resolved_command.len() == 2 {
            return self.handle_wait_command(&resolved_command[1]).await;
        }

        // Execute command using kubectl exec
        let mut kubectl_cmd = AsyncCommand::new("kubectl");
        kubectl_cmd
            .arg("exec")
            .arg("-n")
            .arg(&pod.namespace)
            .arg(&pod.name)
            .arg("--");

        kubectl_cmd.args(&resolved_command);

        let output = kubectl_cmd.output().await?;

        Ok(CommandResult {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    /// Handle wait command with UI progress updates
    async fn handle_wait_command_with_ui(
        &self,
        duration_str: &str,
        pod_index: usize,
        command_index: usize,
        ui_tx: mpsc::Sender<UIUpdate>,
    ) -> Result<CommandResult> {
        let seconds = self.parse_duration(duration_str)?;

        // Update UI to show waiting status
        let _ = ui_tx
            .send(UIUpdate::PodStatusChanged {
                pod_index,
                status: PodStatus::WaitingLocal {
                    duration: duration_str.to_string(),
                    progress: 0.0,
                },
            })
            .await;

        let _ = ui_tx
            .send(UIUpdate::CommandOutput {
                pod_index,
                command_index,
                output: format!("⏳ Waiting {} ({} seconds) locally...", duration_str, seconds),
            })
            .await;

        // Progress updates during wait
        for i in 0..seconds {
            sleep(Duration::from_secs(1)).await;

            let progress = (i + 1) as f64 / seconds as f64;

            // Update progress every second for UI
            let _ = ui_tx
                .send(UIUpdate::PodStatusChanged {
                    pod_index,
                    status: PodStatus::WaitingLocal {
                        duration: duration_str.to_string(),
                        progress,
                    },
                })
                .await;

            // Update progress bar in command output every 10 seconds
            if i % 10 == 0 || i == seconds - 1 {
                let progress_percent = (progress * 100.0) as u32;
                let elapsed_min = (i + 1) / 60;
                let elapsed_sec = (i + 1) % 60;

                let bar_width = 40;
                let filled = (progress * bar_width as f64) as usize;
                let empty = bar_width - filled;
                let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));

                let _ = ui_tx
                    .send(UIUpdate::CommandOutput {
                        pod_index,
                        command_index,
                        output: format!(
                            "⏳ [{}] {}% ({:02}:{:02} elapsed)",
                            bar, progress_percent, elapsed_min, elapsed_sec
                        ),
                    })
                    .await;
            }
        }

        Ok(CommandResult {
            success: true,
            stdout: format!("Waited {} seconds locally", seconds),
            stderr: String::new(),
            exit_code: 0,
        })
    }

    /// Handle wait command (console version)
    async fn handle_wait_command(&self, duration_str: &str) -> Result<CommandResult> {
        let seconds = self.parse_duration(duration_str)?;
        println!("⏳ Waiting {} seconds locally...", seconds);
        sleep(Duration::from_secs(seconds)).await;

        Ok(CommandResult {
            success: true,
            stdout: format!("Waited {} seconds locally", seconds),
            stderr: String::new(),
            exit_code: 0,
        })
    }

    /// Download files matching a pattern from a pod
    async fn download_files_from_pod(
        &self,
        pattern: &OutputFilePattern,
        execution: &TemplateExecution,
        pod: &PodInfo,
    ) -> Result<Vec<DownloadedFile>> {
        // Extract directory and filename pattern from the full pattern
        let pattern_path = Path::new(&pattern.pattern);
        let (search_dir, filename_pattern) = if let Some(parent) = pattern_path.parent() {
            // If pattern has a directory part (e.g., "/tmp/thread_dump_*.txt")
            let parent_str = parent.to_string_lossy();
            let filename = pattern_path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("*");
            (parent_str.to_string(), filename.to_string())
        } else {
            // If pattern is just a filename (e.g., "thread_dump_*.txt")
            ("/".to_string(), pattern.pattern.clone())
        };

        // Use the improved find command: cd to directory then find with -name
        let mut kubectl_cmd = AsyncCommand::new("kubectl");
        kubectl_cmd
            .arg("exec")
            .arg("-n")
            .arg(&pod.namespace)
            .arg(&pod.name)
            .arg("--")
            .arg("sh")
            .arg("-c")
            .arg(format!(
                "cd '{}' 2>/dev/null && find . -name '{}' -type f 2>/dev/null | head -100",
                search_dir, filename_pattern
            ));

        let output = kubectl_cmd.output().await?;
        
        if !output.status.success() {
            if pattern.required {
                return Err(anyhow!("Failed to find files matching pattern: {} in directory: {}", filename_pattern, search_dir));
            }
            return Ok(vec![]);
        }

        let files_output = String::from_utf8_lossy(&output.stdout);
        let relative_files: Vec<&str> = files_output.lines()
            .filter(|line| !line.is_empty() && line.starts_with("./"))
            .collect();

        if relative_files.is_empty() {
            if pattern.required {
                return Err(anyhow!("No files found matching pattern: {} in directory: {}", filename_pattern, search_dir));
            }
            return Ok(vec![]);
        }

        let mut downloaded_files = Vec::new();

        // Create pod-specific output directory
        let pod_output_dir = execution
            .output_dir
            .join(&pod.namespace)
            .join(&pod.name);
        std::fs::create_dir_all(&pod_output_dir)?;

        // Download each file
        for relative_file in relative_files {
            // Convert relative path to absolute path
            let absolute_path = if search_dir == "/" {
                format!("/{}", &relative_file[2..]) // Remove "./" and add root "/"
            } else {
                format!("{}/{}", search_dir, &relative_file[2..]) // Remove "./" and prepend search_dir
            };

            let file_name = Path::new(&absolute_path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("unknown_file");

            let local_path = pod_output_dir.join(file_name);

            // Use kubectl cp to download the file
            let mut cp_cmd = AsyncCommand::new("kubectl");
            cp_cmd
                .arg("cp")
                .arg("-n")
                .arg(&pod.namespace)
                .arg(format!("{}:{}", pod.name, absolute_path))
                .arg(&local_path);

            let cp_output = cp_cmd.output().await?;

            if cp_output.status.success() {
                let file_size = local_path.metadata()?.len();
                downloaded_files.push(DownloadedFile {
                    remote_path: absolute_path,
                    local_path,
                    file_type: pattern.file_type.clone(),
                    size_bytes: file_size,
                });
            }
        }

        Ok(downloaded_files)
    }

    /// Resolve template variables in command
    fn resolve_command_variables(
        &self,
        template_cmd: &TemplateCommand,
        execution: &TemplateExecution,
    ) -> Result<Vec<String>> {
        let mut resolved_command = Vec::new();
        
        for arg in &template_cmd.command {
            let mut resolved_arg = arg.clone();

            // Replace template variables
            for (param_name, value) in &execution.arguments {
                let placeholder = format!("{{{{{}}}}}", param_name);
                resolved_arg = resolved_arg.replace(&placeholder, value);
            }

            resolved_command.push(resolved_arg);
        }

        Ok(resolved_command)
    }

    /// Parse template arguments
    fn parse_template_arguments(
        &self,
        template: &Template,
        arguments: &[String],
    ) -> Result<HashMap<String, String>> {
        let mut parsed_args = HashMap::new();
        
        // Check if we have the right number of arguments
        if arguments.len() != template.parameters.len() {
            return Err(anyhow!(
                "Template '{}' expects {} arguments, got {}",
                template.name,
                template.parameters.len(),
                arguments.len()
            ));
        }

        // Parse and validate each argument
        for (i, param) in template.parameters.iter().enumerate() {
            let arg_value = &arguments[i];
            
            // Validate argument based on parameter type
            match param.param_type {
                ParameterType::Integer => {
                    arg_value.parse::<i64>().map_err(|_| {
                        anyhow!("Argument '{}' must be an integer", param.name)
                    })?;
                }
                ParameterType::Duration => {
                    self.parse_duration(arg_value)?;
                }
                _ => {} // Other types don't need validation here
            }

            // Validate against regex if provided
            if let Some(ref regex) = param.validation_regex {
                let re = regex::Regex::new(regex)?;
                if !re.is_match(arg_value) {
                    return Err(anyhow!(
                        "Argument '{}' doesn't match required format",
                        param.name
                    ));
                }
            }

            parsed_args.insert(param.name.clone(), arg_value.clone());
        }

        Ok(parsed_args)
    }

    /// Parse duration string to seconds
    fn parse_duration(&self, duration_str: &str) -> Result<u64> {
        let duration_str = duration_str.trim();
        
        if duration_str.is_empty() {
            return Err(anyhow!("Duration cannot be empty"));
        }

        let (number_part, unit_part) = if duration_str.ends_with('s') {
            (&duration_str[..duration_str.len() - 1], "s")
        } else if duration_str.ends_with('m') {
            (&duration_str[..duration_str.len() - 1], "m")
        } else if duration_str.ends_with('h') {
            (&duration_str[..duration_str.len() - 1], "h")
        } else {
            return Err(anyhow!("Duration must end with 's', 'm', or 'h'"));
        };

        let number: u64 = number_part.parse()
            .map_err(|_| anyhow!("Invalid duration format"))?;

        let seconds = match unit_part {
            "s" => number,
            "m" => number * 60,
            "h" => number * 3600,
            _ => return Err(anyhow!("Invalid duration unit")),
        };

        Ok(seconds)
    }

    /// List available templates
    pub fn list_templates(&self) -> Vec<&str> {
        self.registry.list_templates()
    }

    /// Get template information
    pub fn get_template(&self, name: &str) -> Option<&Template> {
        self.registry.get_template(name)
    }
}