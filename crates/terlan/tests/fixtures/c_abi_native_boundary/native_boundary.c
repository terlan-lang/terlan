#include "include/native_boundary.h"

#include <stdlib.h>
#include <string.h>

struct TerlanCNativeBoundary {
  int64_t value;
  int64_t samples[2];
};

static int64_t live_count = 0;

TerlanCStatus terlan_c_native_boundary_create(
    int64_t value,
    TerlanCNativeBoundary** out_boundary) {
  if (out_boundary == NULL) {
    return TERLAN_C_STATUS_INVALID_ARGUMENT;
  }
  TerlanCNativeBoundary* boundary =
      (TerlanCNativeBoundary*)malloc(sizeof(TerlanCNativeBoundary));
  if (boundary == NULL) {
    return TERLAN_C_STATUS_INVALID_ARGUMENT;
  }
  boundary->value = value;
  boundary->samples[0] = value;
  boundary->samples[1] = value * 2;
  *out_boundary = boundary;
  ++live_count;
  return TERLAN_C_STATUS_OK;
}

TerlanCStatus terlan_c_native_boundary_sample_count(
    const TerlanCNativeBoundary* boundary,
    int64_t* out_count) {
  if (boundary == NULL) {
    return TERLAN_C_STATUS_NULL_HANDLE;
  }
  if (out_count == NULL) {
    return TERLAN_C_STATUS_INVALID_ARGUMENT;
  }
  *out_count = 2;
  return TERLAN_C_STATUS_OK;
}

TerlanCStatus terlan_c_native_boundary_samples(
    const TerlanCNativeBoundary* boundary,
    int64_t** out_samples) {
  if (boundary == NULL) {
    return TERLAN_C_STATUS_NULL_HANDLE;
  }
  if (out_samples == NULL) {
    return TERLAN_C_STATUS_INVALID_ARGUMENT;
  }
  *out_samples = (int64_t*)boundary->samples;
  return TERLAN_C_STATUS_OK;
}

TerlanCStatus terlan_c_native_boundary_value(
    const TerlanCNativeBoundary* boundary,
    int64_t* out_value) {
  if (boundary == NULL) {
    return TERLAN_C_STATUS_NULL_HANDLE;
  }
  if (out_value == NULL) {
    return TERLAN_C_STATUS_INVALID_ARGUMENT;
  }
  *out_value = boundary->value;
  return TERLAN_C_STATUS_OK;
}

TerlanCStatus terlan_c_native_boundary_add(
    TerlanCNativeBoundary* boundary,
    int64_t delta) {
  if (boundary == NULL) {
    return TERLAN_C_STATUS_NULL_HANDLE;
  }
  boundary->value += delta;
  boundary->samples[0] = boundary->value;
  boundary->samples[1] = boundary->value * 2;
  return TERLAN_C_STATUS_OK;
}

TerlanCStatus terlan_c_native_boundary_destroy(
    TerlanCNativeBoundary* boundary) {
  if (boundary == NULL) {
    return TERLAN_C_STATUS_NULL_HANDLE;
  }
  free(boundary);
  --live_count;
  return TERLAN_C_STATUS_OK;
}

TerlanCStatus terlan_c_native_boundary_duplicate_handle(
    const TerlanCNativeBoundary* boundary,
    TerlanCNativeBoundary** out_boundary) {
  if (boundary == NULL) {
    return TERLAN_C_STATUS_NULL_HANDLE;
  }
  return terlan_c_native_boundary_create(boundary->value, out_boundary);
}

TerlanCStatus terlan_c_call_dispatcher(
    const char* operator_name,
    const char* overload_name,
    StableIValue* stack,
    uint64_t extension_abi_version) {
  if (operator_name == NULL || overload_name == NULL || stack == NULL ||
      extension_abi_version != UINT64_C(0x0001000000000000) ||
      overload_name[0] != '\0' || stack[0] == 0) {
    return TERLAN_C_STATUS_INVALID_ARGUMENT;
  }
  if (strcmp(operator_name, "fixture::clone") == 0) {
    return stack[1] == 0 ? TERLAN_C_STATUS_OK
                         : TERLAN_C_STATUS_INVALID_ARGUMENT;
  }
  if (strcmp(operator_name, "fixture::unsqueeze") == 0) {
    return stack[1] == UINT64_MAX ? TERLAN_C_STATUS_OK
                                  : TERLAN_C_STATUS_INVALID_ARGUMENT;
  }
  if (strcmp(operator_name, "fixture::matmul") == 0 && stack[1] != 0) {
    TerlanCNativeBoundary* left = (TerlanCNativeBoundary*)(uintptr_t)stack[0];
    TerlanCNativeBoundary* right = (TerlanCNativeBoundary*)(uintptr_t)stack[1];
    TerlanCNativeBoundary* result = NULL;
    TerlanCStatus status =
        terlan_c_native_boundary_create(left->value * right->value, &result);
    terlan_c_native_boundary_destroy(left);
    terlan_c_native_boundary_destroy(right);
    if (status == TERLAN_C_STATUS_OK) {
      stack[0] = (StableIValue)(uintptr_t)result;
    }
    return status;
  }
  return TERLAN_C_STATUS_INVALID_ARGUMENT;
}

int64_t terlan_c_native_boundary_live_count(void) { return live_count; }
