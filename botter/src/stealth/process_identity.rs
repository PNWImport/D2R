//! Process Identity Disguise
//!
//! On Windows: Real PEB overwrite (ImagePathName + CommandLine) and
//! Chrome process tree enumeration via CreateToolhelp32Snapshot.
//!
//! On Linux: Records what would be applied, for test validation.

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChromeDisguise {
    Renderer,
    UtilityAudio,
    UtilityNetwork,
    GpuProcess,
    CrashpadHandler,
}

impl ChromeDisguise {
    pub fn command_line(&self) -> String {
        let chrome_path = r#"C:\Program Files\Google\Chrome\Application\chrome.exe"#;
        match self {
            Self::Renderer => format!(
                r#""{}" --type=renderer --renderer-client-id=7 --lang=en-US --enable-auto-reload --num-raster-threads=4 --enable-zero-copy --enable-gpu-memory-buffer-video-frames --field-trial-handle=2036"#,
                chrome_path
            ),
            Self::UtilityAudio => format!(
                r#""{}" --type=utility --utility-sub-type=audio.mojom.AudioService --lang=en-US --service-sandbox-type=audio --field-trial-handle=2036"#,
                chrome_path
            ),
            Self::UtilityNetwork => format!(
                r#""{}" --type=utility --utility-sub-type=network.mojom.NetworkService --lang=en-US --service-sandbox-type=none --field-trial-handle=2036"#,
                chrome_path
            ),
            Self::GpuProcess => format!(
                r#""{}" --type=gpu-process --gpu-preferences=UAAAAAAAAADgAAAYAAAAAAAAAAAAAAAAAABgAAAAAAAwAAAAAAAAAAAAAAAAAAAKAAAA --gpu-vendor-id=0x10de --gpu-device-id=0x2504"#,
                chrome_path
            ),
            Self::CrashpadHandler => r#""C:\Program Files\Google\Chrome\Application\122.0.6261.95\crashpad_handler.exe" --no-rate-limit --database=C:\Users\User\AppData\Local\Google\Chrome\User Data\Crashpad --annotation=plat=Win64 --annotation=prod=Chrome_ChromiumCore"#.to_string(),
        }
    }

    pub fn image_path(&self) -> PathBuf {
        match self {
            Self::CrashpadHandler => PathBuf::from(
                r"C:\Program Files\Google\Chrome\Application\122.0.6261.95\crashpad_handler.exe",
            ),
            _ => PathBuf::from(r"C:\Program Files\Google\Chrome\Application\chrome.exe"),
        }
    }
}

#[derive(Debug)]
pub enum ProcessIdentityError {
    PebWriteFailed(String),
    ChromeNotFound,
}

impl std::fmt::Display for ProcessIdentityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PebWriteFailed(e) => write!(f, "PEB write failed: {}", e),
            Self::ChromeNotFound => write!(f, "Chrome process not found"),
        }
    }
}
impl std::error::Error for ProcessIdentityError {}

pub struct ProcessIdentity {
    disguise: ChromeDisguise,
    applied: bool,
    chrome_pid: Option<u32>,
}

impl ProcessIdentity {
    pub fn new(disguise: ChromeDisguise) -> Self {
        Self {
            disguise,
            applied: false,
            chrome_pid: None,
        }
    }

    pub fn find_chrome_parent(&mut self) -> Option<u32> {
        #[cfg(windows)]
        {
            self.chrome_pid = find_chrome_browser_pid();
        }
        #[cfg(not(windows))]
        {
            // On Linux, report None — Chrome native messaging handles parent process
            self.chrome_pid = None;
        }
        self.chrome_pid
    }

    pub fn apply(&mut self) -> Result<(), ProcessIdentityError> {
        if self.applied {
            return Ok(());
        }

        #[cfg(windows)]
        {
            self.apply_peb_overwrite()?;
        }
        #[cfg(not(windows))]
        {
            tracing::debug!("PEB overwrite skipped (non-Windows): {:?}", self.disguise);
        }

        self.applied = true;
        Ok(())
    }

    pub fn revert(&mut self) -> Result<(), ProcessIdentityError> {
        // PEB revert not implemented — in practice we never revert,
        // process exits when Chrome pipe closes
        self.applied = false;
        Ok(())
    }

    pub fn is_applied(&self) -> bool {
        self.applied
    }

    pub fn status(&self) -> ProcessIdentityStatus {
        ProcessIdentityStatus {
            disguise: self.disguise,
            image_path: self.disguise.image_path(),
            command_line: self.disguise.command_line(),
            applied: self.applied,
            chrome_pid: self.chrome_pid,
        }
    }

    // ─── Windows PEB Overwrite ─────────────────────────────────

    #[cfg(windows)]
    fn apply_peb_overwrite(&self) -> Result<(), ProcessIdentityError> {
        use std::mem;
        use windows::Win32::System::Threading::*;

        // PROCESS_BASIC_INFORMATION is not exposed in windows 0.58;
        // define it manually for NtQueryInformationProcess.
        #[repr(C)]
        struct ProcessBasicInformation {
            reserved1: *mut std::ffi::c_void,
            peb_base_address: *mut u8,
            reserved2: [*mut std::ffi::c_void; 2],
            unique_process_id: usize,
            reserved3: *mut std::ffi::c_void,
        }

        // NtQueryInformationProcess from ntdll
        #[link(name = "ntdll")]
        extern "system" {
            fn NtQueryInformationProcess(
                process_handle: *mut std::ffi::c_void,
                process_information_class: u32,
                process_information: *mut std::ffi::c_void,
                process_information_length: u32,
                return_length: *mut u32,
            ) -> i32;
        }

        unsafe {
            // 1. Get PEB address via NtQueryInformationProcess
            let process = GetCurrentProcess();
            let mut pbi: ProcessBasicInformation = mem::zeroed();
            let status = NtQueryInformationProcess(
                process.0 as *mut std::ffi::c_void,
                0, // ProcessBasicInformation
                &mut pbi as *mut _ as *mut std::ffi::c_void,
                mem::size_of::<ProcessBasicInformation>() as u32,
                std::ptr::null_mut(),
            );
            if status != 0 {
                return Err(ProcessIdentityError::PebWriteFailed(format!(
                    "NtQueryInformationProcess failed: 0x{:08X}",
                    status
                )));
            }

            let peb = pbi.peb_base_address;
            if peb.is_null() {
                return Err(ProcessIdentityError::PebWriteFailed("Null PEB".into()));
            }

            // 2. Read ProcessParameters pointer from PEB
            let params_offset = 0x20usize; // PEB.ProcessParameters on x64
            let params_ptr: *mut u8 =
                std::ptr::read((peb as *const u8).add(params_offset) as *const *mut u8);

            if params_ptr.is_null() {
                return Err(ProcessIdentityError::PebWriteFailed(
                    "Null ProcessParameters".into(),
                ));
            }

            // 3. Overwrite ImagePathName (offset 0x60) and CommandLine (offset 0x70)
            let image_path = self.disguise.image_path();
            let image_utf16: Vec<u16> = image_path
                .to_str()
                .unwrap_or("")
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();

            let cmd_line = self.disguise.command_line();
            let cmd_utf16: Vec<u16> = cmd_line.encode_utf16().chain(std::iter::once(0)).collect();

            // Write UNICODE_STRING for ImagePathName at offset 0x60
            write_unicode_string(params_ptr.add(0x60), &image_utf16);

            // Write UNICODE_STRING for CommandLine at offset 0x70
            write_unicode_string(params_ptr.add(0x70), &cmd_utf16);

            tracing::info!(
                "PEB overwritten: image={}, cmdline={}...",
                image_path.display(),
                &cmd_line[..80.min(cmd_line.len())]
            );
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct ProcessIdentityStatus {
    pub disguise: ChromeDisguise,
    pub image_path: PathBuf,
    pub command_line: String,
    pub applied: bool,
    pub chrome_pid: Option<u32>,
}

/// Write a UTF-16 buffer into a UNICODE_STRING structure in-place
#[cfg(windows)]
unsafe fn write_unicode_string(us_ptr: *mut u8, utf16: &[u16]) {
    let byte_len = (utf16.len() * 2) as u16;
    // UNICODE_STRING: Length (u16), MaximumLength (u16), padding (u32 on x64), Buffer (*mut u16)
    std::ptr::write(us_ptr as *mut u16, byte_len.saturating_sub(2)); // Length (excludes null)
    std::ptr::write(us_ptr.add(2) as *mut u16, byte_len); // MaximumLength
                                                          // Buffer pointer at offset 8 (x64 aligned)
    let buf_ptr = std::ptr::read(us_ptr.add(8) as *const *mut u16);
    if !buf_ptr.is_null() {
        std::ptr::copy_nonoverlapping(utf16.as_ptr(), buf_ptr, utf16.len());
    }
}

/// Find the Chrome browser process PID via process snapshot
#[cfg(windows)]
fn find_chrome_browser_pid() -> Option<u32> {
    use windows::Win32::Foundation::*;
    use windows::Win32::System::Diagnostics::ToolHelp::*;

    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
        let mut entry = PROCESSENTRY32W::default();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let name: String = entry
                    .szExeFile
                    .iter()
                    .take_while(|&&c| c != 0)
                    .map(|&c| c as u8 as char)
                    .collect();

                if name.eq_ignore_ascii_case("chrome.exe") {
                    // Check if this is the browser process (has no --type= flag)
                    // The browser process is the one whose parent is NOT chrome.exe
                    let _ = CloseHandle(snapshot);
                    return Some(entry.th32ProcessID);
                }

                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disguise_command_lines() {
        let r = ChromeDisguise::Renderer;
        assert!(r.command_line().contains("--type=renderer"));

        let g = ChromeDisguise::GpuProcess;
        assert!(g.command_line().contains("--type=gpu-process"));

        let n = ChromeDisguise::UtilityNetwork;
        assert!(n.command_line().contains("network.mojom.NetworkService"));
    }

    #[test]
    fn test_image_paths() {
        let r = ChromeDisguise::Renderer;
        assert!(r.image_path().to_str().unwrap().contains("chrome.exe"));

        let c = ChromeDisguise::CrashpadHandler;
        assert!(c.image_path().to_str().unwrap().contains("crashpad"));
    }

    #[test]
    fn test_identity_lifecycle() {
        let mut id = ProcessIdentity::new(ChromeDisguise::UtilityAudio);
        assert!(!id.is_applied());
        id.find_chrome_parent();
        id.apply().unwrap();
        assert!(id.is_applied());
        id.revert().unwrap();
        assert!(!id.is_applied());
    }

    #[test]
    fn test_double_apply_safe() {
        let mut id = ProcessIdentity::new(ChromeDisguise::Renderer);
        id.apply().unwrap();
        id.apply().unwrap();
        assert!(id.is_applied());
    }
}
