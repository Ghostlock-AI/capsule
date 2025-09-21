//! Streaming utilities for wiring broadcast channels to listeners.

use anyhow::Result;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use tokio::fs;
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

/// High-performance async writer used by file listeners.
pub struct StreamWriter {
    writer: BufWriter<tokio::fs::File>,
    line_count: u64,
    file_path: PathBuf,
}

impl StreamWriter {
    pub async fn new(file_path: PathBuf) -> Result<Self> {
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let file = tokio::fs::File::create(&file_path).await?;
        let writer = BufWriter::with_capacity(64 * 1024, file);

        info!("Created stream writer for {:?}", file_path);

        Ok(Self {
            writer,
            line_count: 0,
            file_path,
        })
    }

    pub async fn write_line(&mut self, line: &str) -> Result<()> {
        self.writer.write_all(line.as_bytes()).await?;
        self.writer.write_all(b"\n").await?;

        self.line_count += 1;

        if self.line_count % 100 == 0 {
            self.writer.flush().await?;
            debug!("Flushed {} lines to {:?}", self.line_count, self.file_path);
        }

        Ok(())
    }

    pub async fn close(mut self) -> Result<()> {
        self.writer.flush().await?;
        info!(
            "Closed stream writer for {:?} after {} lines",
            self.file_path, self.line_count
        );
        Ok(())
    }
}

/// Future type returned by listeners when they are attached to a stream.
pub type ListenerFuture = Pin<Box<dyn Future<Output = Result<()>> + Send>>;

/// StreamListener consumes messages from a broadcast receiver until cancellation.
pub trait StreamListener<T>: Send + Sync + 'static
where
    T: Clone + Send + 'static,
{
    fn name(&self) -> &str;
    fn run(
        self: Box<Self>,
        rx: broadcast::Receiver<T>,
        cancellation_token: CancellationToken,
    ) -> ListenerFuture;
}

/// Configures a broadcast stream along with the listeners that should observe it.
pub struct StreamBuilder<T>
where
    T: Clone + Send + 'static,
{
    name: String,
    capacity: usize,
    listeners: Vec<Box<dyn StreamListener<T>>>,
}

impl<T> StreamBuilder<T>
where
    T: Clone + Send + 'static,
{
    pub fn new(name: impl Into<String>, capacity: usize) -> Self {
        Self {
            name: name.into(),
            capacity,
            listeners: Vec::new(),
        }
    }

    pub fn add_listener<L>(&mut self, listener: L)
    where
        L: StreamListener<T>,
    {
        self.listeners.push(Box::new(listener));
    }

    pub fn add_listener_box(&mut self, listener: Box<dyn StreamListener<T>>) {
        self.listeners.push(listener);
    }

    pub fn build(
        self,
        cancellation_token: CancellationToken,
    ) -> (broadcast::Sender<T>, Vec<JoinHandle<Result<()>>>) {
        let (tx, _) = broadcast::channel::<T>(self.capacity);
        info!(
            "Building stream '{}' with {} listener(s)",
            self.name,
            self.listeners.len()
        );

        let mut handles = Vec::new();

        for listener in self.listeners {
            let listener_name = listener.name().to_string();
            let rx = tx.subscribe();
            let fut = listener.run(rx, cancellation_token.clone());

            let handle = tokio::spawn(async move {
                info!("Listener '{}' started", listener_name);
                let result = fut.await;
                match &result {
                    Ok(()) => info!("Listener '{}' completed", listener_name),
                    Err(err) => error!("Listener '{}' error: {}", listener_name, err),
                }
                result
            });

            handles.push(handle);
        }

        (tx, handles)
    }
}

/// Listener that writes each message to disk using the provided formatter.
pub struct FileStreamListener<T>
where
    T: Clone + Send + 'static,
{
    name: String,
    file_path: PathBuf,
    formatter: Arc<dyn Fn(&T) -> Result<String> + Send + Sync>,
}

impl<T> FileStreamListener<T>
where
    T: Clone + Send + 'static,
{
    pub fn new(
        name: impl Into<String>,
        file_path: PathBuf,
        formatter: impl Fn(&T) -> Result<String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            name: name.into(),
            file_path,
            formatter: Arc::new(formatter),
        }
    }

    pub fn plain(file_path: PathBuf) -> Self
    where
        T: ToString,
    {
        Self::new("file-writer", file_path, |value: &T| Ok(value.to_string()))
    }

    pub fn jsonl(file_path: PathBuf) -> Self
    where
        T: serde::Serialize,
    {
        Self::new("file-jsonl", file_path, |value: &T| {
            Ok(serde_json::to_string(value)?)
        })
    }
}

impl<T> StreamListener<T> for FileStreamListener<T>
where
    T: Clone + Send + 'static,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn run(
        self: Box<Self>,
        mut rx: broadcast::Receiver<T>,
        cancellation_token: CancellationToken,
    ) -> ListenerFuture {
        let file_path = self.file_path.clone();
        let formatter = self.formatter.clone();
        let display_name = self.name.clone();

        Box::pin(async move {
            let mut writer = StreamWriter::new(file_path.clone()).await?;

            loop {
                tokio::select! {
                    value = rx.recv() => {
                        match value {
                            Ok(msg) => {
                                let line = (formatter)(&msg)?;
                                if let Err(e) = writer.write_line(&line).await {
                                    error!("Failed to write {}: {}", display_name, e);
                                    return Err(e.into());
                                }
                            },
                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                warn!("Listener '{}' lagged by {} messages", display_name, n);
                            },
                            Err(broadcast::error::RecvError::Closed) => {
                                info!("Listener '{}' stream closed", display_name);
                                break;
                            },
                        }
                    },
                    _ = cancellation_token.cancelled() => {
                        info!("Listener '{}' received cancellation", display_name);
                        break;
                    }
                }
            }

            if let Err(e) = writer.close().await {
                error!("Error closing writer for '{}': {}", display_name, e);
            }

            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use tokio::time::{timeout, Duration};

    struct MemoryListener {
        name: &'static str,
        seen: Arc<Mutex<Vec<String>>>,
    }

    impl MemoryListener {
        fn new(name: &'static str, seen: Arc<Mutex<Vec<String>>>) -> Self {
            Self { name, seen }
        }
    }

    impl StreamListener<String> for MemoryListener {
        fn name(&self) -> &str {
            self.name
        }

        fn run(
            self: Box<Self>,
            mut rx: broadcast::Receiver<String>,
            cancellation_token: CancellationToken,
        ) -> ListenerFuture {
            let seen = self.seen.clone();
            let name = self.name;

            Box::pin(async move {
                loop {
                    tokio::select! {
                        value = rx.recv() => {
                            match value {
                                Ok(msg) => {
                                    let mut guard = seen.lock().await;
                                    guard.push(msg);
                                },
                                Err(broadcast::error::RecvError::Lagged(n)) => {
                                    warn!("Memory listener '{}' lagged by {} messages", name, n);
                                },
                                Err(broadcast::error::RecvError::Closed) => break,
                            }
                        },
                        _ = cancellation_token.cancelled() => break,
                    }
                }

                Ok(())
            })
        }
    }

    #[tokio::test]
    async fn stream_builder_delivers_messages() -> Result<()> {
        let mut builder = StreamBuilder::new("test-stream", 4);
        let seen = Arc::new(Mutex::new(Vec::new()));
        builder.add_listener(MemoryListener::new("memory", seen.clone()));

        let cancellation = CancellationToken::new();
        let (tx, handles) = builder.build(cancellation.clone());

        tx.send("hello".to_string())?;
        tx.send("world".to_string())?;

        timeout(Duration::from_millis(200), async {
            loop {
                if seen.lock().await.len() >= 2 {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("listener did not receive messages in time");

        cancellation.cancel();

        for handle in handles {
            let result = handle.await?;
            result?;
        }

        let guard = seen.lock().await;
        assert_eq!(guard.len(), 2);
        assert_eq!(guard[0], "hello");
        assert_eq!(guard[1], "world");

        Ok(())
    }
}
