use anyhow::{Result, Context};
use colored::*;
use k8s_openapi::api::core::v1::Pod;
use kube::{Api, Client};
use kube::api::{ListParams, ResourceExt};
use regex::Regex;
use tracing::{info, debug};
use crate::k8s::selector::create_selector_for_resource;
use comfy_table::{Table, presets::UTF8_FULL, ContentArrangement, Cell, CellAlignment};
use chrono::{Utc, DateTime};

/// Represents pod information relevant to log watching
#[derive(Debug, Clone)]
pub struct PodInfo {
    pub namespace: String,
    pub name: String,
    pub containers: Vec<String>,
}

/// Filters and selects pods based on regex patterns or resource type
pub async fn select_pods(
    client: &Client, 
    namespace: &str, 
    pod_re: &Regex, 
    container_re: &Regex,
    all_namespaces: bool,
    resource_query: Option<&str>,
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
        
        // Determine how to list pods based on whether we have a resource query
        let params = if let Some(query) = resource_query {
            info!("Using resource selector: {}", query);
            create_selector_for_resource(client, &ns, query).await?
        } else {
            // If no resource selector, use regex-based selection
            ListParams::default()
        };
        
        let pod_list = pods.list(&params).await.context(format!("Failed to list pods in namespace {}", ns))?;
        
        for pod in pod_list {
            let pod_name = pod.name_unchecked();
            
            // If using a resource selector, we skip the pod regex check since the K8s API
            // has already filtered the pods. Otherwise, apply the regex filter.
            if resource_query.is_none() && !pod_re.is_match(&pod_name) {
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
                        if is_plain_name(container_re) {
                            if &cs.name != container_re.as_str() {
                                continue;
                            }
                        } else {
                            if !container_re.is_match(&cs.name) {
                                continue;
                            }
                        }
                        containers.push(cs.name.clone());
                    }
                }
                // Also check init containers
                if let Some(init_containers) = &status.init_container_statuses {
                    for cs in init_containers {
                        if is_plain_name(container_re) {
                            if &cs.name != container_re.as_str() {
                                continue;
                            }
                        } else {
                            if !container_re.is_match(&cs.name) {
                                continue;
                            }
                        }
                        containers.push(cs.name.clone());
                    }
                }
            }
            // If no containers found via status, try spec
            if containers.is_empty() {
                if let Some(spec) = &pod.spec {
                    for c in &spec.containers {
                        if is_plain_name(container_re) {
                            if &c.name != container_re.as_str() {
                                continue;
                            }
                        } else {
                            if !container_re.is_match(&c.name) {
                                continue;
                            }
                        }
                        containers.push(c.name.clone());
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

/// List container names for all pods matching the given regex or resource query
pub async fn list_container_names(
    client: &Client,
    namespace: &str,
    pod_regex: &Regex,
    all_namespaces: bool,
    resource_query: Option<&str>,
    container_regex: Option<&Regex>, // NEW: allow passing container regex
) -> Result<()> {
    let pods_api: Api<Pod>;
    if all_namespaces {
        pods_api = Api::all(client.clone());
    } else {
        pods_api = Api::namespaced(client.clone(), namespace);
    }
    let params = if let Some(query) = resource_query {
        println!("Using resource selector: {}", query);
        let ns = if all_namespaces { "default" } else { namespace };
        create_selector_for_resource(client, ns, query).await?
    } else {
        ListParams::default()
    };
    let pods = pods_api.list(&params).await?;
    // Collect all rows for table
    let mut rows = Vec::new();
    for pod in pods.items {
        let pod_name = pod.metadata.name.as_deref().unwrap_or("");
        if resource_query.is_none() && !pod_regex.is_match(pod_name) {
            continue;
        }
        let pod_namespace = pod.metadata.namespace.as_deref().unwrap_or("default");
        let pod_ip = pod.status.as_ref().and_then(|s| s.pod_ip.as_deref()).unwrap_or("");
        // Container statuses for info
        let mut status_map = std::collections::HashMap::new();
        if let Some(status) = &pod.status {
            if let Some(container_statuses) = &status.container_statuses {
                for cs in container_statuses {
                    status_map.insert(cs.name.clone(), cs);
                }
            }
            if let Some(init_statuses) = &status.init_container_statuses {
                for cs in init_statuses {
                    status_map.insert(cs.name.clone(), cs);
                }
            }
            if let Some(ephemeral_statuses) = &status.ephemeral_container_statuses {
                for cs in ephemeral_statuses {
                    status_map.insert(cs.name.clone(), cs);
                }
            }
        }
        if let Some(spec) = &pod.spec {
            let mut add_row = |container: &k8s_openapi::api::core::v1::Container, typ: &str| {
                if let Some(re) = container_regex {
                    if !re.is_match(&container.name) {
                        return;
                    }
                }
                let status = status_map.get(&container.name);
                let (running_for, restarts, state) = if let Some(cs) = status {
                    let running_from_opt = cs.state.as_ref()
                        .and_then(|state| state.running.as_ref().and_then(|r| r.started_at.as_ref()))
                        .map(|dt| dt.0);
                    let running_for = if let Some(started_at) = running_from_opt {
                        let now = Utc::now();
                        let duration = now.signed_duration_since(started_at);
                        if duration.num_seconds() > 0 {
                            // Format as human readable
                            let secs = duration.num_seconds();
                            let (days, rem) = (secs / 86400, secs % 86400);
                            let (hours, rem) = (rem / 3600, rem % 3600);
                            let (mins, secs) = (rem / 60, rem % 60);
                            if days > 0 {
                                format!("{}d {}h {}m", days, hours, mins)
                            } else if hours > 0 {
                                format!("{}h {}m", hours, mins)
                            } else if mins > 0 {
                                format!("{}m {}s", mins, secs)
                            } else {
                                format!("{}s", secs)
                            }
                        } else {
                            "0s".to_string()
                        }
                    } else {
                        "-".to_string()
                    };
                    let restarts = cs.restart_count;
                    let state = if let Some(state) = &cs.state {
                        if state.running.is_some() {
                            if cs.ready { "Running+Ready" } else { "Running (Not Ready)" }
                        } else if state.terminated.is_some() {
                            "Terminated"
                        } else if state.waiting.is_some() {
                            "Waiting"
                        } else {
                            "Unknown"
                        }
                    } else {
                        "Unknown"
                    };
                    (running_for, restarts, state.to_string())
                } else {
                    ("-".to_string(), 0, "Unknown".to_string())
                };
                rows.push((pod_namespace.to_string(), pod_name.to_string(), container.name.clone(), typ.to_string(), pod_ip.to_string(), running_for, restarts, state));
            };
            for container in &spec.containers {
                add_row(container, "normal");
            }
            if let Some(init_containers) = &spec.init_containers {
                for container in init_containers {
                    add_row(container, "init");
                }
            }
            if let Some(ephemeral_containers) = &spec.ephemeral_containers {
                for container in ephemeral_containers {
                    // EphemeralContainer is a different type, so handle manually
                    let name = &container.name;
                    if let Some(re) = container_regex {
                        if !re.is_match(name) {
                            continue;
                        }
                    }
                    // Ephemeral containers do not have status info in ContainerStatus, so use defaults
                    rows.push((pod_namespace.to_string(), pod_name.to_string(), name.clone(), "ephemeral".to_string(), pod_ip.to_string(), "-".to_string(), 0, "Unknown".to_string()));
                }
            }
        }
    }
    if rows.is_empty() {
        let container_filter = if let Some(re) = container_regex {
            re.as_str()
        } else {
            "(all containers)"
        };
        println!(
            "No containers found matching pod pattern \"{}\" and container filter \"{}\" in namespace \"{}\"",
            pod_regex,
            container_filter,
            if all_namespaces { "all namespaces" } else { namespace }
        );
        return Ok(());
    }
    // Use comfy-table for output
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec!["Namespace", "Pod", "Container", "Type", "Pod IP", "Running For", "Restarts", "State"]);
    for (ns, pod, cont, typ, ip, start, restarts, state) in rows {
        let padded_state = format!("{:<18}", state); // pad all states to 18 chars
        let colored_state = if state == "Running+Ready" {
            padded_state.green().bold().to_string()
        } else {
            padded_state.red().bold().to_string()
        };
        let state_cell = Cell::new(colored_state).set_alignment(CellAlignment::Left);
        table.add_row(vec![Cell::new(ns), Cell::new(pod), Cell::new(cont), Cell::new(typ), Cell::new(ip), Cell::new(start), Cell::new(restarts.to_string()), state_cell]);
    }
    println!("{}", table);
    Ok(())
}

// Helper function to check if a regex is a plain name (no regex metacharacters)
fn is_plain_name(re: &Regex) -> bool {
    let s = re.as_str();
    // If it contains only alphanumeric, dash, or underscore, treat as plain name
    // and does not start/end with ^/$ or contain . * + ? | ( ) [ ] { } \
    !s.contains(|c: char| "^$.|?*+()[]{}\\".contains(c))
}