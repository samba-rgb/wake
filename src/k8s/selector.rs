use anyhow::Result;
use kube::Client;
use kube::api::ListParams;
use crate::k8s::resource::{parse_resource_query, get_pod_selectors_for_resource};
use tracing::debug;

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
                .map(|(key, value)| format!("{key}={value}"))
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