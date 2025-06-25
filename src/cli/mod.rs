mod args;

pub use args::{Args, parse_args};
use anyhow::{Result, Context};
use tracing::info;
use std::path::Path;
use std::fs;
use regex::Regex;
use crate::k8s::pod::select_pods;
use kube::Api;
use k8s_openapi::api::core::v1::Pod;
use std::io::Write;
use std::fs::File;
use chrono::Local;
use std::collections::HashMap;

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
    info!("=== CLI MODULE STARTING ===");
    info!("CLI: Received args - namespace: {}, pod_selector: {}, container: {}", 
          args.namespace, args.pod_selector, args.container);
    info!("CLI: UI flags - ui: {}, no_ui: {}, output_file: {:?}", 
          args.ui, args.no_ui, args.output_file);
    // Handle configuration commands first
    if let Some(command) = &args.command {
        return handle_config_command(command).await;
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
        return crate::k8s::pod::list_container_names(
            &client, 
            &args.namespace, 
            &pod_regex, 
            args.all_namespaces,
            args.resource.as_deref()
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
                    // Display all configuration in tabular format
                    print!("{}", config.display());
                }
            }
        }
    }
    
    Ok(())
}