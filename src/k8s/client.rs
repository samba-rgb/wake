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
    
    // Default to inferred config from environment
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