#![allow(dead_code)]
//! Process Identity Disguise for Map Helper
//!
//! On Windows: PEB overwrite (ImagePathName + CommandLine) so the map helper
//! reports as a Chrome utility process (NetworkService) when enumerated.
//!
//! On Linux: No-op for dev/test builds — records what would be applied.

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChromeDisguise {
    UtilityNetwork,
    Renderer,
    GpuProcess,
}

impl ChromeDisguise {
    pub fn command_line(&self) -> String {
        let chrome_path = r#"C:\Program Files\Google\Chrome\Application\chrome.exe"#;
        match self {
            Self::UtilityNetwork => format!(
                r#""{}" --type=utility --utility-sub-type=network.mojom.NetworkService --lang=en-US --service-sandbox-type=none --field-trial-handle=2036"#,
                chrome_path
            ),
            Self::Renderer => format!(
                r#""{}" --type=renderer --renderer-client-id=7 --lang=en-US --enable-auto-reload --num-raster-threads=4 --enable-zero-copy --field-trial-handle=2036"#,
                chrome_path
            ),
            Self::GpuProcess => format!(
                r#""{}" --type=gpu-process --gpu-preferences=UAAAAAAAAADgAAAYAAAAAAAAAAAAAAAAAABgAAAAAAAwAAAAAAAAAAAAAAAAAAAKAAAA --gpu-vendor-id=0x10de --gpu-device-id=0x2504"#,
                chrome_path
            ),
        }
    }

    pub fn image_path(&self) -> PathBuf {
        PathBuf::from(r"C:\Program Files\Google\Chrome\Application\chrome.exe")
    }
}

pub struct ProcessIdentity {
    disguise: ChromeDisguise,
    applied: bool,
}

impl ProcessIdentity {
    pub fn new(disguise: ChromeDisguise) -> Self {
        Self {
            disguise,
            applied: false,
        }
    }

    pub fn apply(&mut self) -> Result<(), String> {
        if self.applied {
            return Ok(());
        }

        #[cfg(windows)]
        {
            self.apply_peb_overwrite()?;
        }
        #[cfg(not(windows))]
        {
            eprintln!(
                "[stealth] PEB overwrite skipped (non-Windows): {:?}",
                self.disguise
            );
        }

        self.applied = true;
        Ok(())
    }

    pub fn is_applied(&self) -> bool {
        self.applied
    }

    // ─── Windows PEB Overwrite ─────────────────────────────────

    #[cfg(windows)]
    fn apply_peb_overwrite(&self) -> Result<(), String> {
        use std::mem;
        use std::ptr;
        use winapi::um::processthreadsapi::GetCurrentProcess;
        use winapi::um::winnt::PROCESS_BASIC_INFORMATION;

        // NtQueryInformationProcess is not in winapi by default,
        // so we load it dynamically from ntdll.dll
        type NtQueryInfoProc = unsafe extern "system" fn(
            winapi::um::winnt::HANDLE,
            u32, // PROCESSINFOCLASS
            *mut std::ffi::c_void,
            u32,
            *mut u32,
        ) -> i32;

        unsafe {
            // 1. Load NtQueryInformationProcess from ntdll
            let ntdll = winapi::um::libloaderapi::GetModuleHandleW(
                wide_str("ntdll.dll").as_ptr()
            );
            if ntdll.is_null() {
                return Err("Failed to get ntdll handle".into());
            }

            let proc_addr = winapi::um::libloaderapi::GetProcAddress(
                ntdll,
                b"NtQueryInformationProcess\0".as_ptr() as *const i8,
            );
            if proc_addr.is_null() {
                return Err("NtQueryInformationProcess not found".into());
            }

            let nt_query: NtQueryInfoProc = mem::transmute(proc_addr);

            // 2. Get PEB address
            let process = GetCurrentProcess();
            let mut pbi: PROCESS_BASIC_INFORMATION = mem::zeroed();
            let status = nt_query(
                process,
                0, // ProcessBasicInformation
                &mut pbi as *mut _ as *mut _,
                mem::size_of::<PROCESS_BASIC_INFORMATION>() as u32,
                ptr::null_mut(),
            );
            if status != 0 {
                return Err(format!("NtQueryInformationProcess failed: {:#X}", status));
            }

            let peb = pbi.PebBaseAddress;
            if peb.is_null() {
                return Err("Null PEB".into());
            }

            // 3. Read ProcessParameters pointer from PEB (+0x20 on x64)
            let params_ptr: *mut u8 = ptr::read(
                (peb as *const u8).add(0x20) as *const *mut u8
            );
            if params_ptr.is_null() {
                return Err("Null ProcessParameters".into());
            }

            // 4. Overwrite ImagePathName (+0x60) and CommandLine (+0x70)
            let image_path = self.disguise.image_path();
            let image_utf16: Vec<u16> = image_path
                .to_str()
                .unwrap_or("")
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();

            let cmd_line = self.disguise.command_line();
            let cmd_utf16: Vec<u16> = cmd_line
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();

            write_unicode_string(params_ptr.add(0x60), &image_utf16);
            write_unicode_string(params_ptr.add(0x70), &cmd_utf16);

            eprintln!(
                "[stealth] PEB overwritten: image={}, cmdline={}...",
                image_path.display(),
                &cmd_line[..80.min(cmd_line.len())]
            );
        }

        Ok(())
    }
}

/// Write a UTF-16 buffer into a UNICODE_STRING structure in-place
#[cfg(windows)]
unsafe fn write_unicode_string(us_ptr: *mut u8, utf16: &[u16]) {
    let byte_len = (utf16.len() * 2) as u16;
    // UNICODE_STRING layout on x64:
    //   +0x00: Length (u16)
    //   +0x02: MaximumLength (u16)
    //   +0x08: Buffer (*mut u16) — 8-byte aligned
    std::ptr::write(us_ptr as *mut u16, byte_len.saturating_sub(2)); // Length (excludes null)
    std::ptr::write(us_ptr.add(2) as *mut u16, byte_len);           // MaximumLength
    // Buffer pointer at offset 8 (x64)
    let buf_ptr = std::ptr::read(us_ptr.add(8) as *const *mut u16);
    if !buf_ptr.is_null() {
        std::ptr::copy_nonoverlapping(utf16.as_ptr(), buf_ptr, utf16.len());
    }
}

/// Convert a &str to a null-terminated wide string
#[cfg(windows)]
fn wide_str(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disguise_command_lines() {
        let n = ChromeDisguise::UtilityNetwork;
        assert!(n.command_line().contains("network.mojom.NetworkService"));

        let r = ChromeDisguise::Renderer;
        assert!(r.command_line().contains("--type=renderer"));

        let g = ChromeDisguise::GpuProcess;
        assert!(g.command_line().contains("--type=gpu-process"));
    }

    #[test]
    fn test_image_paths() {
        let n = ChromeDisguise::UtilityNetwork;
        assert!(n.image_path().to_str().unwrap().contains("chrome.exe"));
    }

    #[test]
    fn test_identity_lifecycle() {
        let mut id = ProcessIdentity::new(ChromeDisguise::UtilityNetwork);
        assert!(!id.is_applied());
        id.apply().unwrap();
        assert!(id.is_applied());
        // Double-apply is safe
        id.apply().unwrap();
        assert!(id.is_applied());
    }
}
