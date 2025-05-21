mod args;

pub use args::{Args, parse_args};
use anyhow::{Result, Context};

/// Prints WAKE in big text with dots
fn print_wake_big_text() {
    println!("................................................................
                                              
█|          █|    █|█|    █|    █|  █|█|█|█|  
█|          █|  █|    █|  █|  █|    █|        
█|    █|    █|  █|█|█|█|  █|█|      █|█|█|    
  █|  █|  █|    █|    █|  █|  █|    █|        
    █|  █|      █|    █|  █|    █|  █|█|█|█| 
                                              
                                               by: Samba ");
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

pub async fn run(args: Args) -> Result<()> {
    // Always print the big text WAKE
    print_wake_big_text();
    
    // If running with default options, show help message and exit
    if is_default_run(&args) {
        println!("No filters specified. Use arguments to begin watching pods.");
        println!("Example: wake -n kube-system \"kube-proxy\"");
        println!("Run with --help for more information.");
        return Ok(());
    }
    
    // Initialize kubernetes client
    let client = crate::k8s::create_client(&args).await?;
    
    // Get the pod regex
    let pod_regex = args.pod_regex().context("Invalid pod selector regex")?;
    
    // If list_containers flag is set, just list containers and exit
    if args.list_containers {
        return crate::k8s::pod::list_container_names(
            &client, 
            &args.namespace, 
            &pod_regex, 
            args.all_namespaces,
            args.resource.as_deref()
        ).await;
    }
    
    // Set up log watcher
    let watcher = crate::k8s::LogWatcher::new(client, &args);
    
    // Stream the logs
    let log_streams = watcher.stream().await?;
    
    // Create output formatter
    let formatter = crate::output::Formatter::new(&args);
    
    // Process and display logs with the new threaded filtering pipeline
    crate::logging::process_logs(log_streams, &args, formatter).await?;
    
    Ok(())
}