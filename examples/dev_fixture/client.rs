use anyhow::{Result, bail};
use codex_session_board::rpc::Rpc;
use serde_json::{Value, json};
use std::io::{Read, Write};
use tungstenite::{Message, WebSocket};

pub struct FixtureConnection<Stream> {
    socket: WebSocket<Stream>,
    serial: u64,
}

impl<Stream: Read + Write> FixtureConnection<Stream> {
    pub fn open(stream: Stream) -> Result<Self> {
        let (socket, _) = tungstenite::client("ws://localhost/rpc", stream)
            .map_err(|error| anyhow::anyhow!("{error}"))?;
        let mut client = Self { socket, serial: 0 };
        client.call("initialize", json!({"clientInfo":{"name":"fixture","version":"1"},
            "capabilities":{"experimentalApi":true}}))?;
        client.socket.send(Message::Text(json!({"method":"initialized"}).to_string().into()))?;
        Ok(client)
    }
}

impl<Stream: Read + Write> Rpc for FixtureConnection<Stream> {
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        self.serial += 1;
        self.socket.send(Message::Text(
            json!({"id":self.serial,"method":method,"params":params}).to_string().into()))?;
        loop {
            let message = self.socket.read()?;
            if !message.is_text() { continue; }
            let value: Value = serde_json::from_str(message.to_text()?)?;
            if value["id"] != self.serial { continue; }
            if !value["error"].is_null() { bail!("fixture RPC: {}", value["error"]); }
            return Ok(value["result"].clone());
        }
    }
}
