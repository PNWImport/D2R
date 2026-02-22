pub mod process_identity;
pub mod syscall_cadence;

pub use process_identity::{ChromeDisguise, ProcessIdentity};
pub use syscall_cadence::{CadenceConfig, SyscallCadence, SyscallCategory};
