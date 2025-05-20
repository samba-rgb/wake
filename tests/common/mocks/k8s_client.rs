use k8s_openapi::api::core::v1::{Pod, PodSpec, PodStatus, Container};
use kube::{Client, Config};
use http::Uri;
use std::pin::Pin;
use anyhow::Result;
use futures::Stream;
use kube::api::ListParams;
use async_trait::async_trait;

/// Creates a mock pod that can be used in tests
pub fn mock_pod(name: &str, namespace: &str, containers: Vec<&str>) -> Pod {
    let mut pod = Pod::default();
    pod.metadata.name = Some(name.to_string());
    pod.metadata.namespace = Some(namespace.to_string());
    
    // Create containers
    let containers = containers.into_iter()
        .map(|container_name| {
            let mut container = Container::default();
            container.name = container_name.to_string();
            container
        })
        .collect();
    
    pod.spec = Some(PodSpec {
        containers,
        ..PodSpec::default()
    });
    
    pod.status = Some(PodStatus {
        phase: Some("Running".to_string()),
        ..PodStatus::default()
    });
    
    pod
}

/// Creates a collection of mock pods for testing
pub fn create_mock_pods() -> Vec<Pod> {
    vec![
        mock_pod("nginx-1", "default", vec!["nginx", "sidecar"]),
        mock_pod("nginx-2", "default", vec!["nginx"]),
        mock_pod("app-1", "app-ns", vec!["app", "db", "cache"]),
    ]
}

// Mock Pod API trait
#[async_trait]
pub trait PodApiTrait {
    async fn list(&self, params: &ListParams) -> Result<Vec<Pod>>;
}

// Mock Pod API implementation
#[derive(Default)]
pub struct MockPodApi;

#[async_trait]
impl PodApiTrait for MockPodApi {
    async fn list(&self, _params: &ListParams) -> Result<Vec<Pod>> {
        Ok(create_mock_pods())
    }
}