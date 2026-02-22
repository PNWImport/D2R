pub mod capture_timing;
pub mod handle_table;
pub mod process_identity;
pub mod syscall_cadence;
pub mod thread_input;

pub use capture_timing::{CaptureAction, CaptureMode, CaptureTiming, CaptureTimingConfig};
pub use handle_table::{HandleManager, HandleState, ManagedHandle};
pub use process_identity::{ChromeDisguise, ProcessIdentity, ProcessIdentityError, ProcessIdentityStatus};
pub use syscall_cadence::{CadenceConfig, SyscallCadence, SyscallCategory, SyscallPrep};
pub use thread_input::{
    InputCommand, MouseButton, RotationStrategy, ThreadPoolConfig, ThreadRotatedInput,
};
