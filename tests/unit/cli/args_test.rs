use crate::cli::args::Args;

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use regex::Regex;
    use anyhow::Result;

    // Tests for command-line argument parsing and validation

    #[test]
    fn test_default_args() {
        // Purpose: Verify all argument fields have correct default values when no args provided
        // Validation:
        // - namespace defaults to "default"
        // - all_namespaces is false
        // - pod_selector defaults to ".*" (match all)
        // - container defaults to ".*" (match all)
        // - tail defaults to 10 lines
        // - follow defaults to true
        // - include/exclude patterns are None
        // - timestamps default to false
        // - output format defaults to "text"
        let args = Args::default();
        
        // Verify default values
        assert_eq!(args.namespace, "default");
        assert_eq!(args.all_namespaces, false);
        assert_eq!(args.pod_selector, ".*");
        assert_eq!(args.container, ".*");
        assert_eq!(args.tail, 10);
        assert_eq!(args.follow, true);
        assert!(args.include.is_none());
        assert!(args.exclude.is_none());
        assert_eq!(args.timestamps, false);
        assert_eq!(args.output, "text");
        assert!(args.template.is_none());
        assert!(args.since.is_none());
        assert_eq!(args.verbosity, 0);
    }

    #[test]
    fn test_pod_regex() -> Result<()> {
        // Purpose: Verify pod name regex pattern compilation and matching
        // Tests:
        // - Valid regex pattern compilation
        // - Correct matching against pod names
        // - Pattern specific to nginx pods
        let mut args = Args::default();
        args.pod_selector = "nginx-.*".to_string();
        
        let re = args.pod_regex()?;
        assert!(re.is_match("nginx-123"));
        assert!(re.is_match("nginx-abcd"));
        assert!(!re.is_match("apache-123"));
        
        Ok(())
    }

    #[test]
    fn test_container_regex() -> Result<()> {
        // Purpose: Verify container name regex pattern compilation and matching
        // Tests:
        // - Valid regex pattern compilation for container names
        // - Matches sidecar container patterns
        // - Proper handling of container name formats
        let mut args = Args::default();
        args.container = "side.*".to_string();
        
        let re = args.container_regex()?;
        assert!(re.is_match("sidecar"));
        assert!(re.is_match("side-init"));
        assert!(!re.is_match("main"));
        
        Ok(())
    }

    // Test parsing CLI arguments
    #[test]
    fn test_parse_args() {
        // Purpose: Verify parsing of command-line arguments into Args struct
        // Tests:
        // - Namespace specification (-n flag)
        // - Pod selector as positional argument
        // - Container name pattern (--container flag)
        // - Tail line count (--tail flag)
        // - Follow flag with explicit false
        // - Timestamp flag enablement
        // - Output format selection
        let args = Args::parse_from([
            "wake",
            "-n", "kube-system",
            "coredns.*",
            "--container", "dns",
            "--tail", "50",
            "--follow=false",
            "--timestamps",
            "--output", "json"
        ]);
        
        assert_eq!(args.namespace, "kube-system");
        assert_eq!(args.pod_selector, "coredns.*");
        assert_eq!(args.container, "dns");
        assert_eq!(args.tail, 50);
        assert_eq!(args.follow, false);
        assert_eq!(args.timestamps, true);
        assert_eq!(args.output, "json");
    }

    #[test]
    fn test_all_namespaces_flag() {
        // Purpose: Verify the all-namespaces flag behavior
        // Tests:
        // - -A flag sets all_namespaces to true
        // - Affects namespace selection behavior
        let args = Args::parse_from([
            "wake",
            "-A",
        ]);
        
        assert_eq!(args.all_namespaces, true);
    }

    #[test]
    fn test_include_exclude_filters() -> Result<()> {
        // Purpose: Verify log filtering patterns work correctly
        // Tests:
        // - Include pattern matches ERROR and WARN logs
        // - Exclude pattern filters out DEBUG logs
        // - Regex pattern compilation succeeds
        // - Pattern matching works as expected
        let args = Args::parse_from([
            "wake",
            "--include", "ERROR|WARN",
            "--exclude", "DEBUG",
        ]);
        
        assert!(args.include.is_some());
        assert!(args.exclude.is_some());
        
        let include_re = Regex::new(args.include.as_ref().unwrap())?;
        let exclude_re = Regex::new(args.exclude.as_ref().unwrap())?;
        
        assert!(include_re.is_match("ERROR: Failed to connect"));
        assert!(include_re.is_match("WARN: Connection slow"));
        assert!(!include_re.is_match("INFO: Starting up"));
        
        assert!(exclude_re.is_match("DEBUG: Variable x = 5"));
        assert!(!exclude_re.is_match("ERROR: Connection failed"));
        
        Ok(())
    }

    #[test]
    fn test_resource_type_parsing() {
        // Purpose: Verify parsing of resource type specifications
        // Tests:
        // - Resource type format (type/name)
        // - Valid resource type recognition
        // - Proper storage in args struct
        let args = Args::parse_from([
            "wake",
            "-r", "deployment/nginx",
        ]);
        
        assert!(args.resource.is_some());
        assert_eq!(args.resource.unwrap(), "deployment/nginx");
        
        // We could add more tests here to verify parsing of resource strings
        // into resource type and name once that functionality is implemented
    }

    #[test]
    fn test_default_arguments() {
        let args = Args::parse_from(&["wake"]);
        assert_eq!(args.namespace, Some("default".to_string()));
        assert_eq!(args.container, ".*");
        assert_eq!(args.pod_selector, ".*");
        assert_eq!(args.tail, 10);
        assert!(args.follow);
        assert!(!args.no_ui);
    }

    #[test]
    fn test_namespace_argument() {
        let args = Args::parse_from(&["wake", "-n", "kube-system"]);
        assert_eq!(args.namespace, Some("kube-system".to_string()));
    }

    #[test]
    fn test_container_filter() {
        let args = Args::parse_from(&["wake", "-c", "nginx"]);
        assert_eq!(args.container, "nginx");
    }

    #[test]
    fn test_pod_selector() {
        let args = Args::parse_from(&["wake", "-p", "app=myapp"]);
        assert_eq!(args.pod_selector, "app=myapp");
    }

    #[test]
    fn test_tail_lines() {
        let args = Args::parse_from(&["wake", "--tail", "50"]);
        assert_eq!(args.tail, 50);
    }

    #[test]
    fn test_no_follow() {
        let args = Args::parse_from(&["wake", "--no-follow"]);
        assert!(!args.follow);
    }

    #[test]
    fn test_include_exclude_patterns() {
        let args = Args::parse_from(&[
            "wake", 
            "-i", "ERROR",
            "-i", "WARN", 
            "-e", "debug",
            "-e", "trace"
        ]);
        assert_eq!(args.include, vec!["ERROR", "WARN"]);
        assert_eq!(args.exclude, vec!["debug", "trace"]);
    }

    #[test]
    fn test_output_file() {
        let args = Args::parse_from(&["wake", "-w", "output.log"]);
        assert_eq!(args.output_file, Some("output.log".to_string()));
    }

    #[test]
    fn test_buffer_size() {
        let args = Args::parse_from(&["wake", "--buffer-size", "5000"]);
        assert_eq!(args.buffer_size, 5000);
    }

    #[test]
    fn test_no_ui_mode() {
        let args = Args::parse_from(&["wake", "--no-ui"]);
        assert!(args.no_ui);
    }

    #[test]
    fn test_timestamps_flag() {
        let args = Args::parse_from(&["wake", "--timestamps"]);
        assert!(args.timestamps);
    }

    #[test]
    fn test_dev_mode() {
        let args = Args::parse_from(&["wake", "--dev"]);
        assert!(args.dev);
    }

    #[test]
    fn test_combined_flags() {
        let args = Args::parse_from(&[
            "wake", 
            "-n", "production",
            "-c", "web",
            "-p", "app=frontend",
            "--tail", "100",
            "--no-follow",
            "--timestamps",
            "--dev",
            "-i", "ERROR",
            "-e", "healthcheck"
        ]);
        
        assert_eq!(args.namespace, Some("production".to_string()));
        assert_eq!(args.container, "web");
        assert_eq!(args.pod_selector, "app=frontend");
        assert_eq!(args.tail, 100);
        assert!(!args.follow);
        assert!(args.timestamps);
        assert!(args.dev);
        assert_eq!(args.include, vec!["ERROR"]);
        assert_eq!(args.exclude, vec!["healthcheck"]);
    }
}