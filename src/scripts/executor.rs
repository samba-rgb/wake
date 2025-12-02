use super::manager::SavedScript;
use crate::k8s::pod::PodInfo;
use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::process::Command as AsyncCommand;

/// Execution result for a script on a single pod
#[derive(Debug, Clone)]
pub struct PodScriptResult {
    pub pod_name: String,
    pub pod_namespace: String,
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Overall execution result
#[derive(Debug, Clone)]
pub struct ScriptExecutionResult {
    pub script_name: String,
    pub pod_results: Vec<PodScriptResult>,
    pub output_dir: PathBuf,
    pub merge_outputs: bool,
}

/// Script executor for running saved scripts on pods
pub struct ScriptExecutor;

impl ScriptExecutor {
    /// Execute a script on all selected pods
    pub async fn execute_script(
        script: &SavedScript,
        arguments: HashMap<String, String>,
        pods: &[PodInfo],
        output_dir: PathBuf,
    ) -> Result<ScriptExecutionResult> {
        // Create output directory
        std::fs::create_dir_all(&output_dir)?;

        // Prepare the script with argument substitution
        let resolved_script = Self::substitute_arguments(&script.script_content, &arguments)?;

        let mut pod_results = Vec::new();

        // Execute script on all pods
        for pod in pods {
            let result = Self::execute_on_single_pod(&resolved_script, pod).await;
            
            match result {
                Ok(pod_result) => {
                    pod_results.push(pod_result);
                }
                Err(e) => {
                    pod_results.push(PodScriptResult {
                        pod_name: pod.name.clone(),
                        pod_namespace: pod.namespace.clone(),
                        success: false,
                        stdout: String::new(),
                        stderr: format!("Execution failed: {e}"),
                        exit_code: -1,
                    });
                }
            }
        }

        Ok(ScriptExecutionResult {
            script_name: script.name.clone(),
            pod_results,
            output_dir,
            merge_outputs: false,
        })
    }

    /// Execute script on a single pod
    async fn execute_on_single_pod(script_content: &str, pod: &PodInfo) -> Result<PodScriptResult> {
        let container = pod.containers.first().cloned().unwrap_or_else(|| "default".to_string());
        
        // Create a temp script file in the pod
        let escaped_script = script_content.replace("'", "'\\''");
        let copy_cmd = format!("echo '{}' > /tmp/wake-custom-script.sh && chmod +x /tmp/wake-custom-script.sh", escaped_script);
        
        // Copy script to pod
        let mut copy_process = AsyncCommand::new("kubectl")
            .arg("exec")
            .arg("-n")
            .arg(&pod.namespace)
            .arg(&pod.name)
            .arg("-c")
            .arg(&container)
            .arg("--")
            .arg("sh")
            .arg("-c")
            .arg(&copy_cmd)
            .output()
            .await?;

        if !copy_process.status.success() {
            return Err(anyhow!(
                "Failed to copy script to pod {}: {}",
                pod.name,
                String::from_utf8_lossy(&copy_process.stderr)
            ));
        }

        // Execute the script
        let mut exec_process = AsyncCommand::new("kubectl")
            .arg("exec")
            .arg("-n")
            .arg(&pod.namespace)
            .arg(&pod.name)
            .arg("-c")
            .arg(&container)
            .arg("--")
            .arg("sh")
            .arg("/tmp/wake-custom-script.sh")
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&exec_process.stdout).to_string();
        let stderr = String::from_utf8_lossy(&exec_process.stderr).to_string();
        let exit_code = exec_process.status.code().unwrap_or(-1);

        Ok(PodScriptResult {
            pod_name: pod.name.clone(),
            pod_namespace: pod.namespace.clone(),
            success: exec_process.status.success(),
            stdout,
            stderr,
            exit_code,
        })
    }

    /// Substitute argument placeholders in script
    fn substitute_arguments(
        script: &str,
        arguments: &HashMap<String, String>,
    ) -> Result<String> {
        let mut result = script.to_string();
        
        for (name, value) in arguments {
            let placeholder = format!("${{{}}}", name);
            result = result.replace(&placeholder, value);
        }
        
        Ok(result)
    }

    /// Save script output to file
    pub fn save_output_to_file(
        result: &ScriptExecutionResult,
        pod_result: &PodScriptResult,
        filename: &str,
    ) -> Result<PathBuf> {
        let output_path = result.output_dir.join(filename);
        
        let mut content = String::new();
        content.push_str(&format!("Pod: {}/{}\n", pod_result.pod_namespace, pod_result.pod_name));
        content.push_str(&format!("Exit Code: {}\n", pod_result.exit_code));
        content.push_str("---STDOUT---\n");
        content.push_str(&pod_result.stdout);
        content.push_str("\n---STDERR---\n");
        content.push_str(&pod_result.stderr);
        
        std::fs::write(&output_path, content)?;
        Ok(output_path)
    }

    /// Merge all pod outputs into a single file
    pub fn merge_outputs(result: &ScriptExecutionResult, filename: &str) -> Result<PathBuf> {
        let output_path = result.output_dir.join(filename);
        
        let mut content = String::new();
        content.push_str(&format!("Script: {}\n", result.script_name));
        content.push_str(&format!("Execution Date: {}\n", chrono::Utc::now()));
        content.push_str("=".repeat(80));
        content.push_str("\n\n");
        
        for pod_result in &result.pod_results {
            content.push_str(&format!("Pod: {}/{}\n", pod_result.pod_namespace, pod_result.pod_name));
            content.push_str(&format!("Exit Code: {}\n", pod_result.exit_code));
            content.push_str("---STDOUT---\n");
            content.push_str(&pod_result.stdout);
            content.push_str("\n---STDERR---\n");
            content.push_str(&pod_result.stderr);
            content.push_str("\n");
            content.push_str("=".repeat(80));
            content.push_str("\n\n");
        }
        
        std::fs::write(&output_path, content)?;
        Ok(output_path)
    }
}
