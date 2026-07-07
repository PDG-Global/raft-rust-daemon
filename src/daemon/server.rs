//! Server module - Server management and WebSocket communication.

use anyhow::Result;
use dashmap::DashMap;
use futures::{sink::SinkExt, stream::StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio_tungstenite::{WebSocketStream, connect_async, tungstenite};
use tracing::info;

use crate::models::{Agent, Computer, Message, Server};

/// WebSocket message types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsMessageType {
    /// Server to daemon message.
    ServerToDaemon {
        id: String,
        timestamp: i64,
        #[serde(flatten)]
        data: serde_json::Map<String, serde_json::Value>,
    },
    /// Daemon to server message.
    DaemonToServer {
        id: String,
        timestamp: i64,
        agent_id: String,
        #[serde(flatten)]
        data: serde_json::Map<String, serde_json::Value>,
    },
}

/// Server manager for managing server connection.
#[allow(dead_code)] // scaffolding for server-driven control loop (see T11)
pub struct ServerManager {
    /// The server.
    server: Server,
    /// The computers.
    computers: DashMap<String, Computer>,
    /// The agents.
    agents: DashMap<String, Agent>,
    /// The state manager.
    state_manager: Arc<dyn crate::daemon::state::StateMgr>,
    /// The WebSocket connection.
    ws: Option<WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>>,
    /// The server URL.
    server_url: String,
    /// The API key.
    api_key: String,
}

impl ServerManager {
    /// Create a new server manager.
    pub fn new(
        server: Server,
        computers: DashMap<String, Computer>,
        agents: DashMap<String, Agent>,
        state_manager: Arc<dyn crate::daemon::state::StateMgr>,
        server_url: String,
        api_key: String,
    ) -> Self {
        Self {
            server,
            computers,
            agents,
            state_manager,
            ws: None,
            server_url,
            api_key,
        }
    }

    /// Connect to the server.
    ///
    /// # Errors
    ///
    /// Returns an error if `server_url` uses an unsupported or unencrypted
    /// scheme, the WebSocket handshake fails, or the authentication message
    /// cannot be sent.
    pub async fn connect(&mut self) -> Result<()> {
        info!("Connecting to server: {}", self.server_url);

        // Build the URL, ensuring it uses the wss:// scheme. Reject other
        // schemes so a misconfigured value cannot silently downgrade to plain
        // ws:// or be misinterpreted as a different protocol.
        let url = if self.server_url.starts_with("wss://") {
            self.server_url.clone()
        } else if self.server_url.starts_with("ws://") {
            anyhow::bail!(
                "refusing to connect over unencrypted ws://: set server_url to a wss:// URL"
            );
        } else if self.server_url.contains("://") {
            anyhow::bail!("unsupported scheme in server_url; use wss://");
        } else {
            format!("wss://{}", self.server_url)
        };

        // Connect to WebSocket
        let (mut ws, _response) = connect_async(&url).await?;

        // Authenticate
        let auth = tungstenite::Message::Text(format!("{{\"auth\":\"{}\"}}", self.api_key));
        ws.send(auth).await?;
        self.ws = Some(ws);

        Ok(())
    }

    /// Disconnect from the server.
    ///
    /// # Errors
    ///
    /// Returns an error if the WebSocket close handshake fails.
    pub async fn disconnect(&mut self) -> Result<()> {
        info!("Disconnecting from server");

        // Close the WebSocket
        if let Some(mut ws) = self.ws.take() {
            ws.close(None).await?;
        }

        Ok(())
    }

    /// Send a message to the server.
    ///
    /// # Errors
    ///
    /// Returns an error if there is no active connection, the message cannot
    /// be serialised, the send fails, or the server response is unexpected or
    /// malformed.
    pub async fn send_message(&mut self, message: Message) -> Result<String> {
        let ws = self
            .ws
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Not connected"))?;

        // Build the message
        let msg = serde_json::json!({
            "type": "message",
            "id": message.id,
            "timestamp": message.timestamp,
            "channel_id": message.channel_id,
            "thread_id": message.thread_id,
            "content": message.content,
            "metadata": message.metadata,
        });

        // Send the message
        let msg_str = serde_json::to_string(&msg)?;
        ws.send(tungstenite::Message::Text(msg_str)).await?;

        // Wait for response
        let response = ws.next().await;
        match response {
            Some(Ok(tungstenite::Message::Text(text))) => {
                let resp: WsMessageType = serde_json::from_str(&text)?;
                match resp {
                    WsMessageType::ServerToDaemon { id, .. } => Ok(id),
                    WsMessageType::DaemonToServer { .. } => {
                        Err(anyhow::anyhow!("Unexpected response type"))
                    }
                }
            }
            Some(Ok(_)) => Err(anyhow::anyhow!("Unexpected non-text response")),
            Some(Err(e)) => Err(anyhow::anyhow!("WebSocket error: {e}")),
            None => Err(anyhow::anyhow!("Connection closed waiting for response")),
        }
    }

    /// Get server info.
    ///
    /// # Errors
    ///
    /// Returns an error if no server is recorded in state and the live fetch is
    /// unimplemented.
    pub fn get_server_info(&self) -> Result<Server> {
        // Get the server from state manager
        if let Some(server) = self.state_manager.get_state().server.as_ref() {
            return Ok(server.clone());
        }

        // TODO: Fetch from server
        Err(anyhow::anyhow!("Server not found"))
    }

    /// Get agents in the server.
    pub fn get_agents(&self) -> &DashMap<String, Agent> {
        &self.agents
    }

    /// Get computers in the server.
    pub fn get_computers(&self) -> &DashMap<String, Computer> {
        &self.computers
    }
}
