#include <libpq-fe.h>

#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

typedef struct TerlanLibpqConnection {
  PGconn *connection;
  char **parameters;
  int parameter_count;
  int parameter_capacity;
  int64_t *scratch;
  int64_t scratch_length;
} TerlanLibpqConnection;

typedef struct TerlanLibpqResult {
  PGresult *result;
  int64_t *scratch;
  int64_t scratch_length;
  bool selected_null;
} TerlanLibpqResult;

enum {
  TERLAN_LIBPQ_OK = 0,
  TERLAN_LIBPQ_NO_RESULT = 1,
  TERLAN_LIBPQ_INVALID_ARGUMENT = 2,
  TERLAN_LIBPQ_ALLOCATION_FAILED = 3,
  TERLAN_LIBPQ_DRIVER_ERROR = 4
};

static void clear_parameters(TerlanLibpqConnection *connection) {
  if (connection == NULL) {
    return;
  }
  for (int index = 0; index < connection->parameter_count; ++index) {
    free(connection->parameters[index]);
  }
  free(connection->parameters);
  connection->parameters = NULL;
  connection->parameter_count = 0;
  connection->parameter_capacity = 0;
}

static int copy_scratch(
    int64_t **scratch,
    int64_t *scratch_length,
    const char *bytes,
    size_t length) {
  int64_t *replacement = NULL;
  if (length > 0) {
    replacement = (int64_t *)malloc(length * sizeof(int64_t));
    if (replacement == NULL) {
      return TERLAN_LIBPQ_ALLOCATION_FAILED;
    }
    for (size_t index = 0; index < length; ++index) {
      replacement[index] = (unsigned char)bytes[index];
    }
  }
  free(*scratch);
  *scratch = replacement;
  *scratch_length = (int64_t)length;
  return TERLAN_LIBPQ_OK;
}

int32_t terlan_libpq_connection_start(
    const char *url,
    TerlanLibpqConnection **out_connection) {
  if (url == NULL || out_connection == NULL) {
    return TERLAN_LIBPQ_INVALID_ARGUMENT;
  }
  TerlanLibpqConnection *connection =
      (TerlanLibpqConnection *)calloc(1, sizeof(TerlanLibpqConnection));
  if (connection == NULL) {
    return TERLAN_LIBPQ_ALLOCATION_FAILED;
  }
  connection->connection = PQconnectStart(url);
  if (connection->connection == NULL) {
    free(connection);
    return TERLAN_LIBPQ_DRIVER_ERROR;
  }
  *out_connection = connection;
  return TERLAN_LIBPQ_OK;
}

int32_t terlan_libpq_connection_destroy(TerlanLibpqConnection *connection) {
  if (connection == NULL) {
    return TERLAN_LIBPQ_INVALID_ARGUMENT;
  }
  clear_parameters(connection);
  free(connection->scratch);
  if (connection->connection != NULL) {
    PQfinish(connection->connection);
  }
  free(connection);
  return TERLAN_LIBPQ_OK;
}

int32_t terlan_libpq_connection_socket(
    const TerlanLibpqConnection *connection,
    int64_t *out_socket) {
  if (connection == NULL || connection->connection == NULL || out_socket == NULL) {
    return TERLAN_LIBPQ_INVALID_ARGUMENT;
  }
  *out_socket = (int64_t)PQsocket(connection->connection);
  return *out_socket < 0 ? TERLAN_LIBPQ_DRIVER_ERROR : TERLAN_LIBPQ_OK;
}

int32_t terlan_libpq_connection_poll(
    TerlanLibpqConnection *connection,
    int64_t *out_state) {
  if (connection == NULL || connection->connection == NULL || out_state == NULL) {
    return TERLAN_LIBPQ_INVALID_ARGUMENT;
  }
  PostgresPollingStatusType state = PQconnectPoll(connection->connection);
  *out_state = (int64_t)state;
  if (state == PGRES_POLLING_OK && PQsetnonblocking(connection->connection, 1) != 0) {
    return TERLAN_LIBPQ_DRIVER_ERROR;
  }
  return state == PGRES_POLLING_FAILED ? TERLAN_LIBPQ_DRIVER_ERROR
                                       : TERLAN_LIBPQ_OK;
}

int32_t terlan_libpq_connection_error_length(
    TerlanLibpqConnection *connection,
    int64_t *out_length) {
  if (connection == NULL || connection->connection == NULL || out_length == NULL) {
    return TERLAN_LIBPQ_INVALID_ARGUMENT;
  }
  const char *message = PQerrorMessage(connection->connection);
  int status = copy_scratch(
      &connection->scratch,
      &connection->scratch_length,
      message,
      message == NULL ? 0 : strlen(message));
  *out_length = connection->scratch_length;
  return status;
}

int32_t terlan_libpq_connection_error_bytes(
    TerlanLibpqConnection *connection,
    int64_t **out_bytes) {
  if (connection == NULL || out_bytes == NULL) {
    return TERLAN_LIBPQ_INVALID_ARGUMENT;
  }
  *out_bytes = connection->scratch;
  return TERLAN_LIBPQ_OK;
}

int32_t terlan_libpq_connection_clear_parameters(TerlanLibpqConnection *connection) {
  if (connection == NULL) {
    return TERLAN_LIBPQ_INVALID_ARGUMENT;
  }
  clear_parameters(connection);
  return TERLAN_LIBPQ_OK;
}

static int reserve_parameter(TerlanLibpqConnection *connection) {
  if (connection->parameter_count < connection->parameter_capacity) {
    return TERLAN_LIBPQ_OK;
  }
  int next_capacity = connection->parameter_capacity == 0
      ? 4
      : connection->parameter_capacity * 2;
  char **parameters = (char **)realloc(
      connection->parameters,
      (size_t)next_capacity * sizeof(char *));
  if (parameters == NULL) {
    return TERLAN_LIBPQ_ALLOCATION_FAILED;
  }
  connection->parameters = parameters;
  connection->parameter_capacity = next_capacity;
  return TERLAN_LIBPQ_OK;
}

int32_t terlan_libpq_connection_push_null(TerlanLibpqConnection *connection) {
  if (connection == NULL) {
    return TERLAN_LIBPQ_INVALID_ARGUMENT;
  }
  int status = reserve_parameter(connection);
  if (status != TERLAN_LIBPQ_OK) {
    return status;
  }
  connection->parameters[connection->parameter_count++] = NULL;
  return TERLAN_LIBPQ_OK;
}

int32_t terlan_libpq_connection_push_text(
    TerlanLibpqConnection *connection,
    const char *value) {
  if (connection == NULL || value == NULL) {
    return TERLAN_LIBPQ_INVALID_ARGUMENT;
  }
  int status = reserve_parameter(connection);
  if (status != TERLAN_LIBPQ_OK) {
    return status;
  }
  size_t length = strlen(value);
  char *copy = (char *)malloc(length + 1);
  if (copy == NULL) {
    return TERLAN_LIBPQ_ALLOCATION_FAILED;
  }
  memcpy(copy, value, length + 1);
  connection->parameters[connection->parameter_count++] = copy;
  return TERLAN_LIBPQ_OK;
}

int32_t terlan_libpq_connection_send_query(
    TerlanLibpqConnection *connection,
    const char *sql) {
  if (connection == NULL || connection->connection == NULL || sql == NULL) {
    return TERLAN_LIBPQ_INVALID_ARGUMENT;
  }
  int sent = PQsendQueryParams(
      connection->connection,
      sql,
      connection->parameter_count,
      NULL,
      (const char *const *)connection->parameters,
      NULL,
      NULL,
      0);
  clear_parameters(connection);
  return sent == 1 ? TERLAN_LIBPQ_OK : TERLAN_LIBPQ_DRIVER_ERROR;
}

int32_t terlan_libpq_connection_send_batch(
    TerlanLibpqConnection *connection,
    const char *sql) {
  if (connection == NULL || connection->connection == NULL || sql == NULL) {
    return TERLAN_LIBPQ_INVALID_ARGUMENT;
  }
  clear_parameters(connection);
  return PQsendQuery(connection->connection, sql) == 1
             ? TERLAN_LIBPQ_OK
             : TERLAN_LIBPQ_DRIVER_ERROR;
}

int32_t terlan_libpq_connection_consume_input(TerlanLibpqConnection *connection) {
  if (connection == NULL || connection->connection == NULL) {
    return TERLAN_LIBPQ_INVALID_ARGUMENT;
  }
  return PQconsumeInput(connection->connection) == 1 ? TERLAN_LIBPQ_OK
                                                      : TERLAN_LIBPQ_DRIVER_ERROR;
}

int32_t terlan_libpq_connection_is_busy(
    const TerlanLibpqConnection *connection,
    bool *out_busy) {
  if (connection == NULL || connection->connection == NULL || out_busy == NULL) {
    return TERLAN_LIBPQ_INVALID_ARGUMENT;
  }
  *out_busy = PQisBusy(connection->connection) == 1;
  return TERLAN_LIBPQ_OK;
}

int32_t terlan_libpq_connection_next_result(
    TerlanLibpqConnection *connection,
    TerlanLibpqResult **out_result) {
  if (connection == NULL || connection->connection == NULL || out_result == NULL) {
    return TERLAN_LIBPQ_INVALID_ARGUMENT;
  }
  PGresult *result = PQgetResult(connection->connection);
  if (result == NULL) {
    return TERLAN_LIBPQ_NO_RESULT;
  }
  TerlanLibpqResult *wrapped =
      (TerlanLibpqResult *)calloc(1, sizeof(TerlanLibpqResult));
  if (wrapped == NULL) {
    PQclear(result);
    return TERLAN_LIBPQ_ALLOCATION_FAILED;
  }
  wrapped->result = result;
  *out_result = wrapped;
  return TERLAN_LIBPQ_OK;
}

int32_t terlan_libpq_connection_abort(TerlanLibpqConnection *connection) {
  if (connection == NULL) {
    return TERLAN_LIBPQ_INVALID_ARGUMENT;
  }
  if (connection->connection != NULL) {
    PQfinish(connection->connection);
    connection->connection = NULL;
  }
  return TERLAN_LIBPQ_OK;
}

int32_t terlan_libpq_result_destroy(TerlanLibpqResult *result) {
  if (result == NULL) {
    return TERLAN_LIBPQ_INVALID_ARGUMENT;
  }
  free(result->scratch);
  PQclear(result->result);
  free(result);
  return TERLAN_LIBPQ_OK;
}

int32_t terlan_libpq_result_status(
    const TerlanLibpqResult *result,
    int64_t *out_status) {
  if (result == NULL || result->result == NULL || out_status == NULL) {
    return TERLAN_LIBPQ_INVALID_ARGUMENT;
  }
  *out_status = (int64_t)PQresultStatus(result->result);
  return TERLAN_LIBPQ_OK;
}

int32_t terlan_libpq_result_row_count(
    const TerlanLibpqResult *result,
    int64_t *out_count) {
  if (result == NULL || result->result == NULL || out_count == NULL) {
    return TERLAN_LIBPQ_INVALID_ARGUMENT;
  }
  *out_count = (int64_t)PQntuples(result->result);
  return TERLAN_LIBPQ_OK;
}

int32_t terlan_libpq_result_column_count(
    const TerlanLibpqResult *result,
    int64_t *out_count) {
  if (result == NULL || result->result == NULL || out_count == NULL) {
    return TERLAN_LIBPQ_INVALID_ARGUMENT;
  }
  *out_count = (int64_t)PQnfields(result->result);
  return TERLAN_LIBPQ_OK;
}

int32_t terlan_libpq_result_select_column_name(
    TerlanLibpqResult *result,
    int64_t column) {
  if (result == NULL || result->result == NULL || column < 0 ||
      column >= PQnfields(result->result)) {
    return TERLAN_LIBPQ_INVALID_ARGUMENT;
  }
  const char *name = PQfname(result->result, (int)column);
  if (name == NULL) {
    return TERLAN_LIBPQ_DRIVER_ERROR;
  }
  result->selected_null = false;
  return copy_scratch(
      &result->scratch,
      &result->scratch_length,
      name,
      strlen(name));
}

int32_t terlan_libpq_result_column_oid(
    const TerlanLibpqResult *result,
    int64_t column,
    int64_t *out_oid) {
  if (result == NULL || result->result == NULL || column < 0 ||
      column >= PQnfields(result->result) || out_oid == NULL) {
    return TERLAN_LIBPQ_INVALID_ARGUMENT;
  }
  *out_oid = (int64_t)PQftype(result->result, (int)column);
  return TERLAN_LIBPQ_OK;
}

int32_t terlan_libpq_result_select_value(
    TerlanLibpqResult *result,
    int64_t row,
    int64_t column) {
  if (result == NULL || result->result == NULL || row < 0 || column < 0 ||
      row >= PQntuples(result->result) || column >= PQnfields(result->result)) {
    return TERLAN_LIBPQ_INVALID_ARGUMENT;
  }
  result->selected_null = PQgetisnull(result->result, (int)row, (int)column) == 1;
  if (result->selected_null) {
    return copy_scratch(&result->scratch, &result->scratch_length, NULL, 0);
  }
  return copy_scratch(
      &result->scratch,
      &result->scratch_length,
      PQgetvalue(result->result, (int)row, (int)column),
      (size_t)PQgetlength(result->result, (int)row, (int)column));
}

int32_t terlan_libpq_result_value_length(
    const TerlanLibpqResult *result,
    int64_t *out_length) {
  if (result == NULL || out_length == NULL) {
    return TERLAN_LIBPQ_INVALID_ARGUMENT;
  }
  *out_length = result->scratch_length;
  return TERLAN_LIBPQ_OK;
}

int32_t terlan_libpq_result_value_bytes(
    const TerlanLibpqResult *result,
    int64_t **out_bytes) {
  if (result == NULL || out_bytes == NULL) {
    return TERLAN_LIBPQ_INVALID_ARGUMENT;
  }
  *out_bytes = result->scratch;
  return TERLAN_LIBPQ_OK;
}

int32_t terlan_libpq_result_value_is_null(
    const TerlanLibpqResult *result,
    bool *out_null) {
  if (result == NULL || out_null == NULL) {
    return TERLAN_LIBPQ_INVALID_ARGUMENT;
  }
  *out_null = result->selected_null;
  return TERLAN_LIBPQ_OK;
}

int32_t terlan_libpq_result_affected_rows(
    const TerlanLibpqResult *result,
    int64_t *out_count) {
  if (result == NULL || result->result == NULL || out_count == NULL) {
    return TERLAN_LIBPQ_INVALID_ARGUMENT;
  }
  const char *text = PQcmdTuples(result->result);
  if (text == NULL || text[0] == '\0') {
    *out_count = 0;
    return TERLAN_LIBPQ_OK;
  }
  char *end = NULL;
  long long value = strtoll(text, &end, 10);
  if (end == text || *end != '\0') {
    return TERLAN_LIBPQ_DRIVER_ERROR;
  }
  *out_count = (int64_t)value;
  return TERLAN_LIBPQ_OK;
}
