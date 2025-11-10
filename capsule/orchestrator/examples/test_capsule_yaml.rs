// Test that the example capsule.yaml file parses correctly
//
// Run with: cargo run --example test_capsule_yaml

use std::path::Path;

// Import config types - note that this won't work until the crate is a library
// For now this is a placeholder showing the intended API
fn main() -> anyhow::Result<()> {
    println!("Testing capsule.yaml parsing...\n");

    let yaml_path = Path::new("/Users/jakehenderson/nocuments/code/ghostlock/capsule/capsule.yaml");

    // Read and parse the YAML file
    let content = std::fs::read_to_string(yaml_path)?;
    println!("Read {} bytes from capsule.yaml", content.len());

    // Try to parse with serde_yaml directly
    let config: serde_yaml::Value = serde_yaml::from_str(&content)?;

    println!("\n✓ YAML structure is valid!");
    println!("\nTop-level keys:");
    if let serde_yaml::Value::Mapping(map) = config {
        for (key, _) in map.iter() {
            if let serde_yaml::Value::String(k) = key {
                println!("  - {}", k);
            }
        }
    }

    println!("\nNote: Full config parsing requires the config module to be integrated.");
    println!("This example verifies the YAML syntax is valid.");

    Ok(())
}
