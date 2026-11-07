use super::*;
use crate::remote::hosts::source::{Binding, Peer};

pub(crate) const CLOCK: &str = "exec python3 -B -c 'import time; print(time.time(), flush=True)'";
pub(crate) const PROXY: &str = "exec codex app-server proxy";
const BANNER_LIMIT: usize = 65536;
const PEER_LIMIT: usize = 8192;

impl DesktopConnection {
    pub(crate) fn remote(binding: &Binding, timeout: Duration, role: Role) -> Result<Self> {
        let budget = if role == Role::TitleWriter {
            timeout.min(Duration::from_secs(3))
        } else {
            timeout
        };
        let deadline = deadline_after(budget)?;
        let (transport, peer) = open(binding, PROXY, deadline)?;
        let mut connection =
            Self::handshake_bound(transport, timeout, role, deadline, Some((binding, &peer)))?;
        connection.remote_binding = Some(binding.clone());
        connection.peer = Some(peer);
        Ok(connection)
    }
}

fn byte(reader: &mut impl Read) -> Result<u8> {
    let mut value = [0];
    reader
        .read_exact(&mut value)
        .map_err(|error| anyhow!("远端启动协议未完成: {}", error.kind()))?;
    Ok(value[0])
}

fn header(reader: &mut impl Read, marker: &str) -> Result<Peer> {
    let marker = format!("{marker}\n");
    let mut banner = Vec::new();
    loop {
        ensure!(
            banner.len() < BANNER_LIMIT + marker.len(),
            "远端登录 banner 超限或启动标记不匹配"
        );
        banner.push(byte(reader)?);
        if banner.ends_with(marker.as_bytes()) {
            break;
        }
    }
    let mut line = Vec::new();
    loop {
        ensure!(line.len() < PEER_LIMIT, "远端身份响应过长");
        let value = byte(reader)?;
        if value == b'\n' {
            break;
        }
        line.push(value);
    }
    let peer: Peer = serde_json::from_slice(&line).map_err(|_| anyhow!("远端身份响应格式无效"))?;
    peer.validate()?;
    Ok(peer)
}

pub(super) fn open(
    binding: &Binding,
    operation: &str,
    deadline: Instant,
) -> Result<(pipe::Transport, Peer)> {
    let binding = binding.current()?;
    binding.verify(deadline)?;
    let marker = uuid::Uuid::new_v4().simple().to_string();
    let command = binding.launch(operation, &marker)?;
    let mut stream = DeadlineStream {
        stream: pipe::Transport::proxy(&command)?,
        deadline,
    };
    let peer = header(&mut stream, &marker)?;
    binding.check_peer(&peer)?;
    Ok((stream.stream, peer))
}

pub(super) fn clock(binding: &Binding, deadline: Instant) -> Result<(String, Peer)> {
    let (transport, peer) = open(binding, CLOCK, deadline)?;
    let mut stream = DeadlineStream {
        stream: transport,
        deadline,
    };
    Ok((pipe::line(&mut stream)?, peer))
}

pub(super) fn verify_current(binding: &Binding, deadline: Instant) -> Result<()> {
    let _ = open(binding, ":", deadline)?;
    Ok(())
}

pub(crate) fn guard(binding: &Binding, timeout: Duration) -> Result<()> {
    binding.require_confirmed()?;
    verify_current(binding, deadline_after(timeout)?)
}

#[cfg(test)]
#[path = "framing_tests.rs"]
mod tests;
