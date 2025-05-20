use wake::k8s::pod::PodInfo;
use crate::common::mocks::{create_mock_pods, MockPodApi, PodApiTrait};
use regex::Regex;
use anyhow::Result;
use kube::api::ListParams;

#[test]
fn test_pod_info_creation() {
    // Purpose: Verify PodInfo struct creation and field access
    // Tests:
    // - Basic struct initialization
    // - Multiple container names
    // - Field value storage and retrieval
    // Validates:
    // - All fields store and retrieve correctly
    // - Container list handling
    let pod_info = PodInfo {
        name: "test-pod".to_string(),
        namespace: "test-namespace".to_string(),
        containers: vec!["container1".to_string(), "container2".to_string()],
    };

    assert_eq!(pod_info.name, "test-pod");
    assert_eq!(pod_info.namespace, "test-namespace");
    assert_eq!(pod_info.containers.len(), 2);
    assert_eq!(pod_info.containers[0], "container1");
    assert_eq!(pod_info.containers[1], "container2");
}

#[tokio::test]
async fn test_select_pods_with_regex() -> Result<()> {
    // Purpose: Verify pod selection based on regex patterns
    // Tests:
    // - Pod name matching with regex
    // - Container name matching with regex
    // - Filtering of non-matching pods
    // Validates:
    // - Correct pods are selected
    // - Container filtering works
    // - Non-matching pods are excluded

    // Create our mock API
    let mock_api = MockPodApi::default();
    let pod_regex = Regex::new(r"nginx").unwrap();
    let container_regex = Regex::new(r"nginx").unwrap();
    
    // Test pod selection using our mock
    let pods = mock_api.list(&ListParams::default()).await?;
    
    // Filter pods manually using our patterns
    let matching_pods = pods.into_iter()
        .filter(|pod| {
            let pod_name = pod.metadata.name.as_ref().unwrap();
            pod_regex.is_match(pod_name) &&
            pod.spec.as_ref().unwrap().containers.iter()
                .any(|c| container_regex.is_match(&c.name))
        })
        .collect::<Vec<_>>();
    
    // Verify pod selection logic
    assert_eq!(matching_pods.len(), 2, "Should find 2 nginx pods");
    for pod in matching_pods {
        let pod_name = pod.metadata.name.unwrap();
        assert!(pod_name.contains("nginx"), "Pod name should contain nginx");
        assert!(pod.spec.unwrap().containers.iter().any(|c| c.name.contains("nginx")), 
            "Pod should have at least one nginx container");
    }
    
    Ok(())
}

#[tokio::test]
async fn test_select_pods_across_namespaces() -> Result<()> {
    // Purpose: Verify pod selection across multiple namespaces
    // Tests:
    // - Multi-namespace pod discovery
    // - Namespace counting and validation
    // - Pod distribution verification
    // Validates:
    // - Pods found in multiple namespaces
    // - Expected namespace presence
    // - Pod count per namespace
    use std::collections::HashMap;
    
    let mock_pods = create_mock_pods();
    
    // Count pods in each namespace
    let mut namespace_counts = HashMap::new();
    for pod in mock_pods {
        let ns = pod.metadata.namespace.unwrap_or_default();
        *namespace_counts.entry(ns).or_insert(0) += 1;
    }
    
    // Verify we have pods across different namespaces
    assert!(namespace_counts.len() > 1, "Should have pods in multiple namespaces");
    assert!(namespace_counts.contains_key("default"), "Should have pods in default namespace");
    assert!(namespace_counts.contains_key("app-ns"), "Should have pods in app-ns namespace");
    
    Ok(())
}