-module(std_beam_nativebridge).

-moduledoc "BEAM SafeNative bridge contract.\n\n`std.beam.NativeBridge` connects BEAM supervision and message flow to\nRust-backed SafeNative workers. It is a BEAM target contract, not a portable\nruntime abstraction.".

-export([call/2, dispose/1, start/1, stop/1]).

-export_type([native_bridge/1]).

-doc "NativeBridge represents a supervised native worker handle.\n\nInput: native resource type `Resource`.\nOutput: an opaque BEAM-owned bridge handle.\nTransformation: separates BEAM process ownership from native resource\nownership while preserving a typed handle in Terlan source.".

-opaque native_bridge(_Resource) :: term().

%% trait NativeBridgeRuntime.
-doc "Starts a BEAM-owned native bridge worker.\n\nInput: one native resource descriptor of type `Resource`.\nOutput: `Result[NativeBridge[Resource], Error]`.\nTransformation: asks the BEAM runtime to attach a supervised SafeNative\nworker while keeping native ownership outside portable Terlan semantics.".

-spec start(resource()) -> std_core_result:result(native_bridge(resource()), std_core_error:error()).

start(Resource) ->
    Native.

-doc "Sends a synchronous command to a native bridge worker.\n\nInput: one `NativeBridge[Resource]` receiver and one command value.\nOutput: `Result[Reply, Error]`.\nTransformation: routes command execution through the BEAM/SafeNative bridge\nand normalizes adapter failures into the standard typed error channel.".

-spec call(native_bridge(resource()), command()) -> std_core_result:result(reply(), std_core_error:error()).

call(Bridge, Command) ->
    Native.

-doc "Disposes the native resource owned by a bridge worker.\n\nInput: one mutable `NativeBridge[Resource]` receiver.\nOutput: `Unit`.\nTransformation: releases target-owned native resources deterministically\nwithout exposing handle generations or disposal messages to Terlan source.".

-spec dispose(native_bridge(resource())) -> native_bridge(resource()).

dispose(Bridge) ->
    Native.

-doc "Stops the BEAM bridge process.\n\nInput: one mutable `NativeBridge[Resource]` receiver.\nOutput: `Unit`.\nTransformation: asks the BEAM runtime to shut down the bridge process after\nany required native disposal policy has run.".

-spec stop(native_bridge(resource())) -> native_bridge(resource()).

stop(Bridge) ->
    Native.

