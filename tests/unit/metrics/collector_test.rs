#[cfg(test)]
mod metrics_collector_tests {
    use chrono::{Duration, Utc};
    use std::time::Duration as StdDuration;
    use std::sync::{Arc, Mutex};
    use tokio_util::sync::CancellationToken;
    
    use crate::k8s::metrics::{MetricsClient, ResourceMetrics, PodMetrics};
    use crate::k8s::pod::Pod;
    use crate::metrics::collector::{MetricsCollector, PodMetricsTimeSeries, MetricsSummary};
    
    // Mock MetricsClient implementation for testing
    struct MockMetricsClient {
        pub metrics_available: bool,
        pub pod_metrics: std::collections::HashMap<String, PodMetrics>,
    }
    
    impl MockMetricsClient {
        fn new(metrics_available: bool) -> Self {
            Self {
                metrics_available,
                pod_metrics: std::collections::HashMap::new(),
            }
        }
        
        fn add_pod_metrics(&mut self, pod_name: &str, cpu_usage: f64, memory_usage: f64) {
            let pod_metrics = PodMetrics {
                timestamp: Utc::now(),
                window: Some(StdDuration::from_secs(60)),
                cpu: ResourceMetrics {
                    usage: format!("{}m", (cpu_usage * 1000.0) as u64),
                    usage_value: cpu_usage,
                    request: Some("1000m".to_string()),
                    limit: Some("2000m".to_string()),
                    utilization: cpu_usage * 100.0, // As percentage of the request
                },
                memory: ResourceMetrics {
                    usage: format!("{}Ki", (memory_usage / 1024.0) as u64),
                    usage_value: memory_usage,
                    request: Some("256Mi".to_string()),
                    limit: Some("512Mi".to_string()),
                    utilization: memory_usage * 100.0 / (256.0 * 1024.0 * 1024.0), // As percentage of the request
                },
            };
            
            self.pod_metrics.insert(pod_name.to_string(), pod_metrics);
        }
        
        async fn check_metrics_api_available(&self) -> bool {
            self.metrics_available
        }
        
        async fn get_pod_metrics(
            &self,
            _pod_selector: &str,
            _pods: &[Pod],
        ) -> anyhow::Result<std::collections::HashMap<String, PodMetrics>> {
            if !self.metrics_available {
                return Err(anyhow::anyhow!("Metrics API is not available"));
            }
            
            Ok(self.pod_metrics.clone())
        }
    }
    
    #[tokio::test]
    async fn test_metrics_collector_creation() {
        let mock_client = MockMetricsClient::new(true);
        let refresh_interval = StdDuration::from_secs(5);
        
        let collector = MetricsCollector::new(
            mock_client,
            refresh_interval,
        );
        
        // The collector should be initialized with empty pod metrics
        assert_eq!(collector.get_all_pod_names().len(), 0);
        
        // The metrics summary should reflect no pods being monitored
        let summary = collector.get_metrics_summary();
        assert_eq!(summary.total_pods, 0);
        assert_eq!(summary.monitored_pods, 0);
        assert_eq!(summary.total_containers, 0);
        assert_eq!(summary.cpu_usage_total, 0.0);
        assert_eq!(summary.memory_usage_total, 0.0);
    }
    
    #[tokio::test]
    async fn test_set_pods() {
        let mock_client = MockMetricsClient::new(true);
        let refresh_interval = StdDuration::from_secs(5);
        
        let collector = MetricsCollector::new(
            mock_client,
            refresh_interval,
        );
        
        // Create some test pods
        let pods = vec![
            Pod {
                name: "pod1".to_string(),
                namespace: "test".to_string(),
                status: "Running".to_string(),
                containers: vec![
                    crate::k8s::pod::Container {
                        name: "container1".to_string(),
                        status: "Running".to_string(),
                        resources: crate::k8s::resource::Resources {
                            requests: None,
                            limits: None,
                        },
                    }
                ],
                node: None,
            },
            Pod {
                name: "pod2".to_string(),
                namespace: "test".to_string(),
                status: "Running".to_string(),
                containers: vec![
                    crate::k8s::pod::Container {
                        name: "container1".to_string(),
                        status: "Running".to_string(),
                        resources: crate::k8s::resource::Resources {
                            requests: None,
                            limits: None,
                        },
                    },
                    crate::k8s::pod::Container {
                        name: "container2".to_string(),
                        status: "Running".to_string(),
                        resources: crate::k8s::resource::Resources {
                            requests: None,
                            limits: None,
                        },
                    }
                ],
                node: None,
            }
        ];
        
        // Set the pods
        collector.set_pods(pods);
        
        // Check the metrics summary reflects the new pods
        let summary = collector.get_metrics_summary();
        assert_eq!(summary.total_pods, 2);
        assert_eq!(summary.total_containers, 3);
    }
    
    // More tests can be added here for the collection functionality
    // but they would require more sophisticated mocking of the
    // background collection task
}