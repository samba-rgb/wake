use crate::cli::args::Args;
use regex::Regex;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_regex_patterns() {
        let args = Args {
            pod_selector: "nginx-.*".to_string(),
            container: "web|api".to_string(),
            ..Default::default()
        };

        assert!(args.pod_regex().is_ok());
        assert!(args.container_regex().is_ok());
    }

    #[test]
    fn test_invalid_regex_patterns() {
        let args = Args {
            pod_selector: "[invalid-regex".to_string(),
            ..Default::default()
        };

        assert!(args.pod_regex().is_err());
    }

    #[test]
    fn test_namespace_validation() {
        // Valid namespace names
        assert!(is_valid_namespace("default"));
        assert!(is_valid_namespace("kube-system"));
        assert!(is_valid_namespace("my-app-123"));
        
        // Invalid namespace names
        assert!(!is_valid_namespace(""));
        assert!(!is_valid_namespace("UPPERCASE"));
        assert!(!is_valid_namespace("spaces not allowed"));
        assert!(!is_valid_namespace("-starts-with-dash"));
    }

    #[test]
    fn test_buffer_size_validation() {
        assert!(is_valid_buffer_size(100));
        assert!(is_valid_buffer_size(10000));
        assert!(!is_valid_buffer_size(0));
        assert!(!is_valid_buffer_size(1000001)); // Too large
    }
}

fn is_valid_namespace(name: &str) -> bool {
    if name.is_empty() || name.len() > 63 {
        return false;
    }
    
    let re = Regex::new(r"^[a-z0-9]([-a-z0-9]*[a-z0-9])?$").unwrap();
    re.is_match(name)
}

fn is_valid_buffer_size(size: usize) -> bool {
    size > 0 && size <= 1_000_000
}