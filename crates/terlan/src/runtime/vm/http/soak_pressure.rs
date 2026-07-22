use super::{
    drain_client, soak_response, VmHttpSoakRuntime, VmHttpSoakTerminal, SOAK_ADDRESS,
    SOAK_BACKLOG_LIMIT,
};
use crate::runtime::vm::process::VmExitReason;

const DISCONNECT_STORM_CLIENTS: usize = 8;

impl VmHttpSoakRuntime {
    pub(super) fn disconnect_storm(&mut self) -> Result<(), String> {
        let mut clients = Vec::with_capacity(DISCONNECT_STORM_CLIENTS);
        for index in 0..DISCONNECT_STORM_CLIENTS {
            let client = self
                .tcp
                .connect(SOAK_ADDRESS, format!("disconnect-storm:{index}"))?;
            self.tcp.close_write(client)?;
            clients.push(client);
        }
        self.record_accept_queue_high_water();

        let tick = self.next_tick();
        let error = self
            .server
            .poll_keep_alive_with_deadlines(
                &mut self.processes,
                &mut self.tcp,
                &mut self.timers,
                &mut self.scheduler,
                tick,
                |_request| Err("disconnected request reached handler".to_string()),
            )
            .expect_err("disconnect storm must reject incomplete requests");
        if error != "VM HTTP request closed before headers completed" {
            return Err(format!(
                "VM HTTP disconnect storm diagnostic mismatch: `{error}`"
            ));
        }

        let handlers = self
            .server
            .handlers
            .iter()
            .map(|handler| handler.process)
            .collect::<Vec<_>>();
        if handlers.len() != DISCONNECT_STORM_CLIENTS {
            return Err(format!(
                "VM HTTP disconnect storm accepted {} handlers instead of {DISCONNECT_STORM_CLIENTS}",
                handlers.len()
            ));
        }
        for (index, process) in handlers.into_iter().enumerate() {
            self.server
                .cancel_handler(
                    &mut self.processes,
                    &mut self.tcp,
                    process,
                    VmExitReason::Error(error.clone()),
                )?
                .ok_or_else(|| {
                    format!(
                        "VM HTTP disconnect handler {} was not active",
                        process.as_u64()
                    )
                })?;
            self.counters.terminals.push(VmHttpSoakTerminal {
                phase: "client-disconnect-storm",
                outcome: "disconnected",
                diagnostic: format!(
                    "client={index} process={} terminal=peer_write_closed shutdown_phase=adversarial_replay",
                    process.as_u64()
                ),
            });
        }
        for client in clients {
            self.tcp.close_stream(client)?;
        }
        self.counters.expected_failures += DISCONNECT_STORM_CLIENTS;
        self.counters.disconnected_clients += DISCONNECT_STORM_CLIENTS;
        Ok(())
    }

    pub(super) fn saturate_accept_queue(&mut self) -> Result<(), String> {
        let request = format!(
            "GET /static HTTP/1.1\r\nHost: {SOAK_ADDRESS}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
        );
        let mut clients = Vec::with_capacity(SOAK_BACKLOG_LIMIT);
        for index in 0..SOAK_BACKLOG_LIMIT {
            let client = self
                .tcp
                .connect(SOAK_ADDRESS, format!("saturation:{index}"))?;
            self.tcp.send(client, request.as_bytes().to_vec())?;
            clients.push(client);
        }
        self.record_accept_queue_high_water();
        let error = self
            .tcp
            .connect(SOAK_ADDRESS, "saturation:rejected")
            .expect_err("full VM HTTP accept queue must reject a connection");
        let expected = format!("VM TCP listener `{SOAK_ADDRESS}` backlog is full");
        if error != expected {
            return Err(format!(
                "VM HTTP accept saturation diagnostic mismatch: expected `{expected}`, observed `{error}`"
            ));
        }

        let tick = self.next_tick();
        let poll = self.server.poll_keep_alive_with_deadlines(
            &mut self.processes,
            &mut self.tcp,
            &mut self.timers,
            &mut self.scheduler,
            tick,
            |request| soak_response(request, &mut self.stateful_counter),
        )?;
        self.record_poll(&poll);
        if poll.http.accepted != SOAK_BACKLOG_LIMIT || poll.http.completed != SOAK_BACKLOG_LIMIT {
            return Err(format!(
                "VM HTTP accept saturation drained accepted={} completed={} instead of {SOAK_BACKLOG_LIMIT}",
                poll.http.accepted, poll.http.completed
            ));
        }
        self.record_response_memory();
        for client in clients {
            drain_client(&mut self.tcp, client)?;
            self.tcp.close_stream(client)?;
        }
        self.counters.expected_failures += 1;
        self.counters.backpressure_rejections += 1;
        self.counters.terminals.push(VmHttpSoakTerminal {
            phase: "accept-queue-saturation",
            outcome: "backpressured",
            diagnostic: format!(
                "limit={SOAK_BACKLOG_LIMIT} queued={SOAK_BACKLOG_LIMIT} terminal=backpressure_rejected shutdown_phase=adversarial_replay"
            ),
        });
        Ok(())
    }

    fn record_accept_queue_high_water(&mut self) {
        self.counters.accept_queue_high_water = self
            .counters
            .accept_queue_high_water
            .max(self.tcp.metrics().queued_accepts);
    }
}
