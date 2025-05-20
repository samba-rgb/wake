use anyhow::{Result, anyhow};
use kube::{Api, Client};
use k8s_openapi::api::apps::v1::{Deployment, ReplicaSet, StatefulSet, DaemonSet};
use k8s_openapi::api::batch::v1::Job;
use tracing::debug;

#[derive(Debug, PartialEq, Eq)]
/// Supported resource types for selection
pub enum ResourceType {
    Pod,
    Deployment,
    ReplicaSet,
    StatefulSet,
    Job,
    DaemonSet,
}

impl ResourceType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "pod" => Some(ResourceType::Pod),
            "deployment" | "deploy" => Some(ResourceType::Deployment),
            "replicaset" | "rs" => Some(ResourceType::ReplicaSet),
            "statefulset" | "sts" => Some(ResourceType::StatefulSet),
            "job" => Some(ResourceType::Job),
            "daemonset" | "ds" => Some(ResourceType::DaemonSet),
            _ => None,
        }
    }
}

/// Parses a resource query in the form "type/name"
pub fn parse_resource_query(query: &str) -> Result<(ResourceType, String)> {
    let parts: Vec<&str> = query.split('/').collect();
    
    if parts.len() != 2 {
        return Err(anyhow!("Invalid resource query format: {}. Expected 'type/name'", query));
    }
    
    let resource_type = ResourceType::from_str(parts[0])
        .ok_or_else(|| anyhow!("Unsupported resource type: {}", parts[0]))?;
        
    Ok((resource_type, parts[1].to_string()))
}

/// Gets pod selectors from a Kubernetes resource
pub async fn get_pod_selectors_for_resource(
    client: &Client,
    namespace: &str,
    resource_type: &ResourceType,
    resource_name: &str,
) -> Result<Vec<(String, String)>> {
    match resource_type {
        ResourceType::Pod => {
            // Direct pod selection, return pod name as selector
            Ok(vec![("".to_string(), resource_name.to_string())])
        },
        ResourceType::Deployment => {
            get_deployment_pod_selectors(client, namespace, resource_name).await
        },
        ResourceType::ReplicaSet => {
            get_replicaset_pod_selectors(client, namespace, resource_name).await
        },
        ResourceType::StatefulSet => {
            get_statefulset_pod_selectors(client, namespace, resource_name).await
        },
        ResourceType::Job => {
            get_job_pod_selectors(client, namespace, resource_name).await
        },
        ResourceType::DaemonSet => {
            get_daemonset_pod_selectors(client, namespace, resource_name).await
        },
    }
}

/// Gets pod selectors from a Deployment
async fn get_deployment_pod_selectors(
    client: &Client,
    namespace: &str,
    name: &str,
) -> Result<Vec<(String, String)>> {
    let api: Api<Deployment> = Api::namespaced(client.clone(), namespace);
    
    // Get the deployment
    let deployment = api.get(name).await?;
    
    // Extract pod selector labels
    if let Some(spec) = &deployment.spec {
        if let Some(match_labels) = &spec.selector.match_labels {
            debug!("Found {} label selectors for deployment {}", match_labels.len(), name);
            return Ok(match_labels.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
        }
    }
    
    Err(anyhow!("No selector labels found for deployment {}", name))
}

/// Gets pod selectors from a ReplicaSet
async fn get_replicaset_pod_selectors(
    client: &Client,
    namespace: &str,
    name: &str,
) -> Result<Vec<(String, String)>> {
    let api: Api<ReplicaSet> = Api::namespaced(client.clone(), namespace);
    
    // Get the replicaset
    let rs = api.get(name).await?;
    
    // Extract pod selector labels
    if let Some(spec) = &rs.spec {
        if let Some(match_labels) = &spec.selector.match_labels {
            debug!("Found {} label selectors for replicaset {}", match_labels.len(), name);
            return Ok(match_labels.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
        }
    }
    
    Err(anyhow!("No selector labels found for replicaset {}", name))
}

/// Gets pod selectors from a StatefulSet
async fn get_statefulset_pod_selectors(
    client: &Client,
    namespace: &str,
    name: &str,
) -> Result<Vec<(String, String)>> {
    let api: Api<StatefulSet> = Api::namespaced(client.clone(), namespace);
    
    // Get the statefulset
    let sts = api.get(name).await?;
    
    // Extract pod selector labels
    if let Some(spec) = &sts.spec {
        if let Some(match_labels) = &spec.selector.match_labels {
            debug!("Found {} label selectors for statefulset {}", match_labels.len(), name);
            return Ok(match_labels.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
        }
    }
    
    Err(anyhow!("No selector labels found for statefulset {}", name))
}

/// Gets pod selectors from a Job
async fn get_job_pod_selectors(
    client: &Client,
    namespace: &str,
    name: &str,
) -> Result<Vec<(String, String)>> {
    let api: Api<Job> = Api::namespaced(client.clone(), namespace);
    
    // Get the job
    let job = api.get(name).await?;
    
    // Extract pod selector labels
    if let Some(spec) = &job.spec {
        if let Some(selector) = &spec.selector {
            if let Some(match_labels) = &selector.match_labels {
                debug!("Found {} label selectors for job {}", match_labels.len(), name);
                return Ok(match_labels.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
            }
        }
    }
    
    Err(anyhow!("No selector labels found for job {}", name))
}

/// Gets pod selectors from a DaemonSet
async fn get_daemonset_pod_selectors(
    client: &Client,
    namespace: &str,
    name: &str,
) -> Result<Vec<(String, String)>> {
    let api: Api<DaemonSet> = Api::namespaced(client.clone(), namespace);
    
    // Get the daemonset
    let ds = api.get(name).await?;
    
    // Extract pod selector labels
    if let Some(spec) = &ds.spec {
        if let Some(match_labels) = &spec.selector.match_labels {
            debug!("Found {} label selectors for daemonset {}", match_labels.len(), name);
            return Ok(match_labels.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
        }
    }
    
    Err(anyhow!("No selector labels found for daemonset {}", name))
}

/// Public struct for resource selection, as expected by tests
#[derive(Debug, PartialEq, Eq)]
pub struct ResourceSelector {
    pub resource_type: ResourceType,
    pub name: String,
}

impl ResourceSelector {
    pub fn parse(query: &str) -> Result<Self> {
        // Split into type and name parts
        let parts: Vec<&str> = query.split('/').collect();
        if parts.len() != 2 {
            return Err(anyhow!("Invalid resource selector format. Expected <type>/<name>"));
        }

        let (resource_type_str, name) = (parts[0], parts[1]);
        
        // Validate that name is not empty
        if name.is_empty() {
            return Err(anyhow!("Resource name cannot be empty"));
        }

        // Parse resource type
        let resource_type = ResourceType::from_str(resource_type_str)
            .ok_or_else(|| anyhow!("Unknown resource type: {}", resource_type_str))?;

        Ok(Self {
            resource_type,
            name: name.to_string(),
        })
    }
}