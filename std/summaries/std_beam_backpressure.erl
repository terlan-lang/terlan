-module(std_beam_backpressure).

-moduledoc "BEAM backpressure contract.\n\n`std.beam.Backpressure` defines the credit accounting surface used by\nsupervised native bridges. The implementation is target-owned; Terlan source\nonly depends on the typed trait contract.".

-export_type([credit/0]).

-doc "Credit represents available work capacity.\n\nInput: no type parameters.\nOutput: an integer-backed credit count.\nTransformation: gives BEAM/native bridge contracts a named type alias for\ncapacity accounting without exposing scheduler internals.".

-type credit() :: integer().

%% trait Backpressure.
