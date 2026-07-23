#[allow(dead_code, unused_imports)]
#[path = "../runtime/vm/actor_impl.rs"]
pub(crate) mod actor;
#[allow(dead_code)]
#[path = "../runtime/vm/bitstring.rs"]
pub(crate) mod bitstring;
#[path = "../runtime/vm/call_count.rs"]
pub(crate) mod call_count;
#[path = "../runtime/vm/call_memory.rs"]
pub(crate) mod call_memory;
#[path = "../runtime/vm/call_time.rs"]
pub(crate) mod call_time;
#[allow(dead_code)]
#[path = "../runtime/vm/code_server.rs"]
pub(crate) mod code_server;
#[path = "../runtime/vm/dynamic_module.rs"]
pub(crate) mod dynamic_module;
#[path = "../runtime/vm/failure.rs"]
pub(crate) mod failure;
#[path = "../runtime/vm/fatal_diagnostics.rs"]
pub(crate) mod fatal_diagnostics;
#[allow(dead_code)]
#[path = "../runtime/vm/local_trace.rs"]
pub(crate) mod local_trace;
#[path = "../runtime/vm/map_layout.rs"]
pub(crate) mod map_layout;
#[allow(dead_code)]
#[path = "../runtime/vm/map_value.rs"]
pub(crate) mod map_value;
#[allow(dead_code)]
#[path = "../runtime/vm/value.rs"]
mod value;
pub(crate) use value::ReplValue;
#[path = "../runtime/vm/actor_directory.rs"]
pub(crate) mod actor_directory;
#[path = "../runtime/vm/memory.rs"]
pub(crate) mod memory;
#[allow(dead_code)]
#[path = "../runtime/vm/meta_trace.rs"]
pub(crate) mod meta_trace;
#[allow(dead_code)]
#[path = "vm_native_boundary.rs"]
pub(crate) mod native_boundary;
#[allow(dead_code)]
#[path = "../runtime/vm/native_image_diagnostics.rs"]
pub(crate) mod native_image_diagnostics;
#[path = "../runtime/vm/postgres.rs"]
pub(crate) mod postgres;
#[path = "../runtime/vm/process.rs"]
pub(crate) mod process;
#[path = "../runtime/vm/process_alias.rs"]
pub(crate) mod process_alias;
#[path = "../runtime/vm/process_environment.rs"]
pub(crate) mod process_environment;
#[path = "../runtime/vm/reference.rs"]
pub(crate) mod reference;
#[path = "../runtime/vm/resource.rs"]
pub(crate) mod resource;
#[path = "../runtime/vm/scheduler.rs"]
pub(crate) mod scheduler;
#[allow(dead_code)]
#[path = "../runtime/vm/scheduler_topology.rs"]
pub(crate) mod scheduler_topology;
#[path = "../runtime/vm/statistics.rs"]
pub(crate) mod statistics;
#[path = "../runtime/vm/system_information.rs"]
pub(crate) mod system_information;
#[allow(dead_code)]
#[path = "../runtime/vm/system_profile.rs"]
pub(crate) mod system_profile;
#[path = "../runtime/vm/table.rs"]
pub(crate) mod table;
#[path = "../runtime/vm/timer.rs"]
pub(crate) mod timer;
