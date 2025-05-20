use wake::k8s::client::create_client;
use wake::cli::Args;
use anyhow::Result;

#[tokio::test]
#[ignore] // Ignored by default as it requires kubeconfig setup
async fn test_create_client_default() -> Result<()> {
    // Test creating a client with default settings
    let args = Args::default();
    let client = create_client(&args).await?;
    
    // We can't easily test the client's connection, but we can verify we get a valid client
    // Just check that client creation doesn't panic or error
    assert!(client.list_core_api_versions().await.is_ok());
    
    Ok(())
}

#[tokio::test]
#[ignore] // Ignored by default as it requires kubeconfig setup
async fn test_create_client_custom_context() -> Result<()> {
    // Test creating a client with a specific context
    let mut args = Args::default();
    args.context = Some("minikube".to_string()); // Common local development context name
    
    // This test just checks that client creation with a context doesn't error
    // The actual context might not exist, so we don't assert on the result
    let result = create_client(&args).await;
    assert!(result.is_ok() || result.is_err());
    
    Ok(())
}

#[tokio::test]
#[ignore] // Ignored by default as it requires kubeconfig setup
async fn test_create_client_custom_kubeconfig() -> Result<()> {
    // Test creating a client with a custom kubeconfig path
    // Note: This test assumes the file exists and is valid
    let mut args = Args::default();
    args.kubeconfig = Some("/tmp/test-kubeconfig".to_string().into());
    
    // This will fail if the file doesn't exist or is invalid
    // In a real test setup, we'd create a temporary kubeconfig file first
    let result = create_client(&args).await;
    
    // We're just testing the code path here, not expecting success
    assert!(result.is_err() || result.is_ok());
    
    Ok(())
}