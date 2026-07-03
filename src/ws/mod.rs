//! WebSocket support for OpenAI Responses API v2.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query};
use axum::response::IntoResponse;
use futures::StreamExt;
use futures::SinkExt;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{info, error};

/// Handle WebSocket upgrade for Responses API v2.
pub async fn handle_ws_upgrade(
    ws: WebSocketUpgrade,
    auth: Option<axum::extract::Extension<crate::middleware::api_key_auth::AuthenticatedKey>>,
    Query(_params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    match auth {
        Some(_key) => {
            ws.on_upgrade(move |socket| handle_ws_connection(socket))
        }
        None => axum::http::StatusCode::UNAUTHORIZED.into_response(),
    }
}

/// Handle an established WebSocket connection.
async fn handle_ws_connection(ws: WebSocket) {
    info!("WebSocket connection established");

    let (mut ws_sender, mut ws_receiver) = ws.split();

    // Channel for outgoing messages
    let (tx, rx) = mpsc::channel::<String>(256);
    let mut rx_stream = ReceiverStream::new(rx);

    // Use select to handle both sending and receiving
    loop {
        tokio::select! {
            // Incoming WS message from client
            msg = ws_receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        info!("WS received: {}", text);
                        let response = format!(
                            r#"{{"type":"response.created","response":{{"id":"{}"}}}}"#,
                            uuid::Uuid::new_v4()
                        );
                        let _ = tx.send(response).await;
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(e)) => {
                        error!("WS receive error: {}", e);
                        break;
                    }
                    _ => {}
                }
            }

            // Outgoing message from channel to WS
            msg = rx_stream.next() => {
                match msg {
                    Some(text) => {
                        if ws_sender.send(Message::Text(text.into())).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
        }
    }

    info!("WebSocket connection closed");
}

/// Build the upstream WebSocket URL for OpenAI Responses API v2.
pub fn build_upstream_ws_url(model: &str, token: &str) -> String {
    let mut url = url::Url::parse("wss://api.openai.com/v2/responses").expect("Invalid URL");
    url.query_pairs_mut()
        .append_pair("model", model)
        .append_pair("token", token);
    url.to_string()
}
