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
use comfy_table::Table;

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
    if args.author {
        let author_path = std::path::Path::new("author.txt");
        if let Ok(content) = std::fs::read_to_string(author_path) {
            println!("{}", content);
        } else {
            println!("samba\nGitHub: https://github.com/samba-rgb\n");
        }
        return Ok(());
    }

    info!("=== CLI MODULE STARTING ===");
    info!("CLI: Received args - namespace: {}, pod_selector: {}, container: {}", 
          args.namespace, args.pod_selector, args.container);
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
    if !template_executor.list_templates().contains(&template_name) {
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