#ifndef TERLAN_C_ABI_NATIVE_BOUNDARY_H
#define TERLAN_C_ABI_NATIVE_BOUNDARY_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define TERLAN_C_ABI_VERSION 1

typedef struct TerlanCNativeBoundary TerlanCNativeBoundary;

typedef int32_t TerlanCStatus;
typedef uint64_t StableIValue;
enum {
  TERLAN_C_STATUS_OK = 0,
  TERLAN_C_STATUS_INVALID_ARGUMENT = 1,
  TERLAN_C_STATUS_NULL_HANDLE = 2
};

TerlanCStatus terlan_c_native_boundary_create(
    int64_t value,
    TerlanCNativeBoundary** out_boundary);
TerlanCStatus terlan_c_native_boundary_value(
    const TerlanCNativeBoundary* boundary,
    int64_t* out_value);
TerlanCStatus terlan_c_native_boundary_sample_count(
    const TerlanCNativeBoundary* boundary,
    int64_t* out_count);
TerlanCStatus terlan_c_native_boundary_samples(
    const TerlanCNativeBoundary* boundary,
    int64_t** out_samples);
TerlanCStatus terlan_c_native_boundary_add(
    TerlanCNativeBoundary* boundary,
    int64_t delta);
TerlanCStatus terlan_c_native_boundary_destroy(
    TerlanCNativeBoundary* boundary);
TerlanCStatus terlan_c_native_boundary_duplicate_handle(
    const TerlanCNativeBoundary* boundary,
    TerlanCNativeBoundary** out_boundary);
TerlanCStatus terlan_c_call_dispatcher(
    const char* operator_name,
    const char* overload_name,
    StableIValue* stack,
    uint64_t extension_abi_version);
int64_t terlan_c_native_boundary_live_count(void);

#ifdef __cplusplus
}
#endif

#endif
