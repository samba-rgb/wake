use anyhow::{Result, Context};
use k8s_openapi::api::core::v1::Pod;
use kube::{Api, Client};
use kube::api::{ListParams, ResourceExt};
use regex::Regex;
use tracing::{info, debug};

/// Represents pod information relevant to log watching
#[derive(Debug, Clone)]
pub struct PodInfo {
    pub namespace: String,
    pub name: String,
    pub containers: Vec<String>,
}

/// Filters and selects pods based on regex patterns
pub async fn select_pods(
    client: &Client, 
    namespace: &str, 
    pod_re: &Regex, 
    container_re: &Regex,
    all_namespaces: bool,
) -> Result<Vec<PodInfo>> {
    let namespaces = if all_namespaces {
        get_all_namespaces(client).await?
    } else {
        vec![namespace.to_string()]
    };
    
    let mut selected_pods = Vec::new();
    
    for ns in namespaces {
        info!("Searching for pods in namespace: {}", ns);
        let pods: Api<Pod> = Api::namespaced(client.clone(), &ns);
        
        let params = ListParams::default();
        let pod_list = pods.list(&params).await.context(format!("Failed to list pods in namespace {}", ns))?;
        
        for pod in pod_list {
            let pod_name = pod.name_unchecked();
            
            if !pod_re.is_match(&pod_name) {
                debug!("Pod {} doesn't match regex, skipping", pod_name);
                continue;
            }
            
            // Get container names
            let mut containers = Vec::new();
            
            // Check pod status for containers
            if let Some(status) = &pod.status {
                // Add running containers
                if let Some(container_statuses) = &status.container_statuses {
                    for cs in container_statuses {
                        if container_re.is_match(&cs.name) {
                            containers.push(cs.name.clone());
                        }
                    }
                }
                
                // Also check init containers
                if let Some(init_containers) = &status.init_container_statuses {
                    for cs in init_containers {
                        if container_re.is_match(&cs.name) {
                            containers.push(cs.name.clone());
                        }
                    }
                }
            }
            
            // If no containers found via status, try spec
            if containers.is_empty() {
                if let Some(spec) = &pod.spec {
                    for c in &spec.containers {
                        if container_re.is_match(&c.name) {
                            containers.push(c.name.clone());
                        }
                    }
                }
            }
            
            if !containers.is_empty() {
                info!("Selected pod {} with {} containers", pod_name, containers.len());
                selected_pods.push(PodInfo {
                    namespace: ns.clone(),
                    name: pod_name,
                    containers,
                });
            }
        }
    }
    
    info!("Total selected pods: {}", selected_pods.len());
    Ok(selected_pods)
}

/// Gets all available namespaces in the cluster
async fn get_all_namespaces(client: &Client) -> Result<Vec<String>> {
    use k8s_openapi::api::core::v1::Namespace;
    
    let namespaces: Api<Namespace> = Api::all(client.clone());
    let namespace_list = namespaces.list(&ListParams::default()).await?;
    
    let names = namespace_list
        .iter()
        .filter_map(|ns| ns.metadata.name.clone())
        .collect();
    
    Ok(names)
}

/// List container names for all pods matching the given regex
pub async fn list_container_names(
    client: &Client,
    namespace: &str,
    pod_regex: &Regex,
    all_namespaces: bool,
) -> Result<()> {
    let pods_api: Api<Pod>;
    
    if all_namespaces {
        pods_api = Api::all(client.clone());
    } else {
        pods_api = Api::namespaced(client.clone(), namespace);
    }
    
    let pods = pods_api.list(&Default::default()).await?;
    
    let mut found_pods = false;
    for pod in pods.items {
        let pod_name = pod.metadata.name.as_deref().unwrap_or("");
        if !pod_regex.is_match(pod_name) {
            continue;
        }
        
        found_pods = true;
        let pod_namespace = pod.metadata.namespace.as_deref().unwrap_or("default");
        
        println!("Pod: {}/{}", pod_namespace, pod_name);
        
        if let Some(spec) = pod.spec {
            println!("  Containers:");
            for container in spec.containers {
                println!("  - {}", container.name);
            }
            
            if let Some(init_containers) = spec.init_containers {
                println!("  Init Containers:");
                for container in init_containers {
                    println!("  - {} (init)", container.name);
                }
            }
            
            if let Some(ephemeral_containers) = spec.ephemeral_containers {
                println!("  Ephemeral Containers:");
                for container in ephemeral_containers {
                    println!("  - {} (ephemeral)", container.name);
                }
            }
            
            println!("");
        }
    }
    
    if !found_pods {
        println!("No pods found matching the pattern \"{}\" in namespace \"{}\"", 
                 pod_regex, if all_namespaces { "all namespaces" } else { namespace });
    }
    
    Ok(())
}