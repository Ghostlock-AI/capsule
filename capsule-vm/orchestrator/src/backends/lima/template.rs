use super::LimaBackend;
use crate::vm_backend::VmConfig;
use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

impl LimaBackend {
    pub(super) fn render_template_with_cloudinit(&self, config: &VmConfig) -> Result<String> {
        let new_template_path = PathBuf::from("templates/lima-base.yaml");
        if !new_template_path.exists() {
            return self.get_template_path();
        }

        let template_content = fs::read_to_string(&new_template_path)
            .context("Failed to read templates/lima-base.yaml")?;

        let cloud_init_path = config
            .cloud_init
            .clone()
            .unwrap_or_else(|| "./cloud-init.yaml".to_string());

        let cloud_init_content =
            fs::read_to_string(&cloud_init_path).context("Failed to read cloud-init file")?;

        let mut cloud_init_script = Self::convert_cloudinit_to_script(&cloud_init_content)?;

        if !config.post_install_commands.is_empty() {
            cloud_init_script.push_str("      # Capsule tool bundles\n");
            for command in &config.post_install_commands {
                cloud_init_script.push_str("      ");
                cloud_init_script.push_str(command);
                cloud_init_script.push('\n');
            }
        }

        let rendered = template_content.replace("{{CLOUD_INIT_CONTENT}}", &cloud_init_script);

        let runtime_dir = PathBuf::from(std::env::var("HOME")?).join(".capsule-vm/runtime");
        fs::create_dir_all(&runtime_dir)?;

        let output_path = runtime_dir.join(format!("{}.yaml", config.name));
        fs::write(&output_path, rendered)?;

        Ok(output_path.to_string_lossy().to_string())
    }

    fn get_template_path(&self) -> Result<String> {
        let local_template = PathBuf::from("./lima-template.yaml");
        if local_template.exists() {
            return Ok(local_template.to_string_lossy().to_string());
        }

        if let Ok(exe_path) = std::env::current_exe()
            && let Some(exe_template) = exe_path.parent().map(|dir| dir.join("lima-template.yaml"))
            && exe_template.exists()
        {
            return Ok(exe_template.to_string_lossy().to_string());
        }

        let template_content = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../lima-template.yaml"
        ));
        let temp_path = "/tmp/capsule-vm-lima-template.yaml";
        fs::write(temp_path, template_content).context("Failed to write embedded Lima template")?;
        Ok(temp_path.to_string())
    }

    fn convert_cloudinit_to_script(cloud_init_yaml: &str) -> Result<String> {
        let mut script = String::new();
        let mut in_write_files = false;
        let mut in_runcmd = false;
        let mut current_file_path: Option<String> = None;
        let mut current_file_perms: Option<String> = None;
        let mut in_content = false;
        let mut content_buffer = String::new();
        let mut content_indent = 0;

        for line in cloud_init_yaml.lines() {
            let trimmed = line.trim();

            // Track write_files section
            if trimmed == "write_files:" {
                in_write_files = true;
                in_runcmd = false;
                continue;
            }

            // Track runcmd section
            if trimmed == "runcmd:" {
                in_runcmd = true;
                in_write_files = false;

                // Flush any pending file before starting runcmd
                if let Some(path) = current_file_path.take() {
                    Self::append_file_creation(
                        &mut script,
                        &path,
                        &current_file_perms
                            .take()
                            .unwrap_or_else(|| "0644".to_string()),
                        &content_buffer,
                    );
                    content_buffer.clear();
                }
                continue;
            }

            // Process write_files entries
            if in_write_files {
                if line.starts_with("  - path: ") {
                    // Flush previous file
                    if let Some(path) = current_file_path.take() {
                        Self::append_file_creation(
                            &mut script,
                            &path,
                            &current_file_perms
                                .take()
                                .unwrap_or_else(|| "0644".to_string()),
                            &content_buffer,
                        );
                        content_buffer.clear();
                    }

                    current_file_path =
                        Some(line.trim_start_matches("  - path: ").trim().to_string());
                    in_content = false;
                } else if line.starts_with("    permissions: ") {
                    current_file_perms = Some(
                        line.trim_start_matches("    permissions: ")
                            .trim()
                            .trim_matches('"')
                            .to_string(),
                    );
                } else if line.starts_with("    content: |") {
                    in_content = true;
                    content_indent = 6; // "      " is the base indent for content
                    content_buffer.clear();
                } else if in_content {
                    // Stop processing content when we hit another field or section
                    if !line.starts_with("      ") && !trimmed.is_empty() {
                        in_content = false;
                    } else if line.starts_with("      ") {
                        // Remove the base content indent
                        content_buffer.push_str(&line[content_indent..]);
                        content_buffer.push('\n');
                    }
                }

                // Exit write_files when we hit a top-level section
                if !trimmed.is_empty()
                    && !line.starts_with("  ")
                    && !line.starts_with("    ")
                    && trimmed != "write_files:"
                {
                    in_write_files = false;
                }
            }

            // Process runcmd entries
            if in_runcmd {
                if !line.starts_with("  - ") && !trimmed.is_empty() && !line.starts_with("    ") {
                    break;
                }

                if let Some(cmd) = line.strip_prefix("  - ") {
                    script.push_str("      ");
                    script.push_str(cmd);
                    script.push('\n');
                }
            }
        }

        // Flush any remaining file
        if let Some(path) = current_file_path.take() {
            Self::append_file_creation(
                &mut script,
                &path,
                &current_file_perms
                    .take()
                    .unwrap_or_else(|| "0644".to_string()),
                &content_buffer,
            );
        }

        Ok(script)
    }

    fn append_file_creation(script: &mut String, path: &str, perms: &str, content: &str) {
        // Create parent directory if needed
        if let Some(parent) = std::path::Path::new(path).parent() {
            if parent != std::path::Path::new("/") {
                script.push_str(&format!("      mkdir -p {}\n", parent.display()));
            }
        }

        // Write file using cat with heredoc
        script.push_str(&format!("      cat > {} << 'CAPSULE_EOF'\n", path));
        for line in content.lines() {
            script.push_str("      ");
            script.push_str(line);
            script.push('\n');
        }
        script.push_str("      CAPSULE_EOF\n");
        script.push_str(&format!("      chmod {} {}\n", perms, path));
    }
}
