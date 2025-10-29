use anyhow::{Context, Result};
use std::thread;
use std::time::Duration;

/// Configuration for retry logic with exponential backoff
#[derive(Clone, Debug)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub initial_delay: Duration,
    pub max_delay: Duration,
    pub backoff_factor: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(30),
            backoff_factor: 2.0,
        }
    }
}

impl RetryConfig {
    pub fn new(max_attempts: u32) -> Self {
        Self {
            max_attempts,
            ..Default::default()
        }
    }

    pub fn with_delays(max_attempts: u32, initial_delay: Duration, max_delay: Duration) -> Self {
        Self {
            max_attempts,
            initial_delay,
            max_delay,
            backoff_factor: 2.0,
        }
    }
}

/// Executes an operation with retry logic and exponential backoff
///
/// # Arguments
/// * `operation` - Closure that performs the operation
/// * `config` - Retry configuration
/// * `operation_name` - Human-readable name for logging
///
/// # Returns
/// Result of the operation, or the last error if all retries failed
pub fn retry_operation<F, T>(
    mut operation: F,
    config: RetryConfig,
    operation_name: &str,
) -> Result<T>
where
    F: FnMut() -> Result<T>,
{
    let mut attempt = 0;
    let mut delay = config.initial_delay;

    loop {
        attempt += 1;

        if attempt > 1 {
            println!(
                "🔄 Retry attempt {}/{}: {}",
                attempt, config.max_attempts, operation_name
            );
        }

        match operation() {
            Ok(result) => {
                if attempt > 1 {
                    println!("✅ Succeeded after {} attempts", attempt);
                }
                return Ok(result);
            }
            Err(e) if attempt >= config.max_attempts => {
                return Err(e).context(format!(
                    "Operation '{}' failed after {} attempts",
                    operation_name, attempt
                ));
            }
            Err(e) => {
                eprintln!(
                    "⚠️  Attempt {}/{} failed: {}. Retrying in {:?}...",
                    attempt, config.max_attempts, e, delay
                );
                thread::sleep(delay);

                // Calculate next delay with exponential backoff
                let next_delay_secs = (delay.as_secs_f64() * config.backoff_factor)
                    .min(config.max_delay.as_secs_f64());
                delay = Duration::from_secs_f64(next_delay_secs);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_succeeds_first_attempt() {
        let mut counter = 0;
        let result: Result<i32> = retry_operation(
            || {
                counter += 1;
                Ok(42)
            },
            RetryConfig::default(),
            "test",
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter, 1);
    }

    #[test]
    fn test_retry_succeeds_second_attempt() {
        let mut counter = 0;
        let result: Result<i32> = retry_operation(
            || {
                counter += 1;
                if counter == 1 {
                    anyhow::bail!("first attempt fails")
                } else {
                    Ok(42)
                }
            },
            RetryConfig::default(),
            "test",
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter, 2);
    }

    #[test]
    fn test_retry_fails_all_attempts() {
        let mut counter = 0;
        let result: Result<()> = retry_operation(
            || {
                counter += 1;
                Err(anyhow::anyhow!("always fails"))
            },
            RetryConfig::new(3),
            "test",
        );
        assert!(result.is_err());
        assert_eq!(counter, 3);
    }
}
