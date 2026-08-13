use super::*;

#[cfg(test)]
impl VmWebSocketRuntime {
    /// Creates an empty WebSocket runtime registry.
    #[cfg(test)]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Opens a VM WebSocket session for an accepted VM TCP stream.
    #[cfg(test)]
    pub(crate) fn open_session(&mut self, stream: VmTcpStream) -> VmWebSocketSessionId {
        self.next_session = self.next_session.saturating_add(1);
        let session = VmWebSocketSessionId {
            id: self.next_session,
        };
        self.sessions
            .insert(session.id, VmWebSocketSession::new(stream));
        session
    }

    /// Opens a WebSocket session after validating the VM TCP stream handle.
    #[cfg(test)]
    pub(crate) fn open_session_checked(
        &mut self,
        tcp: &VmTcpRuntime,
        stream: VmTcpStream,
    ) -> Result<VmWebSocketSessionId, String> {
        let stream_info = tcp.inspect_stream(stream).map_err(|error| {
            format!("error[vm_websocket_tcp]: cannot open session for stream: {error}")
        })?;
        if stream_info.closed {
            return Err(
                "error[vm_websocket_session]: cannot open session for closed TCP stream"
                    .to_string(),
            );
        }
        if stream_info.cancelled {
            return Err(
                "error[vm_websocket_session]: cannot open session for cancelled TCP stream"
                    .to_string(),
            );
        }
        if self
            .sessions
            .values()
            .any(|session| session.stream == stream)
        {
            return Err(
                "error[vm_websocket_session]: TCP stream is already bound to a WebSocket session"
                    .to_string(),
            );
        }
        Ok(self.open_session(stream))
    }

    /// Accepts a WebSocket upgrade onto a live VM TCP stream.
    #[cfg(test)]
    pub(crate) fn accept_upgrade(
        &mut self,
        tcp: &VmTcpRuntime,
        stream: VmTcpStream,
        endpoint: &VmWebSocketEndpointPlan,
        sec_websocket_key: &str,
    ) -> Result<VmWebSocketAcceptedUpgrade, String> {
        let response = build_websocket_upgrade_response(sec_websocket_key)?;
        let session = self.open_session_checked(tcp, stream)?;
        Ok(VmWebSocketAcceptedUpgrade {
            session,
            response,
            endpoint: endpoint.clone(),
        })
    }

    /// Sends the accepted WebSocket upgrade response over its bound VM stream.
    #[cfg(test)]
    pub(crate) fn send_upgrade_response(
        &self,
        tcp: &mut VmTcpRuntime,
        accepted: &VmWebSocketAcceptedUpgrade,
    ) -> Result<usize, String> {
        let session = self.session(accepted.session)?;
        session.ensure_open()?;
        let response = serialize_websocket_upgrade_response(&accepted.response)?;
        tcp.send(session.stream, response).map_err(|error| {
            format!("error[vm_websocket_tcp]: failed to send upgrade response: {error}")
        })
    }

    /// Sends the accepted WebSocket upgrade response over a VM TLS stream.
    #[cfg(test)]
    pub(crate) fn send_tls_upgrade_response(
        &self,
        tcp: &mut VmTcpRuntime,
        tls_stream: &mut VmTlsTcpServerStream,
        accepted: &VmWebSocketAcceptedUpgrade,
    ) -> Result<usize, String> {
        let session = self.session(accepted.session)?;
        session.ensure_open()?;
        if tls_stream.stream() != session.stream {
            return Err(
                "error[vm_websocket_tls]: TLS stream does not match WebSocket session".to_string(),
            );
        }
        let response = serialize_websocket_upgrade_response(&accepted.response)?;
        tls_stream.write_plaintext(tcp, &response).map_err(|error| {
            format!("error[vm_websocket_tls]: failed to send upgrade response: {error}")
        })
    }

    /// Returns the number of tracked WebSocket sessions.
    #[cfg(test)]
    pub(crate) fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Returns aggregate inspectable WebSocket runtime state.
    #[cfg(test)]
    pub(crate) fn inspect(&self) -> VmWebSocketRuntimeInfo {
        let mut info = VmWebSocketRuntimeInfo {
            session_count: self.sessions.len(),
            open_sessions: 0,
            closed_sessions: 0,
            frames_sent: 0,
            frames_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
        };
        for session in self.sessions.values() {
            if session.open {
                info.open_sessions = info.open_sessions.saturating_add(1);
            } else {
                info.closed_sessions = info.closed_sessions.saturating_add(1);
            }
            info.frames_sent = info.frames_sent.saturating_add(session.frames_sent);
            info.frames_received = info.frames_received.saturating_add(session.frames_received);
            info.bytes_sent = info.bytes_sent.saturating_add(session.bytes_sent);
            info.bytes_received = info.bytes_received.saturating_add(session.bytes_received);
        }
        info
    }

    /// Returns deterministic per-session inspection snapshots.
    #[cfg(test)]
    pub(crate) fn inspect_sessions(&self) -> Vec<(VmWebSocketSessionId, VmWebSocketSessionInfo)> {
        let mut sessions = self
            .sessions
            .iter()
            .map(|(id, session)| (VmWebSocketSessionId { id: *id }, session.inspect()))
            .collect::<Vec<_>>();
        sessions.sort_by_key(|(session, _)| session.id);
        sessions
    }

    /// Returns inspectable state for a WebSocket session.
    #[cfg(test)]
    pub(crate) fn inspect_session(
        &self,
        session: VmWebSocketSessionId,
    ) -> Result<VmWebSocketSessionInfo, String> {
        Ok(self.session(session)?.inspect())
    }

    /// Returns the deterministic WebSocket session bound to a VM TCP stream.
    #[cfg(test)]
    pub(crate) fn session_for_stream(&self, stream: VmTcpStream) -> Option<VmWebSocketSessionId> {
        self.sessions
            .iter()
            .filter_map(|(id, session)| {
                (session.stream == stream).then_some(VmWebSocketSessionId { id: *id })
            })
            .min_by_key(|session| session.id)
    }

    /// Returns deterministic handles for currently open WebSocket sessions.
    #[cfg(test)]
    pub(crate) fn open_sessions(&self) -> Vec<VmWebSocketSessionId> {
        let mut sessions = self
            .sessions
            .iter()
            .filter_map(|(id, session)| session.open.then_some(VmWebSocketSessionId { id: *id }))
            .collect::<Vec<_>>();
        sessions.sort_by_key(|session| session.id);
        sessions
    }

    /// Sends one text or control frame through a registered session.
    #[cfg(test)]
    pub(crate) fn send_frame(
        &mut self,
        tcp: &mut VmTcpRuntime,
        session: VmWebSocketSessionId,
        frame: VmWebSocketFrame,
    ) -> Result<usize, String> {
        self.session_mut(session)?.send_frame(tcp, frame)
    }

    /// Sends one frame to a deterministic set of WebSocket sessions.
    #[cfg(test)]
    pub(crate) fn send_frame_to_sessions(
        &mut self,
        tcp: &mut VmTcpRuntime,
        sessions: &[VmWebSocketSessionId],
        frame: VmWebSocketFrame,
    ) -> Result<Vec<(VmWebSocketSessionId, usize)>, String> {
        self.validate_open_session_set(sessions, "send")?;

        let mut sent = Vec::with_capacity(sessions.len());
        for session in sessions {
            let bytes = self.session_mut(*session)?.send_frame(tcp, frame.clone())?;
            sent.push((*session, bytes));
        }
        Ok(sent)
    }

    /// Sends one frame independently to selected sessions and reports outcomes.
    #[cfg(test)]
    pub(crate) fn send_frame_to_sessions_best_effort(
        &mut self,
        tcp: &mut VmTcpRuntime,
        sessions: &[VmWebSocketSessionId],
        frame: VmWebSocketFrame,
    ) -> Vec<VmWebSocketSendOutcome> {
        let mut seen = Vec::with_capacity(sessions.len());
        let mut outcomes = Vec::with_capacity(sessions.len());
        for session in sessions {
            if seen.contains(&session.id) {
                outcomes.push(VmWebSocketSendOutcome {
                    session: *session,
                    result: Err(
                        "error[vm_websocket_session]: duplicate session handle in send set"
                            .to_string(),
                    ),
                });
                continue;
            }
            seen.push(session.id);
            outcomes.push(VmWebSocketSendOutcome {
                session: *session,
                result: self.send_frame(tcp, *session, frame.clone()),
            });
        }
        outcomes
    }

    /// Sends one frame to every currently open WebSocket session.
    #[cfg(test)]
    pub(crate) fn send_frame_to_all_open_sessions(
        &mut self,
        tcp: &mut VmTcpRuntime,
        frame: VmWebSocketFrame,
    ) -> Result<Vec<(VmWebSocketSessionId, usize)>, String> {
        let sessions = self.open_sessions();
        self.send_frame_to_sessions(tcp, &sessions, frame)
    }

    /// Receives one text or control frame through a registered session.
    #[cfg(test)]
    pub(crate) fn receive_frame(
        &mut self,
        tcp: &mut VmTcpRuntime,
        session: VmWebSocketSessionId,
        max_bytes: usize,
    ) -> Result<Option<VmWebSocketFrame>, String> {
        self.session_mut(session)?.receive_frame(tcp, max_bytes)
    }

    /// Receives the first available frame from a deterministic session set.
    #[cfg(test)]
    pub(crate) fn receive_frame_from_sessions(
        &mut self,
        tcp: &mut VmTcpRuntime,
        sessions: &[VmWebSocketSessionId],
        max_bytes: usize,
    ) -> Result<Option<(VmWebSocketSessionId, VmWebSocketFrame)>, String> {
        self.validate_open_session_set(sessions, "receive")?;
        for session in sessions {
            if let Some(frame) = self.session_mut(*session)?.receive_frame(tcp, max_bytes)? {
                return Ok(Some((*session, frame)));
            }
        }
        Ok(None)
    }

    /// Receives the first available frame from all currently open sessions.
    #[cfg(test)]
    pub(crate) fn receive_frame_from_all_open_sessions(
        &mut self,
        tcp: &mut VmTcpRuntime,
        max_bytes: usize,
    ) -> Result<Option<(VmWebSocketSessionId, VmWebSocketFrame)>, String> {
        let sessions = self.open_sessions();
        self.receive_frame_from_sessions(tcp, &sessions, max_bytes)
    }

    /// Receives the first available frame from a session set with auto-pong.
    #[cfg(test)]
    pub(crate) fn receive_frame_from_sessions_with_auto_pong(
        &mut self,
        tcp: &mut VmTcpRuntime,
        sessions: &[VmWebSocketSessionId],
        max_bytes: usize,
    ) -> Result<Option<(VmWebSocketSessionId, VmWebSocketFrame)>, String> {
        self.validate_open_session_set(sessions, "receive")?;
        for session in sessions {
            if let Some(frame) = self
                .session_mut(*session)?
                .receive_frame_with_auto_pong(tcp, max_bytes)?
            {
                return Ok(Some((*session, frame)));
            }
        }
        Ok(None)
    }

    /// Receives the first available frame from all open sessions with auto-pong.
    #[cfg(test)]
    pub(crate) fn receive_frame_from_all_open_sessions_with_auto_pong(
        &mut self,
        tcp: &mut VmTcpRuntime,
        max_bytes: usize,
    ) -> Result<Option<(VmWebSocketSessionId, VmWebSocketFrame)>, String> {
        let sessions = self.open_sessions();
        self.receive_frame_from_sessions_with_auto_pong(tcp, &sessions, max_bytes)
    }

    /// Receives one frame and automatically answers ping control frames.
    #[cfg(test)]
    pub(crate) fn receive_frame_with_auto_pong(
        &mut self,
        tcp: &mut VmTcpRuntime,
        session: VmWebSocketSessionId,
        max_bytes: usize,
    ) -> Result<Option<VmWebSocketFrame>, String> {
        self.session_mut(session)?
            .receive_frame_with_auto_pong(tcp, max_bytes)
    }

    /// Removes a WebSocket session from the registry and returns final state.
    #[cfg(test)]
    pub(crate) fn remove_session(
        &mut self,
        session: VmWebSocketSessionId,
    ) -> Result<VmWebSocketSessionInfo, String> {
        self.sessions
            .remove(&session.id)
            .map(|session| session.inspect())
            .ok_or_else(|| "VM WebSocket session handle is unknown".to_string())
    }

    /// Removes the WebSocket session bound to a VM TCP stream without writing.
    #[cfg(test)]
    pub(crate) fn remove_session_for_stream(
        &mut self,
        stream: VmTcpStream,
    ) -> Option<(VmWebSocketSessionId, VmWebSocketSessionInfo)> {
        let session = self.session_for_stream(stream)?;
        let info = self.sessions.remove(&session.id)?.inspect();
        Some((session, info))
    }

    /// Removes every closed WebSocket session and returns final states.
    #[cfg(test)]
    pub(crate) fn remove_closed_sessions(&mut self) -> Vec<VmWebSocketSessionInfo> {
        let closed = self
            .sessions
            .iter()
            .filter_map(|(id, session)| (!session.open).then_some(*id))
            .collect::<Vec<_>>();
        closed
            .into_iter()
            .filter_map(|id| self.sessions.remove(&id))
            .map(|session| session.inspect())
            .collect()
    }

    /// Removes WebSocket sessions whose VM TCP stream is closed or cancelled.
    #[cfg(test)]
    pub(crate) fn remove_inactive_stream_sessions(
        &mut self,
        tcp: &VmTcpRuntime,
    ) -> Result<Vec<(VmWebSocketSessionId, VmWebSocketSessionInfo)>, String> {
        let mut sessions = self
            .sessions
            .iter()
            .map(|(id, session)| (VmWebSocketSessionId { id: *id }, session.stream))
            .collect::<Vec<_>>();
        sessions.sort_by_key(|(session, _)| session.id);

        let mut inactive = Vec::new();
        for (session, stream) in sessions {
            let stream_info = tcp.inspect_stream(stream).map_err(|error| {
                format!("error[vm_websocket_tcp]: failed to inspect session stream: {error}")
            })?;
            if stream_info.closed || stream_info.cancelled {
                inactive.push(session);
            }
        }

        let removed = inactive
            .into_iter()
            .filter_map(|session| {
                self.sessions
                    .remove(&session.id)
                    .map(|state| (session, state.inspect()))
            })
            .collect();
        Ok(removed)
    }

    /// Removes closed WebSocket sessions and closes their VM TCP streams.
    #[cfg(test)]
    pub(crate) fn remove_closed_sessions_and_close_streams(
        &mut self,
        tcp: &mut VmTcpRuntime,
    ) -> Result<Vec<VmWebSocketSessionInfo>, String> {
        let removed = self.remove_closed_sessions();
        for session in &removed {
            tcp.close_stream(session.stream).map_err(|error| {
                format!("error[vm_websocket_tcp]: failed to close session stream: {error}")
            })?;
        }
        Ok(removed)
    }

    /// Closes, removes, and releases the VM TCP stream for one session.
    #[cfg(test)]
    pub(crate) fn close_session_and_stream(
        &mut self,
        tcp: &mut VmTcpRuntime,
        session: VmWebSocketSessionId,
    ) -> Result<VmWebSocketSessionInfo, String> {
        {
            let websocket = self.session_mut(session)?;
            if websocket.open {
                let close_frame = VmWebSocketFrame::Control(VmWebSocketControlFrame::Close);
                websocket.send_frame(tcp, close_frame)?;
            }
        }
        let info = self.remove_session(session)?;
        tcp.close_stream(info.stream).map_err(|error| {
            format!("error[vm_websocket_tcp]: failed to close session stream: {error}")
        })?;
        Ok(info)
    }

    /// Terminates one session for a scheduler timeout or cancellation.
    #[cfg(test)]
    pub(crate) fn terminate_session_and_stream(
        &mut self,
        tcp: &mut VmTcpRuntime,
        session: VmWebSocketSessionId,
        reason: VmWebSocketTerminationReason,
    ) -> Result<VmWebSocketTermination, String> {
        let info = match reason {
            VmWebSocketTerminationReason::Timeout => self.close_session_and_stream(tcp, session)?,
            VmWebSocketTerminationReason::Cancelled => {
                let info = self.remove_session(session)?;
                tcp.cancel_stream(info.stream).map_err(|error| {
                    format!("error[vm_websocket_tcp]: failed to cancel session stream: {error}")
                })?;
                info
            }
        };
        Ok(VmWebSocketTermination {
            session,
            reason,
            info,
        })
    }

    /// Closes, removes, and releases the WebSocket session for a VM TCP stream.
    #[cfg(test)]
    pub(crate) fn close_stream_session_and_stream(
        &mut self,
        tcp: &mut VmTcpRuntime,
        stream: VmTcpStream,
    ) -> Result<Option<(VmWebSocketSessionId, VmWebSocketSessionInfo)>, String> {
        let Some(session) = self.session_for_stream(stream) else {
            return Ok(None);
        };
        let info = self.close_session_and_stream(tcp, session)?;
        Ok(Some((session, info)))
    }

    /// Closes, removes, and releases VM TCP streams for selected sessions.
    #[cfg(test)]
    pub(crate) fn close_sessions_and_streams(
        &mut self,
        tcp: &mut VmTcpRuntime,
        sessions: &[VmWebSocketSessionId],
    ) -> Result<Vec<(VmWebSocketSessionId, VmWebSocketSessionInfo)>, String> {
        self.validate_known_session_set(sessions, "close")?;
        let mut closed = Vec::with_capacity(sessions.len());
        for session in sessions {
            let info = self.close_session_and_stream(tcp, *session)?;
            closed.push((*session, info));
        }
        Ok(closed)
    }

    /// Closes selected sessions independently and reports every outcome.
    #[cfg(test)]
    pub(crate) fn close_sessions_and_streams_best_effort(
        &mut self,
        tcp: &mut VmTcpRuntime,
        sessions: &[VmWebSocketSessionId],
    ) -> Vec<VmWebSocketCloseOutcome> {
        let mut seen = Vec::with_capacity(sessions.len());
        let mut outcomes = Vec::with_capacity(sessions.len());
        for session in sessions {
            if seen.contains(&session.id) {
                outcomes.push(VmWebSocketCloseOutcome {
                    session: *session,
                    result: Err(
                        "error[vm_websocket_session]: duplicate session handle in close set"
                            .to_string(),
                    ),
                });
                continue;
            }
            seen.push(session.id);
            outcomes.push(VmWebSocketCloseOutcome {
                session: *session,
                result: self.close_session_and_stream(tcp, *session),
            });
        }
        outcomes
    }

    /// Closes, removes, and releases every tracked WebSocket session.
    #[cfg(test)]
    pub(crate) fn close_all_sessions_and_streams(
        &mut self,
        tcp: &mut VmTcpRuntime,
    ) -> Result<Vec<(VmWebSocketSessionId, VmWebSocketSessionInfo)>, String> {
        let mut sessions = self
            .sessions
            .keys()
            .map(|id| VmWebSocketSessionId { id: *id })
            .collect::<Vec<_>>();
        sessions.sort_by_key(|session| session.id);
        self.close_sessions_and_streams(tcp, &sessions)
    }

    #[cfg(test)]
    fn session(&self, session: VmWebSocketSessionId) -> Result<&VmWebSocketSession, String> {
        self.sessions
            .get(&session.id)
            .ok_or_else(|| "VM WebSocket session handle is unknown".to_string())
    }

    #[cfg(test)]
    fn session_mut(
        &mut self,
        session: VmWebSocketSessionId,
    ) -> Result<&mut VmWebSocketSession, String> {
        self.sessions
            .get_mut(&session.id)
            .ok_or_else(|| "VM WebSocket session handle is unknown".to_string())
    }

    #[cfg(test)]
    fn validate_open_session_set(
        &self,
        sessions: &[VmWebSocketSessionId],
        operation: &str,
    ) -> Result<(), String> {
        self.validate_known_session_set(sessions, operation)?;
        for session in sessions {
            self.session(*session)?.ensure_open()?;
        }
        Ok(())
    }

    #[cfg(test)]
    fn validate_known_session_set(
        &self,
        sessions: &[VmWebSocketSessionId],
        operation: &str,
    ) -> Result<(), String> {
        let mut seen = Vec::with_capacity(sessions.len());
        for session in sessions {
            if seen.contains(&session.id) {
                return Err(format!(
                    "error[vm_websocket_session]: duplicate session handle in {operation} set"
                ));
            }
            seen.push(session.id);
            self.session(*session)?;
        }
        Ok(())
    }
}

/// VM-owned WebSocket session bound to one VM TCP stream.
///
/// Inputs:
/// - Accepted VM TCP stream after a successful WebSocket opening handshake.
///
/// Output:
/// - Stateful frame send/receive boundary for higher-level VM actors.
///
/// Transformation:
/// - Keeps lifecycle and counters in the VM while delegating protocol frame
///   correctness to maintained tungstenite helpers.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(in crate::runtime::vm::websocket) struct VmWebSocketSession {
    stream: VmTcpStream,
    pub(in crate::runtime::vm::websocket) open: bool,
    frames_sent: usize,
    frames_received: usize,
    bytes_sent: usize,
    bytes_received: usize,
}

#[cfg(test)]
impl VmWebSocketSession {
    /// Creates an open WebSocket session for an accepted VM TCP stream.
    pub(crate) fn new(stream: VmTcpStream) -> Self {
        Self {
            stream,
            open: true,
            frames_sent: 0,
            frames_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
        }
    }

    /// Returns inspectable lifecycle and traffic counters.
    pub(crate) fn inspect(&self) -> VmWebSocketSessionInfo {
        VmWebSocketSessionInfo {
            stream: self.stream,
            open: self.open,
            frames_sent: self.frames_sent,
            frames_received: self.frames_received,
            bytes_sent: self.bytes_sent,
            bytes_received: self.bytes_received,
        }
    }

    /// Sends one text frame through the session stream.
    pub(crate) fn send_text(
        &mut self,
        tcp: &mut VmTcpRuntime,
        text: &str,
    ) -> Result<usize, String> {
        self.ensure_open()?;
        let frame = encode_server_text_frame(text)?;
        let written = tcp.send(self.stream, frame).map_err(|error| {
            format!("error[vm_websocket_tcp]: failed to send text frame: {error}")
        })?;
        self.frames_sent = self.frames_sent.saturating_add(1);
        self.bytes_sent = self.bytes_sent.saturating_add(written);
        Ok(written)
    }

    /// Receives one text frame from the session stream.
    pub(crate) fn receive_text(
        &mut self,
        tcp: &mut VmTcpRuntime,
        max_bytes: usize,
    ) -> Result<Option<String>, String> {
        self.ensure_open()?;
        let Some(frame) = tcp.receive(self.stream, max_bytes).map_err(|error| {
            format!("error[vm_websocket_tcp]: failed to receive text frame: {error}")
        })?
        else {
            return Ok(None);
        };
        let bytes = frame.len();
        let text = decode_client_text_frame(&frame)?;
        self.frames_received = self.frames_received.saturating_add(1);
        self.bytes_received = self.bytes_received.saturating_add(bytes);
        Ok(Some(text))
    }

    /// Sends one control frame through the session stream.
    pub(crate) fn send_control(
        &mut self,
        tcp: &mut VmTcpRuntime,
        frame: VmWebSocketControlFrame,
    ) -> Result<usize, String> {
        self.ensure_open()?;
        let closes = matches!(frame, VmWebSocketControlFrame::Close);
        let bytes = encode_server_control_frame(frame)?;
        let written = tcp.send(self.stream, bytes).map_err(|error| {
            format!("error[vm_websocket_tcp]: failed to send control frame: {error}")
        })?;
        self.frames_sent = self.frames_sent.saturating_add(1);
        self.bytes_sent = self.bytes_sent.saturating_add(written);
        if closes {
            self.open = false;
        }
        Ok(written)
    }

    /// Sends one text or control frame through the session stream.
    pub(crate) fn send_frame(
        &mut self,
        tcp: &mut VmTcpRuntime,
        frame: VmWebSocketFrame,
    ) -> Result<usize, String> {
        match frame {
            VmWebSocketFrame::Text(text) => self.send_text(tcp, &text),
            VmWebSocketFrame::Control(control) => self.send_control(tcp, control),
        }
    }

    /// Receives one control frame from the session stream.
    pub(crate) fn receive_control(
        &mut self,
        tcp: &mut VmTcpRuntime,
        max_bytes: usize,
    ) -> Result<Option<VmWebSocketControlFrame>, String> {
        self.ensure_open()?;
        let Some(frame) = tcp.receive(self.stream, max_bytes).map_err(|error| {
            format!("error[vm_websocket_tcp]: failed to receive control frame: {error}")
        })?
        else {
            return Ok(None);
        };
        let bytes = frame.len();
        let control = decode_client_control_frame(&frame)?;
        self.frames_received = self.frames_received.saturating_add(1);
        self.bytes_received = self.bytes_received.saturating_add(bytes);
        self.open = !matches!(control, VmWebSocketControlFrame::Close);
        Ok(Some(control))
    }

    /// Receives the next text or control frame from the session stream.
    pub(crate) fn receive_frame(
        &mut self,
        tcp: &mut VmTcpRuntime,
        max_bytes: usize,
    ) -> Result<Option<VmWebSocketFrame>, String> {
        self.ensure_open()?;
        let Some(frame) = tcp.receive(self.stream, max_bytes).map_err(|error| {
            format!("error[vm_websocket_tcp]: failed to receive frame: {error}")
        })?
        else {
            return Ok(None);
        };
        let bytes = frame.len();
        let event = decode_client_frame(&frame)?;
        self.frames_received = self.frames_received.saturating_add(1);
        self.bytes_received = self.bytes_received.saturating_add(bytes);
        if matches!(
            event,
            VmWebSocketFrame::Control(VmWebSocketControlFrame::Close)
        ) {
            self.open = false;
        }
        Ok(Some(event))
    }

    /// Receives one frame and automatically answers ping control frames.
    pub(crate) fn receive_frame_with_auto_pong(
        &mut self,
        tcp: &mut VmTcpRuntime,
        max_bytes: usize,
    ) -> Result<Option<VmWebSocketFrame>, String> {
        let event = self.receive_frame(tcp, max_bytes)?;
        if let Some(VmWebSocketFrame::Control(VmWebSocketControlFrame::Ping(payload))) = &event {
            let pong_frame =
                VmWebSocketFrame::Control(VmWebSocketControlFrame::Pong(payload.clone()));
            self.send_frame(tcp, pong_frame)?;
        }
        Ok(event)
    }

    fn ensure_open(&self) -> Result<(), String> {
        if self.open {
            Ok(())
        } else {
            Err("error[vm_websocket_session]: session is closed".to_string())
        }
    }
}

/// Builds the VM-owned WebSocket opening-handshake response.
///
/// Inputs:
/// - `sec_websocket_key`: value of the validated `Sec-WebSocket-Key` request
///   header.
///
/// Output:
/// - Protocol-switch metadata containing the maintained tungstenite-derived
///   accept key, or a stable diagnostic for blank input.
///
/// Transformation:
/// - Delegates the accept-key algorithm to tungstenite and returns typed
///   metadata that HTTP/TCP scheduling can later serialize over VM streams.
pub(crate) fn build_websocket_upgrade_response(
    sec_websocket_key: &str,
) -> Result<VmWebSocketUpgradeResponse, String> {
    let key = sec_websocket_key.trim();
    if key.is_empty() {
        return Err("error[vm_websocket]: missing Sec-WebSocket-Key".to_string());
    }

    Ok(VmWebSocketUpgradeResponse {
        status: 101,
        headers: vec![
            ("upgrade".to_string(), "websocket".to_string()),
            ("connection".to_string(), "Upgrade".to_string()),
            (
                "sec-websocket-accept".to_string(),
                derive_accept_key(key.as_bytes()),
            ),
        ],
    })
}

/// Serializes VM-owned WebSocket upgrade response metadata into HTTP/1 bytes.
#[cfg(test)]
pub(crate) fn serialize_websocket_upgrade_response(
    response: &VmWebSocketUpgradeResponse,
) -> Result<Vec<u8>, String> {
    let status = ::http::StatusCode::from_u16(response.status)
        .map_err(|error| format!("error[vm_websocket_upgrade]: invalid status: {error}"))?;
    if status != ::http::StatusCode::SWITCHING_PROTOCOLS {
        return Err("error[vm_websocket_upgrade]: response status must be 101".to_string());
    }
    let reason = status.canonical_reason().unwrap_or("");
    let mut bytes = Vec::with_capacity(64 + response.headers.len() * 48);
    write!(&mut bytes, "HTTP/1.1 {} {}\r\n", status.as_u16(), reason)
        .map_err(|error| format!("error[vm_websocket_upgrade]: failed to write status: {error}"))?;

    for (name, value) in &response.headers {
        let header_name = ::http::HeaderName::from_bytes(name.as_bytes()).map_err(|error| {
            format!("error[vm_websocket_upgrade]: invalid header `{name}`: {error}")
        })?;
        let header_value = ::http::HeaderValue::from_str(value).map_err(|error| {
            format!("error[vm_websocket_upgrade]: invalid header `{name}` value: {error}")
        })?;
        write!(
            &mut bytes,
            "{}: {}\r\n",
            header_name.as_str(),
            header_value.to_str().map_err(|error| format!(
                "error[vm_websocket_upgrade]: non-text header `{name}` value: {error}"
            ))?
        )
        .map_err(|error| {
            format!("error[vm_websocket_upgrade]: failed to write header `{name}`: {error}")
        })?;
    }
    bytes.extend_from_slice(b"\r\n");
    Ok(bytes)
}

/// Builds one Rust-backed WebSocket text frame value for `std.http.WebSocket.text`.
#[cfg(test)]
pub fn text(value: String) -> VmWebSocketFrame {
    VmWebSocketFrame::Text(value)
}

/// Builds one Rust-backed WebSocket ping frame value for `std.http.WebSocket.ping`.
#[cfg(test)]
pub fn ping(value: String) -> VmWebSocketFrame {
    VmWebSocketFrame::Control(VmWebSocketControlFrame::Ping(value.into_bytes()))
}

/// Builds one Rust-backed WebSocket pong frame value for `std.http.WebSocket.pong`.
#[cfg(test)]
pub fn pong(value: String) -> VmWebSocketFrame {
    VmWebSocketFrame::Control(VmWebSocketControlFrame::Pong(value.into_bytes()))
}

/// Builds one Rust-backed WebSocket close frame value for `std.http.WebSocket.close`.
#[cfg(test)]
pub fn close() -> VmWebSocketFrame {
    VmWebSocketFrame::Control(VmWebSocketControlFrame::Close)
}

/// Builds one Rust-backed WebSocket endpoint plan for `std.http.WebSocket.endpoint`.
#[cfg(test)]
pub fn endpoint(
    max_pending_frames: usize,
    max_frame_bytes: usize,
) -> Result<VmWebSocketEndpointPlan, terlan_runtime_abi::BoundaryError> {
    VmWebSocketEndpointPlan::new(max_pending_frames, max_frame_bytes).map_err(|error| {
        terlan_runtime_abi::BoundaryError::message(
            terlan_runtime_abi::ErrorDomain::VmRuntime,
            "construct WebSocket endpoint",
            error,
        )
    })
}
