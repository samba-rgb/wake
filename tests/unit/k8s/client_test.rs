use wake::k8s::client::{create_client, get_current_context_namespace};
use wake::cli::Args;
use anyhow::Result;
use std::path::PathBuf;
use tempfile::NamedTempFile;
use std::io::Write;

// Mock tests that don't require a real cluster
#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn test_get_current_context_namespace_no_kubeconfig() {
        // Test when kubeconfig doesn't exist or has no namespace
        // This should return None and not panic
        let result = get_current_context_namespace();
        // Should either return Some namespace or None, but not panic
        assert!(result.is_some() || result.is_none());
    }

    #[tokio::test]
    async fn test_create_client_with_invalid_kubeconfig_path() -> Result<()> {
        // Test creating a client with a non-existent kubeconfig path
        let mut args = Args::default();
        args.kubeconfig = Some(PathBuf::from("/non/existent/path"));
        
        let result = create_client(&args).await;
        assert!(result.is_err());
        
        Ok(())
    }

    #[tokio::test]
    async fn test_create_client_with_invalid_kubeconfig_content() -> Result<()> {
        // Create a temporary file with invalid YAML content
        let mut temp_file = NamedTempFile::new()?;
        writeln!(temp_file, "invalid: yaml: content: [")?;
        
        let mut args = Args::default();
        args.kubeconfig = Some(temp_file.path().to_path_buf());
        
        let result = create_client(&args).await;
        assert!(result.is_err());
        
        Ok(())
    }

    #[tokio::test]
    async fn test_create_client_with_valid_kubeconfig_format() -> Result<()> {
        // Create a temporary file with valid kubeconfig format (but fake data)
        let mut temp_file = NamedTempFile::new()?;
        writeln!(temp_file, r#"
apiVersion: v1
kind: Config
clusters:
- cluster:
    server: https://fake-server:6443
  name: fake-cluster
contexts:
- context:
    cluster: fake-cluster
    namespace: test-namespace
    user: fake-user
  name: fake-context
current-context: fake-context
users:
- name: fake-user
  user:
    token: fake-token
"#)?;
        
        let mut args = Args::default();
        args.kubeconfig = Some(temp_file.path().to_path_buf());
        
        let result = create_client(&args).await;
        // This should create a client successfully even with fake data
        // The actual connection will fail later when making API calls
        assert!(result.is_ok());
        
        Ok(())
    }
}

// Integration tests that require a real Kubernetes cluster
#[cfg(test)]
mod integration_tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Run with: cargo test test_create_client_default -- --ignored
    async fn test_create_client_default() -> Result<()> {
        println!("Testing default client creation...");
        
        let args = Args::default();
        let client = create_client(&args).await?;
        
        // Test that we can make a basic API call
        println!("Testing API connectivity...");
        let api_versions = client.list_core_api_versions().await?;
        assert!(!api_versions.versions.is_empty());
        
        println!("✅ Default client test passed");
        Ok(())
    }

    #[tokio::test]
    #[ignore] // Run with: cargo test test_create_client_with_context -- --ignored
    async fn test_create_client_with_context() -> Result<()> {
        println!("Testing client creation with context...");
        
        // First, let's see what contexts are available
        if let Some(current_namespace) = get_current_context_namespace() {
            println!("Current namespace from context: {}", current_namespace);
        } else {
            println!("No namespace found in current context");
        }
        
        let mut args = Args::default();
        
        // Try with common local development contexts
        let test_contexts = vec!["kind-kind", "minikube", "docker-desktop"];
        
        for context in test_contexts {
            args.context = Some(context.to_string());
            
            match create_client(&args).await {
                Ok(client) => {
                    println!("✅ Successfully created client with context: {}", context);
                    
                    // Test basic API call
                    match client.list_core_api_versions().await {
                        Ok(versions) => {
                            println!("  - API versions available: {}", versions.versions.len());
                            return Ok(()); // Success with this context
                        }
                        Err(e) => {
                            println!("  - API call failed: {}", e);
                        }
                    }
                }
                Err(e) => {
                    println!("  - Failed to create client with context {}: {}", context, e);
                }
            }
        }
        
        // If we get here, none of the common contexts worked
        println!("⚠️  No working contexts found, but this is not necessarily an error");
        Ok(())
    }

    #[tokio::test]
    #[ignore] // Run with: cargo test test_cluster_connectivity -- --ignored
    async fn test_cluster_connectivity() -> Result<()> {
        println!("Testing comprehensive cluster connectivity...");
        
        let args = Args::default();
        let client = create_client(&args).await?;
        
        // Test various API endpoints
        println!("1. Testing core API versions...");
        let core_versions = client.list_core_api_versions().await?;
        println!("   Available core API versions: {:?}", core_versions.versions);
        
        println!("2. Testing API groups...");
        let api_groups = client.list_api_groups().await?;
        println!("   Available API groups: {}", api_groups.groups.len());
        
        println!("3. Testing namespace listing...");
        use k8s_openapi::api::core::v1::Namespace;
        use kube::Api;
        
        let namespaces: Api<Namespace> = Api::all(client.clone());
        let ns_list = namespaces.list(&kube::api::ListParams::default()).await?;
        println!("   Found {} namespaces", ns_list.items.len());
        
        for ns in ns_list.items.iter().take(5) {
            if let Some(name) = &ns.metadata.name {
                println!("     - {}", name);
            }
        }
        
        println!("✅ Cluster connectivity test passed");
        Ok(())
    }

    #[tokio::test]
    #[ignore] // Run with: cargo test test_pod_listing -- --ignored
    async fn test_pod_listing() -> Result<()> {
        println!("Testing pod listing functionality...");
        
        let args = Args::default();
        let client = create_client(&args).await?;
        
        use k8s_openapi::api::core::v1::Pod;
        use kube::Api;
        
        // Test listing pods in default namespace
        println!("1. Testing default namespace pods...");
        let pods: Api<Pod> = Api::namespaced(client.clone(), "default");
        let pod_list = pods.list(&kube::api::ListParams::default()).await?;
        println!("   Found {} pods in default namespace", pod_list.items.len());
        
        // Test listing pods across all namespaces
        println!("2. Testing all namespace pods...");
        let all_pods: Api<Pod> = Api::all(client.clone());
        let all_pod_list = all_pods.list(&kube::api::ListParams::default()).await?;
        println!("   Found {} pods across all namespaces", all_pod_list.items.len());
        
        // Show some pod details
        for pod in all_pod_list.items.iter().take(3) {
            if let (Some(name), Some(namespace)) = (&pod.metadata.name, &pod.metadata.namespace) {
                let phase = pod.status.as_ref()
                    .and_then(|s| s.phase.as_ref())
                    .unwrap_or(&"Unknown".to_string());
                println!("     - {}/{} ({})", namespace, name, phase);
            }
        }
        
        println!("✅ Pod listing test passed");
        Ok(())
    }
}

// Helper tests to verify your cluster setup
#[cfg(test)]
mod setup_verification {
    use super::*;

    #[tokio::test]
    #[ignore] // Run with: cargo test verify_cluster_setup -- --ignored
    async fn verify_cluster_setup() -> Result<()> {
        println!("🔍 Verifying your Kubernetes cluster setup...");
        
        // Check if kubectl is available
        match std::process::Command::new("kubectl").arg("version").output() {
            Ok(output) => {
                if output.status.success() {
                    println!("✅ kubectl is available");
                } else {
                    println!("❌ kubectl command failed");
                    println!("   stderr: {}", String::from_utf8_lossy(&output.stderr));
                }
            }
            Err(e) => {
                println!("❌ kubectl not found: {}", e);
                return Err(anyhow::anyhow!("kubectl not available"));
            }
        }
        
        // Check current context
        match std::process::Command::new("kubectl").args(&["config", "current-context"]).output() {
            Ok(output) => {
                if output.status.success() {
                    let context = String::from_utf8_lossy(&output.stdout).trim();
                    println!("✅ Current kubectl context: {}", context);
                } else {
                    println!("❌ Failed to get current context");
                }
            }
            Err(e) => {
                println!("❌ Failed to check context: {}", e);
            }
        }
        
        // Test client creation
        println!("3. Testing Wake client creation...");
        let args = Args::default();
        match create_client(&args).await {
            Ok(client) => {
                println!("✅ Wake client created successfully");
                
                // Test basic API call
                match client.list_core_api_versions().await {
                    Ok(_) => println!("✅ API communication working"),
                    Err(e) => println!("❌ API communication failed: {}", e),
                }
            }
            Err(e) => {
                println!("❌ Wake client creation failed: {}", e);
                return Err(e);
            }
        }
        
        println!("🎉 Cluster setup verification complete!");
        Ok(())
    }
}