-module(std_beam_supervisor).

-moduledoc "BEAM supervisor contract.\n\n`std.beam.Supervisor` models supervised runtime ownership for BEAM-targeted\nservices and native bridges. It is deliberately expressed as ordinary types\nand traits so supervision stays outside the Terlan grammar.".

-export([child_spec/1, start/2, stop/2]).

-export_type([child_spec/1, supervisor/0]).

-doc "Supervisor represents a BEAM supervision owner.\n\nInput: no type parameters.\nOutput: an opaque supervisor handle.\nTransformation: hides BEAM supervision tree identity behind a typed target\ncapability surface.".

-opaque supervisor() :: term().

-doc "ChildSpec describes a supervised child declaration.\n\nInput: child process or resource type `P`.\nOutput: an opaque child specification.\nTransformation: carries target-owned child startup metadata without exposing\nBEAM-specific tuple formats to Terlan source.".

-opaque child_spec(_P) :: term().

%% trait Supervised.
-doc "Builds a child specification for a supervised value.\n\nInput: one process or resource value of type `P`.\nOutput: `ChildSpec[P]`.\nTransformation: asks the BEAM runtime adapter to describe how the value\nshould enter supervision without exposing backend tuple formats.".

-spec child_spec(P) -> child_spec(P).

child_spec(Value) ->
    Native.

-doc "Starts a child under a supervisor.\n\nInput: one `Supervisor` receiver and one child specification.\nOutput: `Result[P, Error]`.\nTransformation: delegates startup to the target supervisor and normalizes\nstartup failure into the standard typed error channel.".

-spec start(supervisor(), child_spec(P)) -> std_core_result:result(P, std_core_error:error()).

start(Supervisor, Spec) ->
    Native.

-doc "Stops a supervised child.\n\nInput: one mutable `Supervisor` receiver and one supervised child value.\nOutput: `Unit`.\nTransformation: asks the target supervisor to stop the child while hiding\nBEAM shutdown and unlink details from Terlan source.".

-spec stop(supervisor(), _P) -> supervisor().

stop(Supervisor, Value) ->
    Native.

