#!/bin/bash
cd orchestrator
cat > /tmp/test_yaml_parse.rs << 'RUST_EOF'
fn main() {
    use std::path::Path;
    
    let yaml_path = Path::new("../capsule.yaml");
    match capsule::config::load_config(yaml_path) {
        Ok(config) => {
            println!("✓ Successfully parsed capsule.yaml");
            println!("  VM name: {}", config.vm.name);
            println!("  CPUs: {}", config.vm.cpus);
            println!("  Memory: {}", config.vm.memory);
            println!("  Disk: {}", config.vm.disk);
            println!("  Security profile: {}", config.security.profile);
            println!("  Tracing enabled: {}", config.tracing.enabled);
            println!("  Runtimes: {} configured", config.tools.runtimes.len());
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("✗ Failed to parse capsule.yaml: {}", e);
            std::process::exit(1);
        }
    }
}
RUST_EOF

# Note: This would require capsule to be a library crate to test this way
# For now, let's just verify the YAML structure with serde_yaml dire
