-module(std_beam_process).

-moduledoc "BEAM process contract.\n\n`std.beam.Process` models a typed process handle for BEAM-targeted runtime\nmodules. It exists so supervision and native bridges can be expressed as\nordinary annotated types and traits instead of source-level actor syntax.".

-export_type([process/1]).

-doc "Process represents a typed BEAM process handle.\n\nInput: type parameter `T` describing accepted message payloads.\nOutput: an opaque BEAM process handle.\nTransformation: keeps process identity target-owned while preserving a typed\nsource contract for message routing and supervision.".

-opaque process(_T) :: term().

%% trait ProcessLike.
