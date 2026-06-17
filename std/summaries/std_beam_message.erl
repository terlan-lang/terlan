-module(std_beam_message).

-moduledoc "BEAM message contract.\n\n`std.beam.Message` models typed payloads that can cross BEAM process\nboundaries. It is a target-gated runtime contract and is not part of\nportable `std.core`.".

-export_type([message/1]).

-doc "Message represents one typed BEAM process message.\n\nInput: type parameter `T`.\nOutput: an opaque message wrapper owned by the BEAM target runtime.\nTransformation: keeps source-level message payloads typed while leaving the\nconcrete mailbox representation to the BEAM backend.".

-opaque message(_T) :: term().

%% trait MessageCodec.
