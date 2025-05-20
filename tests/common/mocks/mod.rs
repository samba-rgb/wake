mod k8s_client;

pub use k8s_client::{
    mock_pod,
    create_mock_pods,
    MockPodApi,
    PodApiTrait,
};