/// Bucket utilities: detect which processes from a bucket are currently installed.

use crate::config::Bucket;

/// Check which processes from a bucket are likely installed.
/// Uses running process snapshot as the primary signal.
#[cfg(target_os = "windows")]
#[allow(dead_code)]
pub fn filter_installed(bucket: &Bucket) -> Vec<String> {
    let running = get_running_processes();
    bucket
        .processes
        .iter()
        .filter(|p| {
            let lower = p.to_lowercase();
            running.iter().any(|r| r.to_lowercase() == lower)
        })
        .cloned()
        .collect()
}

#[cfg(target_os = "windows")]
#[allow(dead_code)]
fn get_running_processes() -> Vec<String> {
    use windows::Win32::System::ProcessStatus::EnumProcesses;
    use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION};
    use windows::core::PWSTR;

    let mut pids = vec![0u32; 4096];
    let mut bytes_returned: u32 = 0;

    unsafe {
        if EnumProcesses(pids.as_mut_ptr(), (pids.len() * 4) as u32, &mut bytes_returned).is_err() {
            return Vec::new();
        }
    }

    let count = bytes_returned as usize / 4;
    let mut names = Vec::new();

    for &pid in &pids[..count] {
        if pid == 0 {
            continue;
        }
        unsafe {
            if let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                let mut buf = [0u16; 1024];
                let mut size = buf.len() as u32;
                let pwstr = PWSTR(buf.as_mut_ptr());
                if QueryFullProcessImageNameW(handle, PROCESS_NAME_FORMAT(0), pwstr, &mut size).is_ok() {
                    let path = String::from_utf16_lossy(&buf[..size as usize]);
                    if let Some(name) = path.rsplit('\\').next() {
                        names.push(name.to_string());
                    }
                }
            }
        }
    }

    names.sort();
    names.dedup();
    names
}

/// Stub for non-Windows platforms.
#[cfg(not(target_os = "windows"))]
#[allow(dead_code)]
pub fn filter_installed(bucket: &Bucket) -> Vec<String> {
    // Return all processes as "installed" for non-Windows dev/test
    bucket.processes.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Action, Bucket};

    #[test]
    fn test_filter_installed_returns_subset() {
        // On non-Windows, returns all
        let bucket = Bucket {
            name: "Test".into(),
            processes: vec!["chrome.exe".into(), "firefox.exe".into()],
            timeout_mins: 15,
            action: Action::Minimize,
            enabled: true,
        };

        let result = filter_installed(&bucket);
        // On non-Windows this returns all; on Windows it returns only running ones
        assert!(!result.is_empty());
    }
}
