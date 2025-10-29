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

        let cloud_init_script = Self::convert_cloudinit_to_script(&cloud_init_content)?;
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

        let template_content = include_str!("../../../lima-template.yaml");
        let temp_path = "/tmp/capsule-vm-lima-template.yaml";
        fs::write(temp_path, template_content).context("Failed to write embedded Lima template")?;
        Ok(temp_path.to_string())
    }

    fn convert_cloudinit_to_script(cloud_init_yaml: &str) -> Result<String> {
        let mut script = String::new();
        let mut in_runcmd = false;

        for line in cloud_init_yaml.lines() {
            let trimmed = line.trim();
            if trimmed == "runcmd:" {
                in_runcmd = true;
                continue;
            }

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

        Ok(script)
    }
}
