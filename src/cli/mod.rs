mod args;

pub use args::{Args, parse_args};
use anyhow::{Result, Context};
use tracing::info;

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

pub async fn run(mut args: Args) -> Result<()> {
    info!("=== CLI MODULE STARTING ===");
    info!("CLI: Received args - namespace: {}, pod_selector: {}, container: {}", 
          args.namespace, args.pod_selector, args.container);
    info!("CLI: UI flags - ui: {}, no_ui: {}, output_file: {:?}", 
          args.ui, args.no_ui, args.output_file);
    
    // Handle configuration commands first
    if let Some(command) = &args.command {
        use crate::cli::args::Commands;
        return handle_config_command(command).await;
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
                    eprintln!("❌ Unknown configuration key: '{}'", key);
                    eprintln!("Available keys:");
                    for available_key in Config::list_keys() {
                        eprintln!("  - {}", available_key);
                    }
                    std::process::exit(1);
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
                            for available_key in Config::list_keys() {
                                eprintln!("  - {}", available_key);
                            }
                            std::process::exit(1);
                        }
                    }
                }
                None => {
                    // Display all configuration
                    print!("{}", config.display());
                }
            }
        }
    }
    
    Ok(())
}