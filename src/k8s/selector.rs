use anyhow::{Result, Context};
use kube::{Api, Client};
use kube::api::ListParams;
use k8s_openapi::api::core::v1::Pod;
use crate::k8s::resource::{ResourceType, parse_resource_query, get_pod_selectors_for_resource};
use tracing::{debug, info};

/// Creates a selector for Kubernetes API based on resource type
pub async fn create_selector_for_resource(
    client: &Client,
    namespace: &str,
    resource_query: &str,
) -> Result<ListParams> {
    if resource_query.contains('/') {
        let (resource_type, resource_name) = parse_resource_query(resource_query)?;
        
        // Get selectors for the specified resource
        let selectors = get_pod_selectors_for_resource(
            client, 
            namespace, 
            &resource_type, 
            &resource_name,
        ).await?;
        
        // Check if this is a field selector or label selector
        if selectors.len() == 1 && selectors[0].0 == "metadata.name" {
            // This is a field selector for a Pod by name
            debug!("Created field selector: metadata.name={}", selectors[0].1);
            
            Ok(ListParams {
                field_selector: Some(format!("metadata.name={}", selectors[0].1)),
                ..Default::default()
            })
        } else {
            // Build a label selector string
            let selector_string = selectors
                .iter()
                .map(|(key, value)| format!("{}={}", key, value))
                .collect::<Vec<_>>()
                .join(",");
                
            debug!("Created label selector: {}", selector_string);
            
            Ok(ListParams {
                label_selector: Some(selector_string),
                ..Default::default()
            })
        }
    } else {
        // If no resource type is specified, use the default (all pods)
        Ok(ListParams::default())
    }
}

/// Checks if a pod matches the given field selector criteria
pub fn match_field_selector(pod: &Pod, field_selector: Option<&str>) -> bool {
    if let Some(selector) = field_selector {
        let parts: Vec<&str> = selector.split('=').collect();
        if parts.len() == 2 {
            let (field, value) = (parts[0], parts[1]);
            
            match field {
                "metadata.name" => {
                    pod.metadata.name.as_ref().map_or(false, |name| name == value)
                },
                "spec.nodeName" => {
                    if let Some(spec) = &pod.spec {
                        spec.node_name.as_ref().map_or(false, |node| node == value)
                    } else {
                        false
                    }
                },
                "status.phase" => {
                    if let Some(status) = &pod.status {
                        status.phase.as_ref().map_or(false, |phase| phase == value)
                    } else {
                        false
                    }
                },
                _ => {
                    debug!("Unsupported field selector: {}", field);
                    true // Default to true for unsupported fields
                }
            }
        } else {
            debug!("Invalid field selector format: {}", selector);
            true
        }
    } else {
        true // No field selector means match all
    }
}