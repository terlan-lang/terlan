use super::*;

#[cfg(test)]
#[derive(Clone, Copy)]
struct VmHttpServerPollPolicy {
    close_completed_connections: bool,
    accept_limit: usize,
    handler_poll_limit: usize,
}

impl VmHttpTcpServer {
    /// Creates server state for a VM TCP listener.
    pub(crate) fn new(listener: VmTcpListener, handler_source: VmProcessSource) -> Self {
        Self {
            listener,
            handler_source,
            overload: None,
            handlers: Vec::new(),
            next_handler_index: 0,
            accepted_total: 0,
            rejected_total: 0,
            spilled_total: 0,
            completed_total: 0,
            response_memory: VmHttpResponseMemory::with_default_limits(),
            request_resources: VmHttpRequestResourceTracker::default(),
            handler_timeout_ticks: None,
            handler_deadlines: VmHttpHandlerDeadlines::default(),
            last_completed_handlers: Vec::new(),
            lifecycle: VmHttpLifecycleState::Running,
            lifecycle_hook: None,
        }
    }

    /// Returns the listener transport mode selected for this HTTP server.
    ///
    /// Inputs:
    /// - TLS runtime containing listener-bound transport plans.
    ///
    /// Output:
    /// - Plaintext or TLS transport selection, or a stable diagnostic when no
    ///   plan is installed for this server listener.
    ///
    /// Transformation:
    /// - Keeps HTTP protocol handoff tied to the VM listener resource instead
    ///   of duplicating TLS certificate-policy matching in server code.
    #[cfg(test)]
    pub(crate) fn transport_mode(&self, tls: &VmTlsRuntime) -> Result<VmTlsTransportMode, String> {
        tls.listener_transport_mode(self.listener)
    }

    /// Returns number of active handler processes owned by this server.
    #[cfg(test)]
    pub(crate) fn active_handlers(&self) -> usize {
        self.handlers.len()
    }

    /// Returns total accepted streams for inspection and tests.
    #[cfg(test)]
    pub(crate) fn accepted_total(&self) -> usize {
        self.accepted_total
    }

    /// Returns total completed handlers for inspection and tests.
    #[cfg(test)]
    pub(crate) fn completed_total(&self) -> usize {
        self.completed_total
    }

    /// Returns transient request-resource ownership and high-water metrics.
    #[cfg(test)]
    pub(crate) fn request_resource_metrics(&self) -> VmHttpRequestResourceMetrics {
        self.request_resources.metrics()
    }

    /// Returns request resources still owned after handler cleanup.
    #[cfg(test)]
    pub(crate) fn request_resource_leaks(&self) -> Vec<VmHttpRequestResourceLeak> {
        self.request_resources.leaks()
    }

    /// Returns a VM-owned inspection snapshot for this HTTP server.
    ///
    /// Inputs:
    /// - TCP runtime that owns the listener resource.
    ///
    /// Output:
    /// - HTTP counters and TCP listener pressure, or a stable diagnostic when
    ///   the listener handle is unknown.
    ///
    /// Transformation:
    /// - Joins HTTP scheduling state with listener inspection without copying
    ///   listener internals into the HTTP server.
    #[cfg(test)]
    pub(crate) fn inspect(&self, tcp: &VmTcpRuntime) -> Result<VmHttpTcpServerInfo, String> {
        Ok(VmHttpTcpServerInfo {
            listener: tcp.inspect_listener(self.listener)?,
            overload: self.overload,
            active_handlers: self.handlers.len(),
            next_handler_index: self.next_handler_index,
            accepted_total: self.accepted_total,
            rejected_total: self.rejected_total,
            spilled_total: self.spilled_total,
            completed_total: self.completed_total,
        })
    }

    /// Polls the VM HTTP server once.
    ///
    /// Inputs:
    /// - Process table, TCP runtime, and request handler.
    ///
    /// Output:
    /// - Counts for accepted, polled, parked, completed, and skipped handlers.
    ///
    /// Transformation:
    /// - Drains currently accepted VM TCP streams into handler processes, polls
    ///   runnable handlers, parks incomplete streams, and finishes completed
    ///   handlers without touching host socket APIs.
    #[cfg(test)]
    pub(crate) fn poll(
        &mut self,
        processes: &mut VmProcessTable,
        tcp: &mut VmTcpRuntime,
        handler: impl FnMut(::http::Request<String>) -> Result<::http::Response<String>, String>,
    ) -> Result<VmHttpTcpServerPoll, String> {
        self.poll_with_policy(processes, tcp, true, usize::MAX, usize::MAX, handler)
    }

    /// Polls the VM HTTP server once with listener-bound TLS policy.
    ///
    /// Inputs:
    /// - Process table, TCP runtime, TLS runtime, and request handler.
    ///
    /// Output:
    /// - Normal HTTP poll report for plaintext listeners and TLS listeners.
    ///
    /// Transformation:
    /// - Selects the listener-bound transport once and then routes accepted
    ///   VM TCP streams through either plaintext HTTP or retained TLS stream
    ///   state before handler execution.
    #[cfg(test)]
    pub(crate) fn poll_with_tls(
        &mut self,
        processes: &mut VmProcessTable,
        tcp: &mut VmTcpRuntime,
        tls: &VmTlsRuntime,
        handler: impl FnMut(::http::Request<String>) -> Result<::http::Response<String>, String>,
    ) -> Result<VmHttpTcpServerPoll, String> {
        match self.transport_mode(tls)? {
            VmTlsTransportMode::Plaintext => self.poll(processes, tcp, handler),
            VmTlsTransportMode::Tls => self.poll_tls_with_policy(
                processes,
                tcp,
                tls,
                VmHttpServerPollPolicy {
                    close_completed_connections: true,
                    accept_limit: usize::MAX,
                    handler_poll_limit: usize::MAX,
                },
                handler,
            ),
        }
    }

    /// Polls the VM HTTP server once without closing completed connections.
    ///
    /// Inputs:
    /// - Process table, TCP runtime, and request handler.
    ///
    /// Output:
    /// - Counts for accepted, polled, parked, completed, and skipped handlers.
    ///
    /// Transformation:
    /// - Completes at most one keep-alive exchange per runnable handler and
    ///   keeps the handler stream active for subsequent pipelined requests.
    #[cfg(test)]
    pub(crate) fn poll_keep_alive(
        &mut self,
        processes: &mut VmProcessTable,
        tcp: &mut VmTcpRuntime,
        handler: impl FnMut(::http::Request<String>) -> Result<::http::Response<String>, String>,
    ) -> Result<VmHttpTcpServerPoll, String> {
        self.poll_with_policy(processes, tcp, false, usize::MAX, usize::MAX, handler)
    }

    /// Polls the VM HTTP keep-alive server once with listener-bound TLS policy.
    ///
    /// Inputs:
    /// - Process table, TCP runtime, TLS runtime, and request handler.
    ///
    /// Output:
    /// - Normal keep-alive poll report for plaintext and TLS listeners.
    ///
    /// Transformation:
    /// - Selects plaintext or retained TLS stream state while preserving
    ///   reusable connection semantics.
    #[cfg(test)]
    pub(crate) fn poll_keep_alive_with_tls(
        &mut self,
        processes: &mut VmProcessTable,
        tcp: &mut VmTcpRuntime,
        tls: &VmTlsRuntime,
        handler: impl FnMut(::http::Request<String>) -> Result<::http::Response<String>, String>,
    ) -> Result<VmHttpTcpServerPoll, String> {
        match self.transport_mode(tls)? {
            VmTlsTransportMode::Plaintext => self.poll_keep_alive(processes, tcp, handler),
            VmTlsTransportMode::Tls => self.poll_tls_with_policy(
                processes,
                tcp,
                tls,
                VmHttpServerPollPolicy {
                    close_completed_connections: false,
                    accept_limit: usize::MAX,
                    handler_poll_limit: usize::MAX,
                },
                handler,
            ),
        }
    }

    /// Polls the VM HTTP server once with keep-alive and an accept budget.
    ///
    /// Inputs:
    /// - Process table, TCP runtime, maximum accepts for this poll, and
    ///   request handler.
    ///
    /// Output:
    /// - Counts for accepted, polled, parked, completed, and skipped handlers.
    ///
    /// Transformation:
    /// - Accepts at most `accept_limit` pending streams before polling
    ///   handlers, which gives the VM scheduler a fairness control for busy
    ///   listeners.
    #[cfg(test)]
    pub(crate) fn poll_keep_alive_with_accept_limit(
        &mut self,
        processes: &mut VmProcessTable,
        tcp: &mut VmTcpRuntime,
        accept_limit: usize,
        handler: impl FnMut(::http::Request<String>) -> Result<::http::Response<String>, String>,
    ) -> Result<VmHttpTcpServerPoll, String> {
        self.poll_with_policy(processes, tcp, false, accept_limit, usize::MAX, handler)
    }

    /// Polls the VM HTTP keep-alive server with TLS policy and an accept
    /// budget.
    ///
    /// Inputs:
    /// - Process table, TCP runtime, TLS runtime, accept limit, and request
    ///   handler.
    ///
    /// Output:
    /// - Normal accept-budgeted keep-alive poll report for plaintext and TLS
    ///   listeners.
    ///
    /// Transformation:
    /// - Keeps the accept-budget convenience path under the same TLS stream
    ///   handoff as the full scheduler-budget path.
    #[cfg(test)]
    pub(crate) fn poll_keep_alive_with_tls_accept_limit(
        &mut self,
        processes: &mut VmProcessTable,
        tcp: &mut VmTcpRuntime,
        tls: &VmTlsRuntime,
        accept_limit: usize,
        handler: impl FnMut(::http::Request<String>) -> Result<::http::Response<String>, String>,
    ) -> Result<VmHttpTcpServerPoll, String> {
        match self.transport_mode(tls)? {
            VmTlsTransportMode::Plaintext => {
                self.poll_keep_alive_with_accept_limit(processes, tcp, accept_limit, handler)
            }
            VmTlsTransportMode::Tls => self.poll_tls_with_policy(
                processes,
                tcp,
                tls,
                VmHttpServerPollPolicy {
                    close_completed_connections: false,
                    accept_limit,
                    handler_poll_limit: usize::MAX,
                },
                handler,
            ),
        }
    }

    /// Polls the VM HTTP server once with keep-alive, accept, and handler
    /// budgets.
    ///
    /// Inputs:
    /// - Process table, TCP runtime, accept limit, handler poll limit, and
    ///   request handler.
    ///
    /// Output:
    /// - Counts for accepted, polled, parked, completed, and skipped handlers.
    ///
    /// Transformation:
    /// - Applies explicit scheduler budgets to both accept work and active
    ///   handler polling.
    #[cfg(test)]
    pub(crate) fn poll_keep_alive_with_limits(
        &mut self,
        processes: &mut VmProcessTable,
        tcp: &mut VmTcpRuntime,
        accept_limit: usize,
        handler_poll_limit: usize,
        handler: impl FnMut(::http::Request<String>) -> Result<::http::Response<String>, String>,
    ) -> Result<VmHttpTcpServerPoll, String> {
        self.poll_with_policy(
            processes,
            tcp,
            false,
            accept_limit,
            handler_poll_limit,
            handler,
        )
    }

    /// Polls the VM HTTP keep-alive server with TLS policy and scheduler
    /// budgets.
    ///
    /// Inputs:
    /// - Process table, TCP runtime, TLS runtime, accept limit, handler poll
    ///   limit, and request handler.
    ///
    /// Output:
    /// - Normal budgeted keep-alive poll report for plaintext and TLS
    ///   listeners.
    ///
    /// Transformation:
    /// - Applies TLS stream handoff to the same fairness-controlled path
    ///   production HTTP uses for busy keep-alive listeners.
    #[cfg(test)]
    pub(crate) fn poll_keep_alive_with_tls_limits(
        &mut self,
        processes: &mut VmProcessTable,
        tcp: &mut VmTcpRuntime,
        tls: &VmTlsRuntime,
        accept_limit: usize,
        handler_poll_limit: usize,
        handler: impl FnMut(::http::Request<String>) -> Result<::http::Response<String>, String>,
    ) -> Result<VmHttpTcpServerPoll, String> {
        match self.transport_mode(tls)? {
            VmTlsTransportMode::Plaintext => self.poll_with_policy(
                processes,
                tcp,
                false,
                accept_limit,
                handler_poll_limit,
                handler,
            ),
            VmTlsTransportMode::Tls => self.poll_tls_with_policy(
                processes,
                tcp,
                tls,
                VmHttpServerPollPolicy {
                    close_completed_connections: false,
                    accept_limit,
                    handler_poll_limit,
                },
                handler,
            ),
        }
    }

    /// Polls the VM HTTP server with an explicit connection lifecycle policy.
    #[cfg(test)]
    fn poll_with_policy(
        &mut self,
        processes: &mut VmProcessTable,
        tcp: &mut VmTcpRuntime,
        close_completed_connections: bool,
        accept_limit: usize,
        handler_poll_limit: usize,
        handler: impl FnMut(::http::Request<String>) -> Result<::http::Response<String>, String>,
    ) -> Result<VmHttpTcpServerPoll, String> {
        if accept_limit == 0 {
            return Err("VM HTTP server accept limit must be greater than 0".to_string());
        }
        self.poll_with_policy_internal(
            processes,
            tcp,
            close_completed_connections,
            accept_limit,
            handler_poll_limit,
            handler,
        )
    }

    /// Polls retained plaintext handlers without accepting new connections.
    #[cfg(test)]
    pub(super) fn poll_retained_handlers(
        &mut self,
        processes: &mut VmProcessTable,
        tcp: &mut VmTcpRuntime,
        handler_poll_limit: usize,
        handler: impl FnMut(::http::Request<String>) -> Result<::http::Response<String>, String>,
    ) -> Result<VmHttpTcpServerPoll, String> {
        self.poll_with_policy_internal(processes, tcp, true, 0, handler_poll_limit, handler)
    }

    /// Polls plaintext HTTP work with an internal accept budget.
    #[cfg(test)]
    fn poll_with_policy_internal(
        &mut self,
        processes: &mut VmProcessTable,
        tcp: &mut VmTcpRuntime,
        close_completed_connections: bool,
        accept_limit: usize,
        handler_poll_limit: usize,
        mut handler: impl FnMut(::http::Request<String>) -> Result<::http::Response<String>, String>,
    ) -> Result<VmHttpTcpServerPoll, String> {
        self.last_completed_handlers.clear();
        if handler_poll_limit == 0 {
            return Err("VM HTTP server handler poll limit must be greater than 0".to_string());
        }
        let mut report = VmHttpTcpServerPoll::default();
        while report.accepted < accept_limit {
            if self.saturated_overload_policy() == Some(VmHttpOverloadPolicy::Queue) {
                break;
            }
            let Some(handler_state) = accept_http1_tcp_handler(
                processes,
                tcp,
                self.listener,
                self.handler_source.clone(),
            )?
            else {
                break;
            };
            self.admit_handler(processes, tcp, handler_state, &mut report)?;
        }

        if self.handlers.is_empty() {
            self.next_handler_index = 0;
            return Ok(report);
        }

        let handler_visit_limit = self.handlers.len();
        let mut index = self.next_handler_index % self.handlers.len();
        let mut visited = 0usize;
        while visited < handler_visit_limit
            && report.polled < handler_poll_limit
            && !self.handlers.is_empty()
        {
            if index >= self.handlers.len() {
                index = 0;
            }
            let process = self.handlers[index].process;
            let Some(process_state) = processes.get(process).map(|process| process.state.clone())
            else {
                return Err(format!(
                    "VM HTTP handler process {} disappeared",
                    process.as_u64()
                ));
            };
            if process_state != VmProcessState::Runnable {
                report.skipped_blocked += 1;
                visited += 1;
                index += 1;
                continue;
            }
            let poll = {
                let handler_state = &mut self.handlers[index];
                let request_resources = &mut self.request_resources;
                let lifecycle_hook = &mut self.lifecycle_hook;
                poll_or_park_http1_tcp_exchange_with_connection(
                    tcp,
                    handler_state.stream,
                    VmHttpActorExchange {
                        processes,
                        process: handler_state.process,
                        buffer: &mut handler_state.buffer,
                        close_connection: close_completed_connections,
                        response_memory: Some(&mut self.response_memory),
                    },
                    |request| {
                        dispatch_http_handler(
                            request_resources,
                            lifecycle_hook,
                            process,
                            request,
                            &mut handler,
                        )
                    },
                )?
            };
            report.polled += 1;
            visited += 1;
            match poll {
                VmHttpTcpActorPoll::Complete(exchange) => {
                    self.last_completed_handlers.push(process);
                    self.completed_total += 1;
                    report.completed += 1;
                    if close_completed_connections || exchange.close_connection {
                        let handler_state = self.handlers.remove(index);
                        let exit_reason = VmExitReason::Normal;
                        self.finish_handler(processes, tcp, &handler_state, exit_reason)?;
                        if self.handlers.is_empty() || index >= self.handlers.len() {
                            index = 0;
                        }
                    } else {
                        index += 1;
                    }
                }
                VmHttpTcpActorPoll::Parked => {
                    report.parked += 1;
                    index += 1;
                }
                VmHttpTcpActorPoll::Ready => index += 1,
            }
        }
        self.next_handler_index = if self.handlers.is_empty() {
            0
        } else {
            index % self.handlers.len()
        };
        Ok(report)
    }

    /// Polls the VM HTTP server with listener-bound TLS stream state.
    #[cfg(test)]
    fn poll_tls_with_policy(
        &mut self,
        processes: &mut VmProcessTable,
        tcp: &mut VmTcpRuntime,
        tls: &VmTlsRuntime,
        policy: VmHttpServerPollPolicy,
        handler: impl FnMut(::http::Request<String>) -> Result<::http::Response<String>, String>,
    ) -> Result<VmHttpTcpServerPoll, String> {
        let accept_limit = policy.accept_limit;
        if accept_limit == 0 {
            return Err("VM HTTP server accept limit must be greater than 0".to_string());
        }
        self.poll_tls_with_policy_internal(processes, tcp, tls, policy, handler)
    }

    /// Polls retained TLS handlers without accepting new connections.
    #[cfg(test)]
    pub(super) fn poll_retained_tls_handlers(
        &mut self,
        processes: &mut VmProcessTable,
        tcp: &mut VmTcpRuntime,
        tls: &VmTlsRuntime,
        handler_poll_limit: usize,
        handler: impl FnMut(::http::Request<String>) -> Result<::http::Response<String>, String>,
    ) -> Result<VmHttpTcpServerPoll, String> {
        self.poll_tls_with_policy_internal(
            processes,
            tcp,
            tls,
            VmHttpServerPollPolicy {
                close_completed_connections: true,
                accept_limit: 0,
                handler_poll_limit,
            },
            handler,
        )
    }

    /// Polls TLS HTTP work with an internal accept budget.
    #[cfg(test)]
    fn poll_tls_with_policy_internal(
        &mut self,
        processes: &mut VmProcessTable,
        tcp: &mut VmTcpRuntime,
        tls: &VmTlsRuntime,
        policy: VmHttpServerPollPolicy,
        mut handler: impl FnMut(::http::Request<String>) -> Result<::http::Response<String>, String>,
    ) -> Result<VmHttpTcpServerPoll, String> {
        let VmHttpServerPollPolicy {
            close_completed_connections,
            accept_limit,
            handler_poll_limit,
        } = policy;
        self.last_completed_handlers.clear();
        if handler_poll_limit == 0 {
            return Err("VM HTTP server handler poll limit must be greater than 0".to_string());
        }
        let mut report = VmHttpTcpServerPoll::default();
        while report.accepted < accept_limit {
            if self.saturated_overload_policy() == Some(VmHttpOverloadPolicy::Queue) {
                break;
            }
            let handler_source = self.handler_source.clone();
            let accepted_handler =
                accept_http1_tls_tcp_handler(processes, tcp, tls, self.listener, handler_source)?;
            let Some(handler_state) = accepted_handler else {
                break;
            };
            self.admit_handler(processes, tcp, handler_state, &mut report)?;
        }

        if self.handlers.is_empty() {
            self.next_handler_index = 0;
            return Ok(report);
        }

        let handler_visit_limit = self.handlers.len();
        let mut index = self.next_handler_index % self.handlers.len();
        let mut visited = 0usize;
        while visited < handler_visit_limit
            && report.polled < handler_poll_limit
            && !self.handlers.is_empty()
        {
            if index >= self.handlers.len() {
                index = 0;
            }
            let process = self.handlers[index].process;
            let Some(process_state) = processes.get(process).map(|process| process.state.clone())
            else {
                return Err(format!(
                    "VM HTTP handler process {} disappeared",
                    process.as_u64()
                ));
            };
            if process_state != VmProcessState::Runnable {
                report.skipped_blocked += 1;
                visited += 1;
                index += 1;
                continue;
            }
            let handler_state = &mut self.handlers[index];
            let request_resources = &mut self.request_resources;
            let lifecycle_hook = &mut self.lifecycle_hook;
            let tls_stream = handler_state
                .tls_stream
                .as_mut()
                .ok_or_else(|| "VM HTTP TLS handler missing TLS stream state".to_string())?;
            let poll_result = poll_or_park_http1_tls_tcp_exchange_with_connection(
                tcp,
                tls_stream,
                VmHttpActorExchange {
                    processes,
                    process: handler_state.process,
                    buffer: &mut handler_state.buffer,
                    close_connection: close_completed_connections,
                    response_memory: Some(&mut self.response_memory),
                },
                |request| {
                    dispatch_http_handler(
                        request_resources,
                        lifecycle_hook,
                        process,
                        request,
                        &mut handler,
                    )
                },
            );
            let poll = poll_result?;
            report.polled += 1;
            visited += 1;
            match poll {
                VmHttpTcpActorPoll::Complete(exchange) => {
                    self.last_completed_handlers.push(process);
                    self.completed_total += 1;
                    report.completed += 1;
                    if close_completed_connections || exchange.close_connection {
                        let handler_state = self.handlers.remove(index);
                        let exit_reason = VmExitReason::Normal;
                        self.finish_handler(processes, tcp, &handler_state, exit_reason)?;
                        index = index.min(self.handlers.len().saturating_sub(1));
                    } else {
                        index += 1;
                    }
                }
                VmHttpTcpActorPoll::Parked => {
                    report.parked += 1;
                    index += 1;
                }
                VmHttpTcpActorPoll::Ready => index += 1,
            }
        }
        self.next_handler_index = if self.handlers.is_empty() {
            0
        } else {
            index % self.handlers.len()
        };
        Ok(report)
    }
}

pub(crate) const HTTP_HEADER_LIMIT: usize = 64 * 1024;
pub(crate) const HTTP_BODY_LIMIT: usize = 1024 * 1024;

impl<T> VmHttpQueue<T> {
    /// Creates a bounded VM HTTP queue.
    ///
    /// Inputs:
    /// - `capacity`: maximum queued work items before enqueue applies
    ///   backpressure.
    ///
    /// Output:
    /// - Queue ready for acceptor and handler worker use.
    ///
    /// Transformation:
    /// - Initializes lock-protected FIFO state and condition variables.
    pub(crate) fn new(capacity: usize) -> Result<Self, String> {
        if capacity == 0 {
            return Err("VM HTTP queue capacity must be greater than 0".to_string());
        }
        Ok(Self {
            capacity,
            state: Mutex::new(VmHttpQueueState {
                items: VecDeque::with_capacity(capacity),
                max_depth: 0,
                enqueue_count: 0,
                dequeue_count: 0,
                enqueue_wait_count: 0,
                enqueue_wait_total_ns: 0,
                dequeue_wait_count: 0,
                dequeue_wait_total_ns: 0,
                parked_producers: 0,
                parked_consumers: 0,
                max_parked_producers: 0,
                max_parked_consumers: 0,
                producer_wakeup_count: 0,
                consumer_wakeup_count: 0,
            }),
            not_empty: Condvar::new(),
            not_full: Condvar::new(),
        })
    }

    /// Returns the configured queue capacity.
    #[cfg(test)]
    pub(crate) fn capacity(&self) -> usize {
        self.capacity
    }

    /// Enqueues one HTTP work item, blocking when the queue is full.
    ///
    /// Inputs:
    /// - `item`: accepted transport item to hand to the VM handler worker.
    ///
    /// Output:
    /// - Success once the item is visible to the consumer, or a stable
    ///   diagnostic when queue synchronization fails.
    ///
    /// Transformation:
    /// - Waits for capacity, records backpressure metrics, appends to the FIFO
    ///   queue, and wakes one blocked consumer.
    #[cfg(test)]
    pub(crate) fn enqueue(&self, item: T) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "VM HTTP queue lock poisoned".to_string())?;
        if state.items.len() >= self.capacity {
            let wait_start = Instant::now();
            state.enqueue_wait_count += 1;
            state.parked_producers += 1;
            state.max_parked_producers = state.max_parked_producers.max(state.parked_producers);
            while state.items.len() >= self.capacity {
                state = self
                    .not_full
                    .wait(state)
                    .map_err(|_| "VM HTTP queue wait poisoned".to_string())?;
                state.producer_wakeup_count += 1;
            }
            state.parked_producers -= 1;
            state.enqueue_wait_total_ns = state
                .enqueue_wait_total_ns
                .saturating_add(wait_start.elapsed().as_nanos());
        }
        state.items.push_back(item);
        state.enqueue_count += 1;
        state.max_depth = state.max_depth.max(state.items.len());
        self.not_empty.notify_one();
        Ok(())
    }

    /// Dequeues one HTTP work item for VM handler execution.
    ///
    /// Inputs:
    /// - None.
    ///
    /// Output:
    /// - Next FIFO work item, or a stable diagnostic when queue
    ///   synchronization fails.
    ///
    /// Transformation:
    /// - Waits for work, removes the oldest item, and wakes one blocked
    ///   producer.
    #[cfg(test)]
    pub(crate) fn dequeue(&self) -> Result<T, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "VM HTTP queue lock poisoned".to_string())?;
        if state.items.is_empty() {
            let wait_start = Instant::now();
            state.dequeue_wait_count += 1;
            state.parked_consumers += 1;
            state.max_parked_consumers = state.max_parked_consumers.max(state.parked_consumers);
            while state.items.is_empty() {
                state = self
                    .not_empty
                    .wait(state)
                    .map_err(|_| "VM HTTP queue wait poisoned".to_string())?;
                state.consumer_wakeup_count += 1;
            }
            state.parked_consumers -= 1;
            state.dequeue_wait_total_ns = state
                .dequeue_wait_total_ns
                .saturating_add(wait_start.elapsed().as_nanos());
        }
        let item = state
            .items
            .pop_front()
            .ok_or_else(|| "VM HTTP queue unexpectedly empty".to_string())?;
        state.dequeue_count += 1;
        self.not_full.notify_one();
        Ok(item)
    }

    /// Returns queue pressure metrics after runtime work.
    #[cfg(test)]
    pub(crate) fn metrics(&self) -> Result<VmHttpQueueMetrics, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "VM HTTP queue lock poisoned".to_string())?;
        Ok(VmHttpQueueMetrics {
            current_depth: state.items.len(),
            max_depth: state.max_depth,
            enqueue_count: state.enqueue_count,
            dequeue_count: state.dequeue_count,
            enqueue_wait_count: state.enqueue_wait_count,
            enqueue_wait_total_ns: state.enqueue_wait_total_ns,
            dequeue_wait_count: state.dequeue_wait_count,
            dequeue_wait_total_ns: state.dequeue_wait_total_ns,
            parked_producers: state.parked_producers,
            parked_consumers: state.parked_consumers,
            max_parked_producers: state.max_parked_producers,
            max_parked_consumers: state.max_parked_consumers,
            producer_wakeup_count: state.producer_wakeup_count,
            consumer_wakeup_count: state.consumer_wakeup_count,
        })
    }
}
