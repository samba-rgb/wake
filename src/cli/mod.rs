pub mod args;

pub use args::{Args, parse_args};
use anyhow::{Result, Context};
use tracing::info;
use std::fs;
use regex::Regex;
use std::io::Write;
use std::fs::File;
use std::collections::HashMap;
use std::path::Path;
use crate::k8s::pod::select_pods;
use kube::Api;
use k8s_openapi::api::core::v1::Pod;
use chrono::Local;
use comfy_table::{Table as HelpTable, presets::UTF8_FULL as HELP_UTF8_FULL, ContentArrangement as HelpContentArrangement, Cell as HelpCell};
use comfy_table::Table;
use colored::Colorize;

/// Prints WAKE in big text with dots
fn print_wake_big_text() {
    println!("................................................................
                                              
█|          █|    █|█|    █|    █|  █|█|█|█|  
█|          █|  █|    █|  █|  █|    █|        
█|    █|    █|  █|█|█|█|  █|█|      █|█|█|    
  █|  █|  █|    █|    █|  █|  █|    █|        
    █|  █|      █|    █|  █|    █|  █|█|█|█| 
                                              
................................................................");
}

/// Checks if wake is being run with default options
fn is_default_run(args: &Args) -> bool {
    // Check if using all default options (effectively no filtering)
    args.pod_selector == ".*" &&
    args.container == ".*" &&
    args.namespace == "default" &&
    !args.all_namespaces &&
    args.resource.is_none() &&
    !args.list_containers
}

/// Prints a tabular help message
fn print_tabular_help() {
    let mut t = HelpTable::new();
    t.load_preset(HELP_UTF8_FULL)
        .set_content_arrangement(HelpContentArrangement::Dynamic)
        .set_header(vec!["Argument", "Description"]);
    let mut add = |arg: &str, desc: &str| {
        t.add_row(vec![HelpCell::new(arg), HelpCell::new(desc)]);
    };
    add("POD_SELECTOR", "Pod selector regular expression (positional), default: .* ");
    add("-c, --container <REGEX>", "Container selector regex, default: .* ");
    add("-s, --sample <N>", "Randomly sample up to N matching pods (default: all)");
    add("-L, --list-containers", "List all containers in matched pods (no streaming)");
    add("--all-containers", "Show logs from all containers in pods");
    add("-n, --namespace <NAME>", "Kubernetes namespace (default: current context)");
    add("-A, --all-namespaces", "Show logs from all namespaces");
    add("-k, --kubeconfig <PATH>", "Path to kubeconfig file");
    add("-x, --context <NAME>", "Kubernetes context to use");
    add("-t, --tail <LINES>", "Lines of logs to display from beginning (default: 10)");
    add("-f, --follow [true|false]", "Follow logs (stream) default: true");
    add("-i, --include <PATTERN>", "Include logs matching advanced pattern (&&, ||, !, quotes, regex)");
    add("-e, --exclude <PATTERN>", "Exclude logs matching advanced pattern");
    add("-T, --timestamps", "Show timestamps in logs");
    add("-o, --output <FORMAT>", "Output format (text, json, raw), default: text");
    add("-w, --output-file <FILE>", "Write logs to file (use with --ui for both file and UI)");
    add("-r, --resource <KIND/NAME>", "Select pods by resource owner (deploy/foo, sts/bar, etc.)");
    add("--exec-template <NAME>", "Execute predefined template (jfr, heap-dump, thread-dump)");
    add("--template-args <ARGS>...", "Arguments to pass to the template");
    add("--list-templates", "List available templates");
    add("--template-output <DIR>", "Directory to save template outputs");
    add("--since <DURATION>", "Show logs since duration (e.g., 5s, 2m, 3h)");
    add("--threads <N>", "Threads for log filtering (default: 2x CPU cores)");
    add("--ui", "Enable interactive UI mode with dynamic filtering");
    add("--no-ui", "Disable UI and force CLI output");
    add("--dev", "Enable development mode (internal logs)");
    add("--buffer-size <N>", "Number of log entries to keep in memory (default: 20000)");
    add("-v, --verbosity <LEVEL>", "Verbosity for internal debug output (default: 0)");
    add("--script-in <PATH>", "Run a script in each selected pod and collect output");
    add("--script-outdir <DIR>", "Directory to save script outputs (overrides config)");
    add("--his [QUERY]", "Show command history or search saved commands using TF-IDF");
    add("--web", "Send filtered logs to web endpoint via HTTP (configure with 'wake setconfig web.*')");
    add("-h, --help", "Print this help");
    add("-V, --version", "Print version");

    println!("{}", t);

    println!("\nExamples:");
    println!("  wake -n kube-system \"kube-proxy\"                # Tail logs for kube-proxy in kube-system namespace");
    println!("  wake -A -i \"error\"                           # Tail logs across all namespaces, including 'error'");
    println!("  wake --ui -o json                                # Use interactive UI mode with JSON output");
    println!("  wake --his \"config\"                           # Search command history for 'config'");
    println!("  wake \"my-app\" -i \"error\" --web              # Send error logs to configured web endpoint");

    println!("\nWeb Mode Examples:");
    println!("  # First configure the web endpoint:");
    println!("  wake setconfig web.endpoint \"https://logs.company.com/ingest\"");
    println!("  wake setconfig web.batch_size 20");
    println!("  wake setconfig web.timeout_seconds 60");
    println!("  ");
    println!("  Then run wake in web mode:");
    println!("  wake --web");
    println!("  Access OpenObserve UI at: http://localhost:5080");

    println!("\nWeb Mode Setup (OpenObserve):");
    println!("  First, start OpenObserve with Docker:");
    println!("  docker run -d \\");
    println!("        --name openobserve \\");
    println!("        -v $PWD/data:/data \\");
    println!("        -p 5080:5080 \\");
    println!("        -e ZO_ROOT_USER_EMAIL=\"root@example.com\" \\");
    println!("        -e ZO_ROOT_USER_PASSWORD=\"Complexpass#123\" \\");
    println!("        public.ecr.aws/zinclabs/openobserve:latest");
    println!();
    println!("  Then run wake in web mode:");
    println!("  wake --web");
    println!("  Access OpenObserve UI at: http://localhost:5080");
    println!("  Stream name: logs_wake_YYYY_MM_DD (auto-generated daily)");

    println!("\nConfiguration Commands:");
    println!("  wake setconfig <key> <value> [--path <path>]      # Set a configuration key to a value");
    println!("  wake getconfig [<key>]                           # Get the value of a configuration key or all keys");
    // Examples
    println!("  wake setconfig autosave true                      # Enable autosave with auto-generated filenames");
    println!("  wake setconfig autosave true --path /var/logs     # Enable autosave with custom directory");
    println!("  wake setconfig autosave false                     # Disable autosave");
    println!("  wake setconfig ui-buffer-expansion 10             # Buffer grows 10x in pause mode for UI");
    println!("  wake setconfig ui-buffer-expansion 5              # Buffer grows 5x in pause mode for UI");
    println!("  wake setconfig script_outdir /tmp/wake-results    # Set default script output directory");
    println!("  wake getconfig                                    # Show all configuration");
    println!("  wake getconfig autosave                           # Show only autosave configuration");
    println!("  wake getconfig ui-buffer-expansion                # Show only buffer expansion setting");

    println!("\nTF-IDF Search Details:");
    println!("  • Use --his \"query\" to search command history intelligently.");
    println!("  • Example: wake --his \"error logs\" to find commands related to error logging.");
    println!("  • Supports contextual suggestions when no exact matches are found.");
}

/// Run a script in all selected pods and collect outputs as a zip file
async fn run_script_in_pods(args: &Args) -> Result<()> {
    println!("[wake] Starting script-in workflow");
    let client = crate::k8s::create_client(args).await?;
    let pod_regex = args.pod_regex()?;
    let container_regex = args.container_regex()?;
    let pods = select_pods(
        &client,
        &args.namespace,
        &pod_regex,
        &container_regex,
        args.all_namespaces,
        args.resource.as_deref(),
        args.sample,
    ).await?;

    let script_path = args.script_in.as_ref().expect("script_in should be Some");
    let script_data = std::fs::read(script_path)?;
    let outdir = if let Some(ref cli_dir) = args.script_outdir {
        cli_dir.clone()
    } else {
        let config = crate::config::Config::load().unwrap_or_default();
        if let Ok(dir) = config.get_value("script_outdir") {
            Path::new(&dir).to_path_buf()
        } else {
            std::env::current_dir()?
        }
    };
    std::fs::create_dir_all(&outdir)?;
    let timestamp = Local::now().format("%Y%m%d_%H%M%S");
    let output_dir = outdir.join(format!("wake_output_{}", timestamp));
    std::fs::create_dir_all(&output_dir)?;

    for pod in pods {
        let pod_name = &pod.name;
        let ns = &pod.namespace;
        let containers = &pod.containers;
        let container = containers.get(0).cloned().unwrap_or_else(|| "default".to_string());
        let pods_api: Api<Pod> = Api::namespaced(client.clone(), ns);

        let script_str = String::from_utf8_lossy(&script_data);
        let copy_cmd = format!("echo '{}' > /tmp/wake-script.sh && chmod +x /tmp/wake-script.sh", script_str.replace("'", "'\\''"));
        let mut copy_out = pods_api.exec(
            pod_name,
            ["sh", "-c", &copy_cmd],
            &kube::api::AttachParams::default().container(&container),
        ).await?;
        let mut _dummy = Vec::new();
        if let Some(mut s) = copy_out.stdout().take() {
            tokio::io::copy(&mut s, &mut _dummy).await?;
        }
        if let Some(mut s) = copy_out.stderr().take() {
            tokio::io::copy(&mut s, &mut _dummy).await?;
        }

        let mut exec_out = pods_api.exec(
            pod_name,
            ["sh", "/tmp/wake-script.sh"],
            &kube::api::AttachParams::default().container(&container),
        ).await?;
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        if let Some(mut s) = exec_out.stdout().take() {
            tokio::io::copy(&mut s, &mut stdout).await?;
        }
        if let Some(mut s) = exec_out.stderr().take() {
            tokio::io::copy(&mut s, &mut stderr).await?;
        }
        let out_file = output_dir.join(format!("{}_{}.stdout.txt", ns, pod_name));
        let err_file = output_dir.join(format!("{}_{}.stderr.txt", ns, pod_name));
        std::fs::write(&out_file, &stdout)?;
        std::fs::write(&err_file, &stderr)?;
    }
    println!("All pod outputs saved in {}", output_dir.display());
    Ok(())
}

pub async fn run(mut args: Args) -> Result<()> {
    // Show custom tabular help and exit early
    if args.help {
        print_tabular_help();
        return Ok(());
    }

    // Store command in history before execution (save early to prevent data loss)
    store_command_in_history(&args)?;
    
    if args.author {
        let author_path = std::path::Path::new("author.txt");
        if let Ok(content) = std::fs::read_to_string(author_path) {
            println!("{}", content);
        } else {
            println!("samba\nGitHub: https://github.com/samba-rgb\n");
        }
        return Ok(());
    }

    // Handle history command (--his flag)
    if let Some(ref query) = args.history {
        if query.is_empty() {
            // Show command history (wake --his)
            return handle_show_history().await;
        } else {
            // Search commands with TF-IDF (wake --his "query")
            return handle_search_commands(query).await;
        }
    }

    info!("=== CLI MODULE STARTING ===");
    info!("CLI: Received args - namespace: {}, pod_selector: {}, container: {}", 
          args.namespace, args.pod_selector, args.container);
    info!("CLI: UI flags - ui: {}, no_ui: {}, output_file: {:?}", 
          args.ui, args.no_ui, args.output_file);
    info!("CLI: Web flags - web: {}", args.web);

    // Validate web mode arguments
    if args.web {
        // Load config to check web endpoint
        let config = crate::config::Config::load().unwrap_or_default();
        let endpoint = config.get_value("web.endpoint").unwrap_or_default();
        
        if endpoint.is_empty() || endpoint == "http://localhost:5080/api/default/logs/_json" {
            info!("CLI: Using default web endpoint from config");
        } else {
            info!("CLI: Using custom web endpoint from config: {}", endpoint);
        }
        
        // Web mode is incompatible with UI mode
        if args.ui {
            eprintln!("❌ Web mode (--web) cannot be used with UI mode (--ui)");
            eprintln!("   Web mode operates in CLI mode only");
            std::process::exit(1);
        }
        
        let batch_size = config.get_value("web.batch_size").unwrap_or_else(|_| "10".to_string());
        let timeout = config.get_value("web.timeout_seconds").unwrap_or_else(|_| "30".to_string());
        
        info!("CLI: Web mode enabled - endpoint: {}, batch_size: {}, timeout: {}s", 
              endpoint, batch_size, timeout);
    }

    info!("CLI: UI flags - ui: {}, no_ui: {}, output_file: {:?}", 
          args.ui, args.no_ui, args.output_file);

    // Handle configuration commands first
    if let Some(command) = &args.command {
        return handle_config_command(command).await;
    }

    // Handle template commands
    if args.list_templates {
        return handle_list_templates().await;
    }

    if let Some(ref template_name) = args.execute_template {
        let result = handle_template_execution(&args, template_name).await;
        // Return the result without forcing exit - let the program complete naturally
        return result;
    }

    // EARLY RETURN for --script-in
    if args.script_in.is_some() {
        return run_script_in_pods(&args).await;
    }
    
    // Handle monitor mode (-m flag)
    if args.monitor {
        info!("CLI: Monitor mode (-m) enabled");
        return handle_monitor_mode(&args).await;
    }
    
    // Resolve namespace based on context if specified
    if let Some(ref context_name) = args.context {
        info!("CLI: Context specified: {}", context_name);
        
        // Get the namespace from the specified context
        if let Some(context_namespace) = crate::k8s::client::get_context_namespace(Some(context_name)) {
            info!("CLI: Resolving namespace from context '{}': {} -> {}", 
                  context_name, args.namespace, context_namespace);
            args.namespace = context_namespace;
        } else {
            info!("CLI: No namespace found in context '{}', keeping current: {}", 
                  context_name, args.namespace);
        }
    } else {
        info!("CLI: No context specified, using namespace: {}", args.namespace);
    }
    
    info!("CLI: Final namespace resolved: {}", args.namespace);
    
    // Determine UI behavior - CLI is now the default, UI only when explicitly requested
    let should_use_ui = if args.no_ui {
        // If --no-ui is explicitly specified, force CLI mode
        info!("CLI: Using CLI mode (--no-ui specified)");
        false
    } else if args.ui {
        // If --ui is explicitly specified, use UI mode
        info!("CLI: Using UI mode (--ui specified)");
        true
    } else {
        // Default behavior: use CLI mode
        info!("CLI: Using CLI mode (default behavior)");
        false
    };
    
    // Print the big text WAKE in CLI mode (default behavior)
    if !should_use_ui {
        print_wake_big_text();
    }
    
    info!("CLI: Final decision - should_use_ui: {}", should_use_ui);
    
    // If running with default options and no output file, show help message and exit
    if is_default_run(&args) && args.output_file.is_none() && !args.ui && !should_use_ui {
        info!("CLI: Showing help message and exiting (default run)");
        println!("No filters specified. Use arguments to begin watching pods.");
        println!("Example: wake -n kube-system \"kube-proxy\"");
        println!("Run with --help for more information.");
        return Ok(());
    }
    
    // Initialize kubernetes client
    info!("CLI: Creating Kubernetes client...");
    let client = crate::k8s::create_client(&args).await?;
    info!("CLI: Kubernetes client created successfully");
    
    // Get the pod regex
    let pod_regex = args.pod_regex().context("Invalid pod selector regex")?;
    info!("CLI: Pod regex compiled: {:?}", pod_regex.as_str());
    
    // If list_containers flag is set, just list containers and exit
    if args.list_containers {
        info!("CLI: Listing containers and exiting");
        let container_regex = args.container_regex().ok();
        return crate::k8s::pod::list_container_names(
            &client, 
            &args.namespace, 
            &pod_regex, 
            args.all_namespaces,
            args.resource.as_deref(),
            container_regex.as_ref(),
            args.sample,
        ).await;
    }
    
    // Set up log watcher
    info!("CLI: Creating LogWatcher...");
    let watcher = crate::k8s::LogWatcher::new(client, &args);
    
    // Stream the logs
    info!("CLI: Starting log stream...");
    let log_streams = watcher.stream().await?;
    info!("CLI: Log stream created successfully");
    
    // Handle different output modes
    if should_use_ui {
        info!("CLI: Starting UI mode...");
        if args.output_file.is_some() {
            println!("Starting UI mode with file output to: {:?}", args.output_file);
        }
        // Use the interactive UI with dynamic filtering
        crate::ui::run_with_ui(log_streams, args).await?;
        info!("CLI: UI mode completed");
    } else {
        info!("CLI: Starting CLI mode...");
        // Use CLI mode with static filtering
        let formatter = crate::output::Formatter::new(&args);
        crate::logging::process_logs(log_streams, &args, formatter).await?;
        info!("CLI: CLI mode completed");
    }
    
    info!("=== CLI MODULE COMPLETED ===");
    Ok(())
}

/// Handle monitor mode
async fn handle_monitor_mode(args: &Args) -> Result<()> {
    info!("CLI: Starting monitor mode...");
    
    // Use the existing pod selection mechanism
    crate::ui::run_with_monitor_ui(args.clone()).await?;
    
    info!("CLI: Monitor mode completed");
    Ok(())
}

/// Handle configuration commands (setconfig, getconfig)
async fn handle_config_command(command: &crate::cli::args::Commands) -> Result<()> {
    use crate::cli::args::Commands;
    use crate::config::Config;
    
    match command {
        Commands::SetConfig { key, value, path } => {
            let mut config = Config::load().context("Failed to load configuration")?;
            
            // Handle special cases that need custom logic
            match key.to_lowercase().as_str() {
                "autosave" => {
                    let enabled = match value.to_lowercase().as_str() {
                        "true" | "1" | "yes" | "on" | "enable" | "enabled" => true,
                        "false" | "0" | "no" | "off" | "disable" | "disabled" => false,
                        _ => {
                            eprintln!("❌ Invalid value for autosave: '{}'. Use 'true' or 'false'", value);
                            std::process::exit(1);
                        }
                    };
                    
                    config.set_autosave(enabled, path.clone());
                    config.save().context("Failed to save configuration")?;
                    
                    if enabled {
                        if let Some(path_str) = path {
                            println!("✅ Autosave enabled with custom path: {}", path_str);
                        } else {
                            println!("✅ Autosave enabled with auto-generated filenames (wake_TIMESTAMP.log)");
                        }
                    } else {
                        println!("✅ Autosave disabled");
                    }
                }
                _ => {
                    // Use the automatic configuration system for all other keys
                    match config.set_value(key, value) {
                        Ok(()) => {
                            config.save().context("Failed to save configuration")?;
                            println!("✅ Configuration updated: {} = {}", key, value);
                            
                            // Provide helpful context for specific settings
                            match key {
                                k if k.contains("buffer_expansion") => {
                                    println!("💡 In pause mode, the buffer will expand to hold {}x more logs for better browsing", value);
                                }
                                k if k.contains("theme") => {
                                    println!("🎨 UI theme set to: {}", value);
                                }
                                k if k.contains("show_timestamps") => {
                                    println!("🕒 Default timestamp display: {}", value);
                                }
                                _ => {}
                            }
                        }
                        Err(e) => {
                            eprintln!("❌ Failed to set configuration: {}", e);
                            eprintln!("\nAvailable keys:");
                            let all_keys = config.get_all_keys();
                            for available_key in &all_keys {
                                eprintln!("  - {}", available_key);
                            }
                            std::process::exit(1);
                        }
                    }
                }
            }
        }
        Commands::GetConfig { key } => {
            let config = Config::load().context("Failed to load configuration")?;
            match key {
                Some(key_name) => {
                    match config.display_key(key_name) {
                        Ok(output) => print!("{}", output),
                        Err(e) => {
                            eprintln!("❌ {}", e);
                            eprintln!("\nAvailable keys:");
                            let all_keys = config.get_all_keys();
                            for available_key in &all_keys {
                                eprintln!("  - {}", available_key);
                            }
                            std::process::exit(1);
                        }
                    }
                }
                None => {
                    // Display all configuration in a pretty table using comfy-table
                    let mut table = Table::new();
                    table.set_header(["Key", "Value"]);
                    for key in config.get_all_keys() {
                        if let Ok(val) = config.display_key(&key) {
                            table.add_row([key, val.trim().to_string()]);
                        }
                    }
                    println!("{}", table);
                }
            }
        }
    }
    
    Ok(())
}

/// Handle listing all available templates
async fn handle_list_templates() -> Result<()> {
    use crate::templates::registry::TemplateRegistry;
    
    println!("📋 Available Wake Templates:");
    println!();
    
    let registry = TemplateRegistry::with_builtins();
    let templates = registry.get_all_templates();
    
    if templates.is_empty() {
        println!("No templates available.");
        return Ok(());
    }
    
    // Create a nice table for templates
    let mut table = Table::new();
    table.set_header(["Template", "Description", "Parameters"]);
    
    for (name, template) in templates {
        let params = template.parameters
            .iter()
            .map(|p| format!("{}:{}", p.name, match p.param_type {
                crate::templates::ParameterType::Integer => "int",
                crate::templates::ParameterType::String => "str", 
                crate::templates::ParameterType::Duration => "duration",
                crate::templates::ParameterType::Path => "path",
                crate::templates::ParameterType::Boolean => "bool",
            }))
            .collect::<Vec<_>>()
            .join(", ");
            
        table.add_row([name.clone(), template.description.clone(), params]);
    }
    
    println!("{}", table);
    println!();
    println!("💡 Usage examples:");
    println!("  wake -t thread-dump 1234");
    println!("  wake -t jfr 1234 30s --template-output ./output");
    println!("  wake -t heap-dump 1234 -n my-namespace");
    
    Ok(())
}

/// Handle template execution
async fn handle_template_execution(args: &Args, template_name: &str) -> Result<()> {
    use crate::templates::registry::TemplateRegistry;
    use crate::templates::executor::TemplateExecutor;
    use crate::k8s::pod::select_pods;
    
    println!("🚀 Executing template: {}", template_name);
    
    // Initialize template system
    let registry = TemplateRegistry::with_builtins();
    let template_executor = TemplateExecutor::new(registry);
    
    // Check if template exists
    if (!template_executor.list_templates().contains(&template_name)) {
        eprintln!("❌ Template '{}' not found.", template_name);
        eprintln!();
        eprintln!("Available templates:");
        for available_template in template_executor.list_templates() {
            eprintln!("  - {}", available_template);
        }
        eprintln!();
        eprintln!("Use --list-templates to see detailed information about each template.");
        std::process::exit(1);
    }
    
    // Get the kubernetes client
    let client = crate::k8s::client::create_client(args).await?;
    
    // Select pods based on the provided criteria
    let pod_regex = args.pod_regex().context("Invalid pod selector regex")?;
    let container_regex = args.container_regex().context("Invalid container regex")?;
    
    let pods = select_pods(
        &client,
        &args.namespace,
        &pod_regex,
        &container_regex,
        args.all_namespaces,
        args.resource.as_deref(),
        args.sample,
    ).await?;
    
    if pods.is_empty() {
        eprintln!("❌ No pods found matching the criteria.");
        eprintln!("   Namespace: {}", args.namespace);
        eprintln!("   Pod selector: {}", args.pod_selector);
        eprintln!("   Container: {}", args.container);
        std::process::exit(1);
    }
    
    println!("📍 Found {} pod(s) to execute template on:", pods.len());
    for pod in &pods {
        println!("  - {}/{}", pod.namespace, pod.name);
    }
    println!();
    
    // Execute the template
    let result = template_executor.execute_template(
        template_name,
        args.template_args.clone(),
        &pods,
        args.template_output.clone(),
        args,
    ).await;
    
    match result {
        Ok(execution_result) => {
            println!("✅ Template execution completed!");
            println!("📁 Results saved to: {}", execution_result.output_dir.display());
            
            let successful = execution_result.pod_results.iter().filter(|r| r.success).count();
            let failed = execution_result.pod_results.len() - successful;
            
            println!();
            println!("📊 Execution Summary:");
            println!("  ✅ Successful: {}", successful);
            if failed > 0 {
                println!("  ❌ Failed: {}", failed);
            }
            println!("  📁 Output directory: {}", execution_result.output_dir.display());
        }
        Err(e) => {
            eprintln!("❌ Template execution failed: {}", e);
            std::process::exit(1);
        }
    }
    
    Ok(())
}

/// Store command in history before execution
fn store_command_in_history(args: &Args) -> Result<()> {
    // Reconstruct the original command from args
    let mut command_parts = vec!["wake".to_string()];
    
    // Add subcommands first
    if let Some(ref cmd) = args.command {
        match cmd {
            crate::cli::args::Commands::SetConfig { key, value, path } => {
                command_parts.push("setconfig".to_string());
                command_parts.push(key.clone());
                command_parts.push(value.clone());
                if let Some(p) = path {
                    command_parts.push("--path".to_string());
                    command_parts.push(p.clone());
                }
            }
            crate::cli::args::Commands::GetConfig { key } => {
                command_parts.push("getconfig".to_string());
                if let Some(k) = key {
                    command_parts.push(k.clone());
                }
            }
        }
    } else {
        // Add regular flags and arguments
        if args.pod_selector != ".*" {
            command_parts.push(args.pod_selector.clone());
        }
        
        if args.container != ".*" {
            command_parts.push("-c".to_string());
            command_parts.push(args.container.clone());
        }
        
        if let Some(s) = args.sample {
            command_parts.push("-s".to_string());
            command_parts.push(s.to_string());
        }
        
        if args.namespace != "default" {
            command_parts.push("-n".to_string());
            command_parts.push(args.namespace.clone());
        }
        
        if args.all_namespaces {
            command_parts.push("-A".to_string());
        }
        
        if let Some(ref include) = args.include {
            command_parts.push("-i".to_string());
            command_parts.push(include.clone());
        }
        
        if let Some(ref exclude) = args.exclude {
            command_parts.push("-e".to_string());
            command_parts.push(exclude.clone());
        }
        
        if args.ui {
            command_parts.push("--ui".to_string());
        }
        
        if let Some(ref output_file) = args.output_file {
            command_parts.push("-w".to_string());
            command_parts.push(output_file.display().to_string());
        }
        
        if args.timestamps {
            command_parts.push("-T".to_string());
        }
        
        if let Some(ref template) = args.execute_template {
            command_parts.push("--exec-template".to_string());
            command_parts.push(template.clone());
            for arg in &args.template_args {
                command_parts.push(arg.clone());
            }
        }
        
        if args.list_templates {
            command_parts.push("--list-templates".to_string());
        }
        
        if let Some(ref history) = args.history {
            command_parts.push("--his".to_string());
            if !history.is_empty() {
                command_parts.push(history.clone());
            }
        }
    }
    
    let command_str = command_parts.join(" ");
    
    // Load config and add to history
    let mut config = crate::config::Config::load().unwrap_or_default();
    config.add_command_to_history(command_str);
    
    // Save config silently (without printing message)
    let _ = config.save_silent();
    
    Ok(())
}

/// Handle showing command history (wake --his)
async fn handle_show_history() -> Result<()> {
    let config = crate::config::Config::load().unwrap_or_default();
    let history = config.get_command_history();
    
    println!("📜 Wake Command History");
    println!("=======================");
    println!();
    
    if history.is_empty() {
        println!("No commands found in history.");
        println!();
        println!("💡 Tips:");
        println!("  • Command history is automatically stored when you run wake commands");
        println!("  • History is limited to the last {} commands", config.history.max_entries);
        println!("  • Use --his \"search query\" to find specific commands");
        return Ok(());
    }
    
    println!("Found {} command(s) in history:", history.len());
    println!();
    
    // Show recent commands (limit to last 50 for display, in descending order - newest first)
    let display_count = std::cmp::min(history.len(), 50);
    for (i, entry) in history.iter().take(display_count).enumerate() {
        let time_ago = format_time_ago(&entry.timestamp);
        println!("{:3}. {} {}", i + 1, entry.command, 
                 format!("({})", time_ago).as_str().dimmed());
    }
    
    if history.len() > 50 {
        println!();
        println!("... (showing last 50 of {} total commands)", history.len());
    }
    
    Ok(())
}

/// Handle searching commands with TF-IDF (wake --his "query")
async fn handle_search_commands(query: &str) -> Result<()> {
    use crate::search::TfIdfSearcher;
    
    // Initialize TF-IDF searcher
    let searcher = match TfIdfSearcher::new() {
        Ok(s) => s,
        Err(e) => {
            println!("❌ Search functionality not available: {}", e);
            println!();
            println!("💡 This might be because:");
            println!("  • The static commands database wasn't built during compilation");
            println!("  • Try rebuilding with: cargo build --release");
            return Ok(());
        }
    };
    
    // Perform search
    if let Some(result) = searcher.search(query) {
        println!("🚀 Command: {}", result.command.green());
        println!("📝 Description: {}", result.description);
    } else {
        println!("❌ No matching commands found for \"{}\"", query);
        println!();
        println!("💡 Try searching with different terms:");
        println!("  • \"error\" instead of \"error logs\"");
        println!("  • \"namespace\" instead of \"kube-system\""); 
        println!("  • \"ui\" instead of \"interactive mode\"");
        println!();
        println!("📚 Available command categories:");
        
        // Show some example categories from the static commands
        let all_commands = searcher.get_all_commands();
        let mut categories: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        
        for cmd in all_commands.iter().take(10) {
            if cmd.command.contains("-i") { *categories.entry("filtering").or_insert(0) += 1; }
            if cmd.command.contains("-n") { *categories.entry("namespaces").or_insert(0) += 1; }
            if cmd.command.contains("--ui") { *categories.entry("ui mode").or_insert(0) += 1; }
            if cmd.command.contains("-w") { *categories.entry("file output").or_insert(0) += 1; }
        }
        
        for (category, _) in categories {
            println!("  • {}", category);
        }
    }
    
    Ok(())
}

/// Format time ago in human readable format
fn format_time_ago(timestamp: &chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    let duration = now.signed_duration_since(*timestamp);
    
    if duration.num_days() > 0 {
        format!("{} days ago", duration.num_days())
    } else if duration.num_hours() > 0 {
        format!("{} hours ago", duration.num_hours())
    } else if duration.num_minutes() > 0 {
        format!("{} minutes ago", duration.num_minutes())
    } else {
        "just now".to_string()
    }
}