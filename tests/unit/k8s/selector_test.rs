// use wake::k8s::selector::{LabelSelector, FieldSelector};
use std::collections::HashMap;
use anyhow::Result;

// The following tests are commented out because LabelSelector and FieldSelector are not implemented in the codebase.
// Uncomment and implement them if/when these features are added.

// #[test]
// fn test_label_selector_equality() -> Result<()> {
//     // Test label selector with equality matching
//     let mut selector = LabelSelector::new();
//     selector.add_equality("app", "nginx");
//     selector.add_equality("environment", "production");
    
//     // Create test labels that should match
//     let mut matching_labels = HashMap::new();
//     matching_labels.insert("app".to_string(), "nginx".to_string());
//     matching_labels.insert("environment".to_string(), "production".to_string());
//     matching_labels.insert("version".to_string(), "1.0".to_string()); // Extra label, should still match
    
//     // Create test labels that should not match
//     let mut non_matching_labels1 = HashMap::new();
//     non_matching_labels1.insert("app".to_string(), "apache".to_string()); // Wrong value
//     non_matching_labels1.insert("environment".to_string(), "production".to_string());
    
//     let mut non_matching_labels2 = HashMap::new();
//     non_matching_labels2.insert("app".to_string(), "nginx".to_string());
//     // Missing environment label
    
//     // Test matching
//     assert!(selector.matches(&matching_labels));
//     assert!(!selector.matches(&non_matching_labels1));
//     assert!(!selector.matches(&non_matching_labels2));
    
//     Ok(())
// }

// #[test]
// fn test_label_selector_set_based() -> Result<()> {
//     // Test label selector with set-based operations
//     let mut selector = LabelSelector::new();
    
//     // app should be one of: nginx, apache, haproxy
//     selector.add_in("app", vec!["nginx", "apache", "haproxy"]);
    
//     // environment should not be: development, testing
//     selector.add_not_in("environment", vec!["development", "testing"]);
    
//     // tier should exist
//     selector.add_exists("tier");
    
//     // canary should not exist
//     selector.add_not_exists("canary");
    
//     // Test labels that should match
//     let mut matching_labels1 = HashMap::new();
//     matching_labels1.insert("app".to_string(), "nginx".to_string());
//     matching_labels1.insert("environment".to_string(), "production".to_string());
//     matching_labels1.insert("tier".to_string(), "frontend".to_string());
    
//     // Test labels that shouldn't match - wrong app
//     let mut non_matching_labels1 = HashMap::new();
//     non_matching_labels1.insert("app".to_string(), "wordpress".to_string());
//     non_matching_labels1.insert("environment".to_string(), "production".to_string());
//     non_matching_labels1.insert("tier".to_string(), "frontend".to_string());
    
//     // Test labels that shouldn't match - excluded environment
//     let mut non_matching_labels2 = HashMap::new();
//     non_matching_labels2.insert("app".to_string(), "nginx".to_string());
//     non_matching_labels2.insert("environment".to_string(), "testing".to_string());
//     non_matching_labels2.insert("tier".to_string(), "frontend".to_string());
    
//     // Test labels that shouldn't match - missing required label
//     let mut non_matching_labels3 = HashMap::new();
//     non_matching_labels3.insert("app".to_string(), "nginx".to_string());
//     non_matching_labels3.insert("environment".to_string(), "production".to_string());
//     // Missing tier
    
//     // Test labels that shouldn't match - has excluded label
//     let mut non_matching_labels4 = HashMap::new();
//     non_matching_labels4.insert("app".to_string(), "nginx".to_string());
//     non_matching_labels4.insert("environment".to_string(), "production".to_string());
//     non_matching_labels4.insert("tier".to_string(), "frontend".to_string());
//     non_matching_labels4.insert("canary".to_string(), "true".to_string());
    
//     // Verify matches
//     assert!(selector.matches(&matching_labels1));
//     assert!(!selector.matches(&non_matching_labels1));
//     assert!(!selector.matches(&non_matching_labels2));
//     assert!(!selector.matches(&non_matching_labels3));
//     assert!(!selector.matches(&non_matching_labels4));
    
//     Ok(())
// }

// #[test]
// fn test_field_selector() -> Result<()> {
//     let mut selector = FieldSelector::new();
//     selector.add_equality("status.phase", "Running");
//     selector.add_equality("spec.nodeName", "node-1");
    
//     // Simple struct to mock a pod's fields
//     struct PodFields {
//         status_phase: String,
//         spec_node_name: String,
//     }
    
//     // Field accessor function - in real code this would use reflection or a similar approach
//     let field_accessor = |pod: &PodFields, field: &str| -> Option<String> {
//         match field {
//             "status.phase" => Some(pod.status_phase.clone()),
//             "spec.nodeName" => Some(pod.spec_node_name.clone()),
//             _ => None,
//         }
//     };
    
//     // Test matching pod
//     let matching_pod = PodFields {
//         status_phase: "Running".to_string(),
//         spec_node_name: "node-1".to_string(),
//     };
    
//     // Test non-matching pod
//     let non_matching_pod1 = PodFields {
//         status_phase: "Pending".to_string(), // Different phase
//         spec_node_name: "node-1".to_string(),
//     };
    
//     let non_matching_pod2 = PodFields {
//         status_phase: "Running".to_string(),
//         spec_node_name: "node-2".to_string(), // Different node
//     };
    
//     // Test matching
//     assert!(selector.matches(&matching_pod, &field_accessor));
//     assert!(!selector.matches(&non_matching_pod1, &field_accessor));
//     assert!(!selector.matches(&non_matching_pod2, &field_accessor));
    
//     Ok(())
// }

// #[test]
// fn test_selector_parsing() -> Result<()> {
//     // Test parsing label selectors from strings
//     let selector1 = LabelSelector::parse("app=nginx,environment=production")?;
    
//     let mut expected_equalities = HashMap::new();
//     expected_equalities.insert("app".to_string(), "nginx".to_string());
//     expected_equalities.insert("environment".to_string(), "production".to_string());
    
//     assert_eq!(selector1.equalities(), &expected_equalities);
    
//     // Test more complex expressions
//     let selector2 = LabelSelector::parse("app in (nginx,apache),environment notin (dev,test),tier,!canary")?;
    
//     // Verify the components of the complex selector
//     assert!(selector2.in_values().contains_key("app"));
//     assert_eq!(selector2.in_values().get("app").unwrap().len(), 2);
//     assert!(selector2.in_values().get("app").unwrap().contains(&"nginx".to_string()));
    
//     assert!(selector2.not_in_values().contains_key("environment"));
//     assert!(selector2.exists_keys().contains("tier"));
//     assert!(selector2.not_exists_keys().contains("canary"));
    
//     Ok(())
// }