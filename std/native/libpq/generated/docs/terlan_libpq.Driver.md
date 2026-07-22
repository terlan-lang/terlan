# terlan_libpq.Driver

Generated stable libpq C ABI boundary for the VM Postgres driver.

## `start`

Starts a nonblocking libpq connection.

- C symbol: `terlan_libpq_connection_start`
- NativeBoundary operation: `postgres.libpq.connection.start`
- Ownership: `opaque_handle`

## `dispose_connection`

Closes and frees a libpq connection.

- C symbol: `terlan_libpq_connection_destroy`
- NativeBoundary operation: `postgres.libpq.connection.dispose`
- Ownership: `dispose_handle`

## `socket`

Returns the socket watched by the VM reactor.

- C symbol: `terlan_libpq_connection_socket`
- NativeBoundary operation: `postgres.libpq.connection.socket`
- Ownership: `borrowed_handle`

## `poll_connect`

Advances nonblocking connection setup once.

- C symbol: `terlan_libpq_connection_poll`
- NativeBoundary operation: `postgres.libpq.connection.poll_connect`
- Ownership: `mutable_handle`

## `error_length`

Copies the current driver error into adapter scratch storage.

- C symbol: `terlan_libpq_connection_error_length`
- NativeBoundary operation: `postgres.libpq.connection.error_length`
- Ownership: `mutable_handle`

## `error_bytes`

Copies current driver error bytes into Rust-owned storage.

- C symbol: `terlan_libpq_connection_error_bytes`
- NativeBoundary operation: `postgres.libpq.connection.error_bytes`
- Ownership: `mutable_handle`

## `clear_parameters`

Clears pending query parameters.

- C symbol: `terlan_libpq_connection_clear_parameters`
- NativeBoundary operation: `postgres.libpq.connection.clear_parameters`
- Ownership: `mutable_handle`

## `push_null`

Appends a null query parameter.

- C symbol: `terlan_libpq_connection_push_null`
- NativeBoundary operation: `postgres.libpq.connection.push_null`
- Ownership: `mutable_handle`

## `push_text`

Appends a text-format query parameter.

- C symbol: `terlan_libpq_connection_push_text`
- NativeBoundary operation: `postgres.libpq.connection.push_text`
- Ownership: `mutable_handle`

## `send_query`

Submits a parameterized query without blocking.

- C symbol: `terlan_libpq_connection_send_query`
- NativeBoundary operation: `postgres.libpq.connection.send_query`
- Ownership: `mutable_handle`

## `send_batch`

Submits a trusted multi-statement SQL batch without parameters.

- C symbol: `terlan_libpq_connection_send_batch`
- NativeBoundary operation: `postgres.libpq.connection.send_batch`
- Ownership: `mutable_handle`

## `consume_input`

Consumes currently readable query bytes.

- C symbol: `terlan_libpq_connection_consume_input`
- NativeBoundary operation: `postgres.libpq.connection.consume_input`
- Ownership: `mutable_handle`

## `is_busy`

Reports whether the current query needs more input.

- C symbol: `terlan_libpq_connection_is_busy`
- NativeBoundary operation: `postgres.libpq.connection.is_busy`
- Ownership: `borrowed_handle`

## `next_result`

Takes the next completed query result.

- C symbol: `terlan_libpq_connection_next_result`
- NativeBoundary operation: `postgres.libpq.connection.next_result`
- Ownership: `transferable_handle`

## `abort`

Cancels work by closing the connection immediately.

- C symbol: `terlan_libpq_connection_abort`
- NativeBoundary operation: `postgres.libpq.connection.abort`
- Ownership: `mutable_handle`

## `dispose_result`

Clears and frees a query result.

- C symbol: `terlan_libpq_result_destroy`
- NativeBoundary operation: `postgres.libpq.result.dispose`
- Ownership: `dispose_handle`

## `status`

Returns the libpq result status.

- C symbol: `terlan_libpq_result_status`
- NativeBoundary operation: `postgres.libpq.result.status`
- Ownership: `borrowed_handle`

## `row_count`

Returns tuple count.

- C symbol: `terlan_libpq_result_row_count`
- NativeBoundary operation: `postgres.libpq.result.row_count`
- Ownership: `borrowed_handle`

## `column_count`

Returns field count.

- C symbol: `terlan_libpq_result_column_count`
- NativeBoundary operation: `postgres.libpq.result.column_count`
- Ownership: `borrowed_handle`

## `select_column_name`

Selects and copies one field name into adapter scratch storage.

- C symbol: `terlan_libpq_result_select_column_name`
- NativeBoundary operation: `postgres.libpq.result.select_column_name`
- Ownership: `mutable_handle`

## `column_oid`

Returns the stable PostgreSQL field type OID.

- C symbol: `terlan_libpq_result_column_oid`
- NativeBoundary operation: `postgres.libpq.result.column_oid`
- Ownership: `borrowed_handle`

## `select_value`

Selects and copies one result value into adapter scratch storage.

- C symbol: `terlan_libpq_result_select_value`
- NativeBoundary operation: `postgres.libpq.result.select_value`
- Ownership: `mutable_handle`

## `value_length`

Returns selected value byte length.

- C symbol: `terlan_libpq_result_value_length`
- NativeBoundary operation: `postgres.libpq.result.value_length`
- Ownership: `borrowed_handle`

## `value_bytes`

Copies selected value bytes into Rust-owned storage.

- C symbol: `terlan_libpq_result_value_bytes`
- NativeBoundary operation: `postgres.libpq.result.value_bytes`
- Ownership: `borrowed_handle`

## `value_is_null`

Reports selected SQL null state.

- C symbol: `terlan_libpq_result_value_is_null`
- NativeBoundary operation: `postgres.libpq.result.value_is_null`
- Ownership: `borrowed_handle`

## `affected_rows`

Returns command affected-row count.

- C symbol: `terlan_libpq_result_affected_rows`
- NativeBoundary operation: `postgres.libpq.result.affected_rows`
- Ownership: `borrowed_handle`
