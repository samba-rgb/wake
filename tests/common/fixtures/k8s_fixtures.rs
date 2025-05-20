use k8s_openapi::api::core::v1::{Pod, PodSpec, PodStatus, Container};
use wake::k8s::logs::LogEntry;
use chrono::{DateTime, TimeZone, Utc};
use std::collections::BTreeMap; // Changed from HashMap to BTreeMap

/// Creates a fixed set of test pods
pub fn create_test_pods() -> Vec<Pod> {
    vec![
        create_test_pod(
            "frontend-1", 
            "default", 
            vec!["nginx", "envoy"], 
            Some("Running"),
            Some(BTreeMap::from([
                ("app".to_string(), "frontend".to_string()),
                ("tier".to_string(), "web".to_string()),
            ])),
        ),
        create_test_pod(
            "frontend-2", 
            "default", 
            vec!["nginx"], 
            Some("Running"),
            Some(BTreeMap::from([
                ("app".to_string(), "frontend".to_string()),
                ("tier".to_string(), "web".to_string()),
            ])),
        ),
        create_test_pod(
            "backend-1", 
            "default", 
            vec!["api", "cache"], 
            Some("Running"),
            Some(BTreeMap::from([
                ("app".to_string(), "backend".to_string()),
                ("tier".to_string(), "api".to_string()),
            ])),
        ),
        create_test_pod(
            "db-1", 
            "db-ns", 
            vec!["postgres", "backup"], 
            Some("Running"),
            Some(BTreeMap::from([
                ("app".to_string(), "database".to_string()),
                ("tier".to_string(), "data".to_string()),
            ])),
        ),
        create_test_pod(
            "monitoring-1", 
            "monitoring", 
            vec!["prometheus"], 
            Some("Running"),
            Some(BTreeMap::from([
                ("app".to_string(), "monitoring".to_string()),
                ("tier".to_string(), "observability".to_string()),
            ])),
        ),
        create_test_pod(
            "failing-pod", 
            "default", 
            vec!["app"], 
            Some("Error"),
            Some(BTreeMap::from([
                ("app".to_string(), "unstable".to_string()),
                ("tier".to_string(), "test".to_string()),
            ])),
        ),
        create_test_pod(
            "pending-pod", 
            "default", 
            vec!["init"], 
            Some("Pending"),
            Some(BTreeMap::from([
                ("app".to_string(), "startup".to_string()),
                ("tier".to_string(), "test".to_string()),
            ])),
        ),
    ]
}

/// Helper function to create a test Pod
pub fn create_test_pod(name: &str, namespace: &str, container_names: Vec<&str>, 
                       phase: Option<&str>, labels: Option<BTreeMap<String, String>>) -> Pod {
    let mut pod = Pod::default();
    
    // Set metadata
    pod.metadata.name = Some(name.to_string());
    pod.metadata.namespace = Some(namespace.to_string());
    
    // Set labels if provided
    if let Some(label_map) = labels {
        pod.metadata.labels = Some(label_map);
    }
    
    // Create containers
    let containers = container_names.into_iter()
        .map(|container_name| {
            let mut container = Container::default();
            container.name = container_name.to_string();
            container
        })
        .collect();
    
    // Set pod spec
    pod.spec = Some(PodSpec {
        containers,
        ..PodSpec::default()
    });
    
    // Set pod status
    pod.status = Some(PodStatus {
        phase: phase.map(ToString::to_string),
        ..PodStatus::default()
    });
    
    pod
}

/// Creates a fixed set of test log entries
pub fn create_test_log_entries() -> Vec<LogEntry> {
    vec![
        LogEntry {
            namespace: "default".to_string(),
            pod_name: "frontend-1".to_string(),
            container_name: "nginx".to_string(),
            message: "INFO: Server started on port 8080".to_string(),
            timestamp: Some(Utc.with_ymd_and_hms(2023, 5, 15, 10, 0, 0).unwrap()),
        },
        LogEntry {
            namespace: "default".to_string(),
            pod_name: "frontend-1".to_string(),
            container_name: "nginx".to_string(),
            message: "INFO: Received request GET /api/users".to_string(),
            timestamp: Some(Utc.with_ymd_and_hms(2023, 5, 15, 10, 1, 0).unwrap()),
        },
        LogEntry {
            namespace: "default".to_string(),
            pod_name: "frontend-1".to_string(),
            container_name: "envoy".to_string(),
            message: "DEBUG: Routing request to backend-1:8080".to_string(),
            timestamp: Some(Utc.with_ymd_and_hms(2023, 5, 15, 10, 1, 1).unwrap()),
        },
        LogEntry {
            namespace: "default".to_string(),
            pod_name: "backend-1".to_string(),
            container_name: "api".to_string(),
            message: "INFO: Processing request for /api/users".to_string(),
            timestamp: Some(Utc.with_ymd_and_hms(2023, 5, 15, 10, 1, 2).unwrap()),
        },
        LogEntry {
            namespace: "default".to_string(),
            pod_name: "backend-1".to_string(),
            container_name: "api".to_string(),
            message: "DEBUG: Query execution time: 45ms".to_string(),
            timestamp: Some(Utc.with_ymd_and_hms(2023, 5, 15, 10, 1, 3).unwrap()),
        },
        LogEntry {
            namespace: "default".to_string(),
            pod_name: "backend-1".to_string(),
            container_name: "cache".to_string(),
            message: "INFO: Cache hit ratio: 0.85".to_string(),
            timestamp: Some(Utc.with_ymd_and_hms(2023, 5, 15, 10, 1, 4).unwrap()),
        },
        LogEntry {
            namespace: "db-ns".to_string(),
            pod_name: "db-1".to_string(),
            container_name: "postgres".to_string(),
            message: "INFO: Database connection established".to_string(),
            timestamp: Some(Utc.with_ymd_and_hms(2023, 5, 15, 10, 0, 30).unwrap()),
        },
        LogEntry {
            namespace: "db-ns".to_string(),
            pod_name: "db-1".to_string(),
            container_name: "postgres".to_string(),
            message: "WARN: High CPU usage detected".to_string(),
            timestamp: Some(Utc.with_ymd_and_hms(2023, 5, 15, 10, 2, 0).unwrap()),
        },
        LogEntry {
            namespace: "db-ns".to_string(),
            pod_name: "db-1".to_string(),
            container_name: "backup".to_string(),
            message: "INFO: Starting scheduled backup".to_string(),
            timestamp: Some(Utc.with_ymd_and_hms(2023, 5, 15, 10, 3, 0).unwrap()),
        },
        LogEntry {
            namespace: "monitoring".to_string(),
            pod_name: "monitoring-1".to_string(),
            container_name: "prometheus".to_string(),
            message: "INFO: Scraping metrics from targets".to_string(),
            timestamp: Some(Utc.with_ymd_and_hms(2023, 5, 15, 10, 0, 15).unwrap()),
        },
        LogEntry {
            namespace: "default".to_string(),
            pod_name: "failing-pod".to_string(),
            container_name: "app".to_string(),
            message: "ERROR: Failed to initialize application".to_string(),
            timestamp: Some(Utc.with_ymd_and_hms(2023, 5, 15, 10, 0, 5).unwrap()),
        },
        LogEntry {
            namespace: "default".to_string(),
            pod_name: "failing-pod".to_string(),
            container_name: "app".to_string(),
            message: "ERROR: Exiting with code 1".to_string(),
            timestamp: Some(Utc.with_ymd_and_hms(2023, 5, 15, 10, 0, 6).unwrap()),
        },
    ]
}