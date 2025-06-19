use crate::cli::Args;
use anyhow::Result;
use kube::Client;
use kube::config::{KubeConfigOptions, Kubeconfig};
use std::path::Path;
use tracing::info;

/// Creates a Kubernetes client based on provided arguments
pub async fn create_client(args: &Args) -> Result<Client> {
    info!("Creating Kubernetes client");
    
    if let Some(path) = &args.kubeconfig {
        return create_client_from_kubeconfig(path, args.context.as_deref()).await;
    }
    
    // If context is specified but no kubeconfig path, use default kubeconfig with specified context
    if args.context.is_some() {
        info!("Using specified context with default kubeconfig location");
        let default_kubeconfig_path = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?
            .join(".kube/config");
        return create_client_from_kubeconfig(&default_kubeconfig_path, args.context.as_deref()).await;
    }
    
    // Default to inferred config from environment (uses current-context from kubeconfig)
    info!("No context specified, using default context from kubeconfig");
    let client = Client::try_default().await?;
    info!("Using default Kubernetes client configuration");
    Ok(client)
}

/// Creates a client from a specific kubeconfig file
async fn create_client_from_kubeconfig(path: &Path, context: Option<&str>) -> Result<Client> {
    info!("Loading kubeconfig from: {:?}", path);
    
    let kubeconfig = Kubeconfig::read_from(path)?;
    
    let options = match context {
        Some(ctx) => {
            info!("Using context: {}", ctx);
            KubeConfigOptions {
                context: Some(ctx.to_string()),
                ..Default::default()
            }
        },
        None => KubeConfigOptions::default(),
    };
    
    let client_config = kube::Config::from_custom_kubeconfig(kubeconfig, &options).await?;
    let client = Client::try_from(client_config)?;
    Ok(client)
}

/// Gets the default namespace from the current Kubernetes context
pub fn get_current_context_namespace() -> Option<String> {
    get_context_namespace(None)
}

/// Gets the namespace from a specific context, or the current context if none specified
pub fn get_context_namespace(context_name: Option<&str>) -> Option<String> {
    // Try to read from default kubeconfig location
    if let Some(home_dir) = dirs::home_dir() {
        let default_kubeconfig_path = home_dir.join(".kube/config");
        if let Ok(kubeconfig) = Kubeconfig::read_from(&default_kubeconfig_path) {
            // Determine which context to use
            let target_context_name = match context_name {
                Some(ctx) => ctx,
                None => kubeconfig.current_context.as_deref()?,
            };
            
            // Find the context and get its namespace
            if let Some(named_context) = kubeconfig.contexts.iter()
                .find(|ctx| ctx.name == target_context_name) {
                // The context field is Option<Context>, so we need to unwrap it
                if let Some(context) = &named_context.context {
                    return context.namespace.clone();
                }
            }
        }
    }
    
    // If no kubeconfig or no namespace in context, return None
    // This will cause the caller to fall back to "default"
    None
}