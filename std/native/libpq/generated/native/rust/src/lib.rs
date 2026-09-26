//! Generated Rust adapter for the `terlan_libpq` native package.

#![deny(unsafe_op_in_unsafe_fn)]

/// Raw C ABI declarations used by the safe generated adapter.
pub mod ffi {
    /// VM-confined nonblocking libpq connection.
    #[repr(C)]
    pub struct TerlanLibpqConnection {
        _private: [u8; 0],
    }

    /// Owned libpq query result copied into VM values before disposal.
    #[repr(C)]
    pub struct TerlanLibpqResult {
        _private: [u8; 0],
    }

    unsafe extern "C" {
        /// Invokes the raw `terlan_libpq_connection_abort` C ABI symbol.
        pub fn terlan_libpq_connection_abort(connection: *mut TerlanLibpqConnection) -> i32;
        /// Invokes the raw `terlan_libpq_connection_clear_parameters` C ABI symbol.
        pub fn terlan_libpq_connection_clear_parameters(
            connection: *mut TerlanLibpqConnection,
        ) -> i32;
        /// Invokes the raw `terlan_libpq_connection_consume_input` C ABI symbol.
        pub fn terlan_libpq_connection_consume_input(connection: *mut TerlanLibpqConnection)
            -> i32;
        /// Invokes the raw `terlan_libpq_connection_destroy` C ABI symbol.
        pub fn terlan_libpq_connection_destroy(connection: *mut TerlanLibpqConnection) -> i32;
        /// Invokes the raw `terlan_libpq_connection_error_bytes` C ABI symbol.
        pub fn terlan_libpq_connection_error_bytes(
            connection: *mut TerlanLibpqConnection,
            out_bytes: *mut *mut i64,
        ) -> i32;
        /// Invokes the raw `terlan_libpq_connection_error_length` C ABI symbol.
        pub fn terlan_libpq_connection_error_length(
            connection: *mut TerlanLibpqConnection,
            out_length: *mut i64,
        ) -> i32;
        /// Invokes the raw `terlan_libpq_connection_is_busy` C ABI symbol.
        pub fn terlan_libpq_connection_is_busy(
            connection: *const TerlanLibpqConnection,
            out_busy: *mut bool,
        ) -> i32;
        /// Invokes the raw `terlan_libpq_connection_next_result` C ABI symbol.
        pub fn terlan_libpq_connection_next_result(
            connection: *mut TerlanLibpqConnection,
            out_result: *mut *mut TerlanLibpqResult,
        ) -> i32;
        /// Invokes the raw `terlan_libpq_connection_poll` C ABI symbol.
        pub fn terlan_libpq_connection_poll(
            connection: *mut TerlanLibpqConnection,
            out_state: *mut i64,
        ) -> i32;
        /// Invokes the raw `terlan_libpq_connection_push_null` C ABI symbol.
        pub fn terlan_libpq_connection_push_null(connection: *mut TerlanLibpqConnection) -> i32;
        /// Invokes the raw `terlan_libpq_connection_push_text` C ABI symbol.
        pub fn terlan_libpq_connection_push_text(
            connection: *mut TerlanLibpqConnection,
            value: *const std::ffi::c_char,
        ) -> i32;
        /// Invokes the raw `terlan_libpq_connection_send_batch` C ABI symbol.
        pub fn terlan_libpq_connection_send_batch(
            connection: *mut TerlanLibpqConnection,
            sql: *const std::ffi::c_char,
        ) -> i32;
        /// Invokes the raw `terlan_libpq_connection_send_query` C ABI symbol.
        pub fn terlan_libpq_connection_send_query(
            connection: *mut TerlanLibpqConnection,
            sql: *const std::ffi::c_char,
        ) -> i32;
        /// Invokes the raw `terlan_libpq_connection_socket` C ABI symbol.
        pub fn terlan_libpq_connection_socket(
            connection: *const TerlanLibpqConnection,
            out_socket: *mut i64,
        ) -> i32;
        /// Invokes the raw `terlan_libpq_connection_start` C ABI symbol.
        pub fn terlan_libpq_connection_start(
            url: *const std::ffi::c_char,
            out_connection: *mut *mut TerlanLibpqConnection,
        ) -> i32;
        /// Invokes the raw `terlan_libpq_result_affected_rows` C ABI symbol.
        pub fn terlan_libpq_result_affected_rows(
            result: *const TerlanLibpqResult,
            out_count: *mut i64,
        ) -> i32;
        /// Invokes the raw `terlan_libpq_result_column_count` C ABI symbol.
        pub fn terlan_libpq_result_column_count(
            result: *const TerlanLibpqResult,
            out_count: *mut i64,
        ) -> i32;
        /// Invokes the raw `terlan_libpq_result_column_oid` C ABI symbol.
        pub fn terlan_libpq_result_column_oid(
            result: *const TerlanLibpqResult,
            column: i64,
            out_oid: *mut i64,
        ) -> i32;
        /// Invokes the raw `terlan_libpq_result_destroy` C ABI symbol.
        pub fn terlan_libpq_result_destroy(result: *mut TerlanLibpqResult) -> i32;
        /// Invokes the raw `terlan_libpq_result_row_count` C ABI symbol.
        pub fn terlan_libpq_result_row_count(
            result: *const TerlanLibpqResult,
            out_count: *mut i64,
        ) -> i32;
        /// Invokes the raw `terlan_libpq_result_select_column_name` C ABI symbol.
        pub fn terlan_libpq_result_select_column_name(
            result: *mut TerlanLibpqResult,
            column: i64,
        ) -> i32;
        /// Invokes the raw `terlan_libpq_result_select_value` C ABI symbol.
        pub fn terlan_libpq_result_select_value(
            result: *mut TerlanLibpqResult,
            row: i64,
            column: i64,
        ) -> i32;
        /// Invokes the raw `terlan_libpq_result_status` C ABI symbol.
        pub fn terlan_libpq_result_status(
            result: *const TerlanLibpqResult,
            out_status: *mut i64,
        ) -> i32;
        /// Invokes the raw `terlan_libpq_result_value_bytes` C ABI symbol.
        pub fn terlan_libpq_result_value_bytes(
            result: *const TerlanLibpqResult,
            out_bytes: *mut *mut i64,
        ) -> i32;
        /// Invokes the raw `terlan_libpq_result_value_is_null` C ABI symbol.
        pub fn terlan_libpq_result_value_is_null(
            result: *const TerlanLibpqResult,
            out_null: *mut bool,
        ) -> i32;
        /// Invokes the raw `terlan_libpq_result_value_length` C ABI symbol.
        pub fn terlan_libpq_result_value_length(
            result: *const TerlanLibpqResult,
            out_length: *mut i64,
        ) -> i32;
    }
}

use std::ptr::NonNull;

/// Error returned when a generated C ABI operation fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CAbiError {
    /// Stable operation identifier for the failed call.
    pub operation: &'static str,
    /// Status code returned by the native implementation.
    pub status: i32,
}

/// VM-confined nonblocking libpq connection.
pub struct Connection {
    raw: NonNull<ffi::TerlanLibpqConnection>,
}

// SAFETY: reviewed `send_only` metadata permits exclusive ownership transfer; the wrapper intentionally remains !Sync.
unsafe impl Send for Connection {}

/// Owned libpq query result copied into VM values before disposal.
pub struct QueryResult {
    raw: NonNull<ffi::TerlanLibpqResult>,
}

impl Connection {
    /// Starts a nonblocking libpq connection.
    pub fn start(url: &str) -> Result<Self, CAbiError> {
        let url_c = std::ffi::CString::new(url.as_bytes()).map_err(|_| CAbiError {
            operation: "postgres.libpq.connection.start",
            status: -3,
        })?;
        let mut raw: *mut ffi::TerlanLibpqConnection = std::ptr::null_mut();
        // SAFETY: generated arguments follow the reviewed ownership metadata.
        let status = unsafe { ffi::terlan_libpq_connection_start(url_c.as_ptr(), &mut raw) };
        check_status("postgres.libpq.connection.start", status, 0)?;
        let raw = NonNull::new(raw).ok_or(CAbiError {
            operation: "postgres.libpq.connection.start",
            status: -1,
        })?;
        Ok(Self { raw })
    }

    /// Returns the socket watched by the VM reactor.
    pub fn socket(&self) -> Result<i64, CAbiError> {
        let mut out_out_socket: i64 = Default::default();
        // SAFETY: generated arguments follow the reviewed ownership metadata.
        let status =
            unsafe { ffi::terlan_libpq_connection_socket(self.raw.as_ptr(), &mut out_out_socket) };
        check_status("postgres.libpq.connection.socket", status, 0)?;
        Ok(out_out_socket)
    }

    /// Advances nonblocking connection setup once.
    pub fn poll_connect(&mut self) -> Result<i64, CAbiError> {
        let mut out_out_state: i64 = Default::default();
        // SAFETY: generated arguments follow the reviewed ownership metadata.
        let status =
            unsafe { ffi::terlan_libpq_connection_poll(self.raw.as_ptr(), &mut out_out_state) };
        check_status("postgres.libpq.connection.poll_connect", status, 0)?;
        Ok(out_out_state)
    }

    /// Copies the current driver error into adapter scratch storage.
    pub fn error_length(&mut self) -> Result<i64, CAbiError> {
        let mut out_out_length: i64 = Default::default();
        // SAFETY: generated arguments follow the reviewed ownership metadata.
        let status = unsafe {
            ffi::terlan_libpq_connection_error_length(self.raw.as_ptr(), &mut out_out_length)
        };
        check_status("postgres.libpq.connection.error_length", status, 0)?;
        Ok(out_out_length)
    }

    /// Copies current driver error bytes into Rust-owned storage.
    pub fn error_bytes(&mut self) -> Result<Vec<i64>, CAbiError> {
        let out_bytes_length = self.error_length()?;
        let mut out_out_bytes: *mut i64 = std::ptr::null_mut();
        // SAFETY: generated arguments follow the reviewed ownership metadata.
        let status = unsafe {
            ffi::terlan_libpq_connection_error_bytes(self.raw.as_ptr(), &mut out_out_bytes)
        };
        check_status("postgres.libpq.connection.error_bytes", status, 0)?;
        let length = usize::try_from(out_bytes_length).map_err(|_| CAbiError {
            operation: "postgres.libpq.connection.error_bytes",
            status: -2,
        })?;
        let values = if length == 0 {
            Vec::new()
        } else {
            let pointer = NonNull::new(out_out_bytes).ok_or(CAbiError {
                operation: "postgres.libpq.connection.error_bytes",
                status: -1,
            })?;
            // SAFETY: metadata ties this borrowed array to `self`; it is copied before the borrow ends.
            unsafe { std::slice::from_raw_parts(pointer.as_ptr(), length).to_vec() }
        };
        Ok(values)
    }

    /// Clears pending query parameters.
    pub fn clear_parameters(&mut self) -> Result<(), CAbiError> {
        // SAFETY: generated arguments follow the reviewed ownership metadata.
        let status = unsafe { ffi::terlan_libpq_connection_clear_parameters(self.raw.as_ptr()) };
        check_status("postgres.libpq.connection.clear_parameters", status, 0)?;
        Ok(())
    }

    /// Appends a null query parameter.
    pub fn push_null(&mut self) -> Result<(), CAbiError> {
        // SAFETY: generated arguments follow the reviewed ownership metadata.
        let status = unsafe { ffi::terlan_libpq_connection_push_null(self.raw.as_ptr()) };
        check_status("postgres.libpq.connection.push_null", status, 0)?;
        Ok(())
    }

    /// Appends a text-format query parameter.
    pub fn push_text(&mut self, value: &str) -> Result<(), CAbiError> {
        let value_c = std::ffi::CString::new(value.as_bytes()).map_err(|_| CAbiError {
            operation: "postgres.libpq.connection.push_text",
            status: -3,
        })?;
        // SAFETY: generated arguments follow the reviewed ownership metadata.
        let status =
            unsafe { ffi::terlan_libpq_connection_push_text(self.raw.as_ptr(), value_c.as_ptr()) };
        check_status("postgres.libpq.connection.push_text", status, 0)?;
        Ok(())
    }

    /// Submits a parameterized query without blocking.
    pub fn send_query(&mut self, sql: &str) -> Result<(), CAbiError> {
        let sql_c = std::ffi::CString::new(sql.as_bytes()).map_err(|_| CAbiError {
            operation: "postgres.libpq.connection.send_query",
            status: -3,
        })?;
        // SAFETY: generated arguments follow the reviewed ownership metadata.
        let status =
            unsafe { ffi::terlan_libpq_connection_send_query(self.raw.as_ptr(), sql_c.as_ptr()) };
        check_status("postgres.libpq.connection.send_query", status, 0)?;
        Ok(())
    }

    /// Submits a trusted multi-statement SQL batch without parameters.
    pub fn send_batch(&mut self, sql: &str) -> Result<(), CAbiError> {
        let sql_c = std::ffi::CString::new(sql.as_bytes()).map_err(|_| CAbiError {
            operation: "postgres.libpq.connection.send_batch",
            status: -3,
        })?;
        // SAFETY: generated arguments follow the reviewed ownership metadata.
        let status =
            unsafe { ffi::terlan_libpq_connection_send_batch(self.raw.as_ptr(), sql_c.as_ptr()) };
        check_status("postgres.libpq.connection.send_batch", status, 0)?;
        Ok(())
    }

    /// Consumes currently readable query bytes.
    pub fn consume_input(&mut self) -> Result<(), CAbiError> {
        // SAFETY: generated arguments follow the reviewed ownership metadata.
        let status = unsafe { ffi::terlan_libpq_connection_consume_input(self.raw.as_ptr()) };
        check_status("postgres.libpq.connection.consume_input", status, 0)?;
        Ok(())
    }

    /// Reports whether the current query needs more input.
    pub fn is_busy(&self) -> Result<bool, CAbiError> {
        let mut out_out_busy: bool = Default::default();
        // SAFETY: generated arguments follow the reviewed ownership metadata.
        let status =
            unsafe { ffi::terlan_libpq_connection_is_busy(self.raw.as_ptr(), &mut out_out_busy) };
        check_status("postgres.libpq.connection.is_busy", status, 0)?;
        Ok(out_out_busy)
    }

    /// Takes the next completed query result.
    pub fn next_result(&mut self) -> Result<QueryResult, CAbiError> {
        let mut raw: *mut ffi::TerlanLibpqResult = std::ptr::null_mut();
        // SAFETY: generated arguments follow the reviewed ownership metadata.
        let status =
            unsafe { ffi::terlan_libpq_connection_next_result(self.raw.as_ptr(), &mut raw) };
        check_status("postgres.libpq.connection.next_result", status, 0)?;
        let raw = NonNull::new(raw).ok_or(CAbiError {
            operation: "postgres.libpq.connection.next_result",
            status: -1,
        })?;
        Ok(QueryResult { raw })
    }

    /// Cancels work by closing the connection immediately.
    pub fn abort(&mut self) -> Result<(), CAbiError> {
        // SAFETY: generated arguments follow the reviewed ownership metadata.
        let status = unsafe { ffi::terlan_libpq_connection_abort(self.raw.as_ptr()) };
        check_status("postgres.libpq.connection.abort", status, 0)?;
        Ok(())
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        // SAFETY: this adapter is the sole owner and invokes the destructor once.
        let status = unsafe { ffi::terlan_libpq_connection_destroy(self.raw.as_ptr()) };
        debug_assert_eq!(status, 0);
    }
}

impl QueryResult {
    /// Returns the libpq result status.
    pub fn status(&self) -> Result<i64, CAbiError> {
        let mut out_out_status: i64 = Default::default();
        // SAFETY: generated arguments follow the reviewed ownership metadata.
        let status =
            unsafe { ffi::terlan_libpq_result_status(self.raw.as_ptr(), &mut out_out_status) };
        check_status("postgres.libpq.result.status", status, 0)?;
        Ok(out_out_status)
    }

    /// Returns tuple count.
    pub fn row_count(&self) -> Result<i64, CAbiError> {
        let mut out_out_count: i64 = Default::default();
        // SAFETY: generated arguments follow the reviewed ownership metadata.
        let status =
            unsafe { ffi::terlan_libpq_result_row_count(self.raw.as_ptr(), &mut out_out_count) };
        check_status("postgres.libpq.result.row_count", status, 0)?;
        Ok(out_out_count)
    }

    /// Returns field count.
    pub fn column_count(&self) -> Result<i64, CAbiError> {
        let mut out_out_count: i64 = Default::default();
        // SAFETY: generated arguments follow the reviewed ownership metadata.
        let status =
            unsafe { ffi::terlan_libpq_result_column_count(self.raw.as_ptr(), &mut out_out_count) };
        check_status("postgres.libpq.result.column_count", status, 0)?;
        Ok(out_out_count)
    }

    /// Selects and copies one field name into adapter scratch storage.
    pub fn select_column_name(&mut self, column: i64) -> Result<(), CAbiError> {
        // SAFETY: generated arguments follow the reviewed ownership metadata.
        let status =
            unsafe { ffi::terlan_libpq_result_select_column_name(self.raw.as_ptr(), column) };
        check_status("postgres.libpq.result.select_column_name", status, 0)?;
        Ok(())
    }

    /// Returns the stable PostgreSQL field type OID.
    pub fn column_oid(&self, column: i64) -> Result<i64, CAbiError> {
        let mut out_out_oid: i64 = Default::default();
        // SAFETY: generated arguments follow the reviewed ownership metadata.
        let status = unsafe {
            ffi::terlan_libpq_result_column_oid(self.raw.as_ptr(), column, &mut out_out_oid)
        };
        check_status("postgres.libpq.result.column_oid", status, 0)?;
        Ok(out_out_oid)
    }

    /// Selects and copies one result value into adapter scratch storage.
    pub fn select_value(&mut self, row: i64, column: i64) -> Result<(), CAbiError> {
        // SAFETY: generated arguments follow the reviewed ownership metadata.
        let status =
            unsafe { ffi::terlan_libpq_result_select_value(self.raw.as_ptr(), row, column) };
        check_status("postgres.libpq.result.select_value", status, 0)?;
        Ok(())
    }

    /// Returns selected value byte length.
    pub fn value_length(&self) -> Result<i64, CAbiError> {
        let mut out_out_length: i64 = Default::default();
        // SAFETY: generated arguments follow the reviewed ownership metadata.
        let status = unsafe {
            ffi::terlan_libpq_result_value_length(self.raw.as_ptr(), &mut out_out_length)
        };
        check_status("postgres.libpq.result.value_length", status, 0)?;
        Ok(out_out_length)
    }

    /// Copies selected value bytes into Rust-owned storage.
    pub fn value_bytes(&self) -> Result<Vec<i64>, CAbiError> {
        let out_bytes_length = self.value_length()?;
        let mut out_out_bytes: *mut i64 = std::ptr::null_mut();
        // SAFETY: generated arguments follow the reviewed ownership metadata.
        let status =
            unsafe { ffi::terlan_libpq_result_value_bytes(self.raw.as_ptr(), &mut out_out_bytes) };
        check_status("postgres.libpq.result.value_bytes", status, 0)?;
        let length = usize::try_from(out_bytes_length).map_err(|_| CAbiError {
            operation: "postgres.libpq.result.value_bytes",
            status: -2,
        })?;
        let values = if length == 0 {
            Vec::new()
        } else {
            let pointer = NonNull::new(out_out_bytes).ok_or(CAbiError {
                operation: "postgres.libpq.result.value_bytes",
                status: -1,
            })?;
            // SAFETY: metadata ties this borrowed array to `self`; it is copied before the borrow ends.
            unsafe { std::slice::from_raw_parts(pointer.as_ptr(), length).to_vec() }
        };
        Ok(values)
    }

    /// Reports selected SQL null state.
    pub fn value_is_null(&self) -> Result<bool, CAbiError> {
        let mut out_out_null: bool = Default::default();
        // SAFETY: generated arguments follow the reviewed ownership metadata.
        let status =
            unsafe { ffi::terlan_libpq_result_value_is_null(self.raw.as_ptr(), &mut out_out_null) };
        check_status("postgres.libpq.result.value_is_null", status, 0)?;
        Ok(out_out_null)
    }

    /// Returns command affected-row count.
    pub fn affected_rows(&self) -> Result<i64, CAbiError> {
        let mut out_out_count: i64 = Default::default();
        // SAFETY: generated arguments follow the reviewed ownership metadata.
        let status = unsafe {
            ffi::terlan_libpq_result_affected_rows(self.raw.as_ptr(), &mut out_out_count)
        };
        check_status("postgres.libpq.result.affected_rows", status, 0)?;
        Ok(out_out_count)
    }
}

impl Drop for QueryResult {
    fn drop(&mut self) {
        // SAFETY: this adapter is the sole owner and invokes the destructor once.
        let status = unsafe { ffi::terlan_libpq_result_destroy(self.raw.as_ptr()) };
        debug_assert_eq!(status, 0);
    }
}

fn check_status<T>(operation: &'static str, status: T, success: i32) -> Result<(), CAbiError>
where
    T: TryInto<i32>,
{
    let status = status.try_into().map_err(|_| CAbiError {
        operation,
        status: -2,
    })?;
    if status == success {
        Ok(())
    } else {
        Err(CAbiError { operation, status })
    }
}

mod package_extension;
pub use package_extension::*;
