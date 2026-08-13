#![forbid(unsafe_code)]

//! Stable types shared by Terlan AOT images, the VM, and capability workers.

mod boundary_error;
mod boundary_type;

pub use boundary_error::{BoundaryError, ErrorDomain};
pub use boundary_type::TvmBoundaryType;
