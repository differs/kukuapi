//! SSE streaming utilities.
//!
//! Handles Server-Sent Events (SSE) streaming for both receiving
//! from upstream and sending to clients.

use async_stream::stream;
use futures::Stream;
use pin_project::pin_project;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;
use tracing::debug;

/// An SSE event from a stream.
#[derive(Debug, Clone)]
pub struct SseEvent {
    pub event: Option<String>,
    pub id: Option<String>,
    pub data: String,
    pub retry: Option<u64>,
}

impl SseEvent {
    /// Format as SSE wire format.
    pub fn to_sse(&self) -> String {
        let mut output = String::new();
        if let Some(ref id) = self.id {
            output.push_str(&format!("id: {}\n", id));
        }
        if let Some(ref event) = self.event {
            output.push_str(&format!("event: {}\n", event));
        }
        if let Some(retry) = self.retry {
            output.push_str(&format!("retry: {}\n", retry));
        }
        for line in self.data.lines() {
            output.push_str(&format!("data: {}\n", line));
        }
        output.push('\n');
        output
    }
}

/// Parse SSE lines into events.
pub fn parse_sse_lines(lines: &[&str]) -> Vec<SseEvent> {
    let mut events = Vec::new();
    let mut current = SseEvent {
        event: None,
        id: None,
        data: String::new(),
        retry: None,
    };

    for line in lines {
        if line.is_empty() {
            // Empty line signals end of event
            if !current.data.is_empty() || current.event.is_some() {
                events.push(current.clone());
                current = SseEvent {
                    event: None,
                    id: None,
                    data: String::new(),
                    retry: None,
                };
            }
        } else if let Some(rest) = line.strip_prefix("data:") {
            let value = rest.trim_start();
            if current.data.is_empty() {
                current.data = value.to_string();
            } else {
                current.data.push('\n');
                current.data.push_str(value);
            }
        } else if let Some(rest) = line.strip_prefix("event:") {
            current.event = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("id:") {
            current.id = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("retry:") {
            if let Ok(retry_ms) = rest.trim().parse::<u64>() {
                current.retry = Some(retry_ms);
            }
        }
    }

    // Don't forget the last event if no trailing newline
    if !current.data.is_empty() || current.event.is_some() {
        events.push(current);
    }

    events
}

/// Convert raw bytes to SSE events.
pub fn bytes_to_sse_events(bytes: &[u8]) -> Vec<SseEvent> {
    if let Ok(text) = std::str::from_utf8(bytes) {
        let lines: Vec<&str> = text.lines().collect();
        parse_sse_lines(&lines)
    } else {
        Vec::new()
    }
}

/// Create a stream that reads SSE from a tokio io reader.
#[pin_project]
pub struct SseStream<R> {
    #[pin]
    reader: BufReader<R>,
}

impl<R> SseStream<R>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    pub fn new(reader: R) -> Self {
        Self {
            reader: BufReader::new(reader),
        }
    }
}

impl<R> Stream for SseStream<R>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    type Item = Result<SseEvent, std::io::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();
        let reader = this.reader;

        // Read lines until we get a complete SSE event
        let mut line = String::new();
        match Pin::new(&mut line).poll_read_line(reader, cx) {
            Poll::Ready(Ok(0)) => Poll::Ready(None), // EOF
            Poll::Ready(Ok(_)) => {
                if line.ends_with('\n') {
                    line.pop(); // Remove \n
                    if line.ends_with('\r') {
                        line.pop(); // Remove \r
                    }
                }

                // We need to accumulate lines into events.
                // For simplicity, yield each line as a data-only event.
                // A production implementation would track state across polls.
                Poll::Ready(Some(Ok(SseEvent {
                    event: None,
                    id: None,
                    data: line,
                    retry: None,
                })))
            }
            Poll::Ready(Err(e)) => Poll::Ready(Some(Err(e))),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Create a channel-based SSE stream from a byte receiver.
pub fn create_sse_channel_stream(
    rx: mpsc::Receiver<Vec<u8>>,
) -> impl Stream<Item = Vec<u8>> {
    stream! {
        let mut buffer = String::new();

        while let Some(chunk) = rx.recv().await {
            if let Ok(text) = String::from_utf8(chunk.clone()) {
                buffer.push_str(&text);

                // Process complete events
                while let Some(pos) = buffer.find("\n\n") {
                    let event_str = &buffer[..pos];
                    buffer = buffer[pos + 2..].to_string();

                    let events = parse_sse_lines(&[event_str]);
                    for event in events {
                        let sse_formatted = event.to_sse();
                        yield sse_formatted.into_bytes();
                    }
                }
            } else {
                // Binary data - forward as-is
                yield chunk;
            }
        }

        // Flush remaining data
        if !buffer.is_empty() {
            yield buffer.into_bytes();
        }
    }
}

/// Ping/SSE keepalive event for long-running streams.
pub fn keepalive_ping() -> Vec<u8> {
    ":keepalive\n\n".as_bytes().to_vec()
}
