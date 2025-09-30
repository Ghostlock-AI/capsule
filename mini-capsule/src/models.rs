use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub uuid: Uuid,
    pub timestamp: DateTime<Utc>,
    pub os: String,
    pub chipset: String,
    pub working_dir: String,
    pub program: String,
    pub args: String,
    pub end_timestamp: Option<DateTime<Utc>>,
}

impl SessionMetadata {
    pub fn new(program: String, args: Vec<String>, working_dir: String) -> Self {
        Self {
            uuid: Uuid::new_v4(),
            timestamp: Utc::now(),
            os: Self::get_os_info(),
            chipset: Self::get_chipset_info(),
            working_dir,
            program,
            args: args.join(" "),
            end_timestamp: None,
        }
    }

    pub fn complete(&mut self) {
        self.end_timestamp = Some(Utc::now());
    }

    fn get_os_info() -> String {
        use sysinfo::System;
        let sys = System::new_all();
        format!(
            "{} {}",
            System::name().unwrap_or_else(|| "Unknown".to_string()),
            System::os_version().unwrap_or_else(|| "Unknown".to_string())
        )
    }

    fn get_chipset_info() -> String {
        use sysinfo::System;
        let sys = System::new_all();
        System::cpu_arch().unwrap_or_else(|| "Unknown".to_string())
    }
}