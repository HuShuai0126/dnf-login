use anyhow::Result;
#[cfg(target_os = "windows")]
use windows::{
    Win32::Foundation::*, Win32::NetworkManagement::IpHelper::*,
    Win32::System::Diagnostics::Debug::WriteProcessMemory, Win32::System::Diagnostics::ToolHelp::*,
    Win32::System::LibraryLoader::*, Win32::System::Memory::*, Win32::System::Threading::*,
};

#[cfg(target_os = "windows")]
pub fn get_mac_address() -> Result<String> {
    unsafe {
        let mut size: u32 = 0;
        let _ = GetAdaptersInfo(None, &mut size);

        if size == 0 {
            anyhow::bail!("No network adapters found");
        }

        let mut buffer = vec![0u8; size as usize];
        let adapter_info = buffer.as_mut_ptr() as *mut IP_ADAPTER_INFO;

        let result = GetAdaptersInfo(Some(adapter_info), &mut size);
        if result != NO_ERROR.0 {
            anyhow::bail!("Failed to get adapter info: {}", result);
        }

        // Find the first non-loopback adapter
        let mut current = adapter_info;
        while !current.is_null() {
            let adapter = &*current;

            if adapter.Type == 24 {
                // MIB_IF_TYPE_LOOPBACK
                current = adapter.Next;
                continue;
            }

            let mac_bytes = &adapter.Address[..adapter.AddressLength as usize];
            let mac_str = mac_bytes
                .iter()
                .map(|b| format!("{:02X}", b))
                .collect::<Vec<_>>()
                .join("-");

            return Ok(mac_str);
        }

        anyhow::bail!("No suitable network adapter found");
    }
}

#[cfg(target_os = "windows")]
pub fn is_process_running(process_name: &str) -> Result<bool> {
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)?;

        if snapshot.is_invalid() {
            anyhow::bail!("Failed to create process snapshot");
        }

        let mut pe32 = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        if Process32FirstW(snapshot, &mut pe32).is_err() {
            let _ = CloseHandle(snapshot);
            anyhow::bail!("Failed to get first process");
        }

        let target_name = process_name.to_lowercase();

        loop {
            let exe_file = String::from_utf16_lossy(
                &pe32.szExeFile[..pe32.szExeFile.iter().position(|&c| c == 0).unwrap_or(0)],
            );

            if exe_file.to_lowercase() == target_name {
                let _ = CloseHandle(snapshot);
                return Ok(true);
            }

            if Process32NextW(snapshot, &mut pe32).is_err() {
                break;
            }
        }

        let _ = CloseHandle(snapshot);
        Ok(false)
    }
}

#[cfg(target_os = "windows")]
pub fn launch_dnf(token: &str, plugins_dir: &str) -> Result<()> {
    use std::process::Command;

    let dnf_path = std::env::current_dir()?.join("DNF.exe");
    if !dnf_path.exists() {
        anyhow::bail!(
            "DNF.exe not found. Please place the launcher in the game directory.\nExpected path: {}",
            dnf_path.display()
        );
    }

    if is_process_running("DNF.exe")? {
        anyhow::bail!("DNF is already running. Please close the game first.");
    }

    tracing::info!("Launching DNF with authentication token");

    let child = Command::new(&dnf_path).arg(token).spawn()?;
    let pid = child.id();

    tracing::info!("DNF launched (PID: {})", pid);

    if let Err(e) = inject_plugins(pid, plugins_dir) {
        tracing::warn!("Plugin injection failed: {}", e);
    }

    Ok(())
}

#[cfg(target_os = "windows")]
#[repr(C)]
struct PeDosHeader {
    e_magic: u16,
    _reserved: [u16; 29], // fields e_cblp … e_res2 (bytes 2–59)
    e_lfanew: u32,        // offset to PE signature (byte 60)
}

#[cfg(target_os = "windows")]
#[repr(C)]
struct PeExportDir {
    _characteristics: u32,
    _time_date_stamp: u32,
    _major_version: u16,
    _minor_version: u16,
    _name: u32,
    _base: u32,
    _number_of_functions: u32,
    number_of_names: u32,
    address_of_functions: u32,
    address_of_names: u32,
    address_of_name_ordinals: u32,
}

/// Loads `%SystemRoot%\SysWOW64\kernel32.dll` as an image resource and parses
/// its export directory to find the RVA of `LoadLibraryW`.
#[cfg(target_os = "windows")]
fn loadlibraryw_rva() -> Result<u32> {
    use std::ffi::CStr;

    let sys_root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
    let path = format!("{}\\SysWOW64\\kernel32.dll\0", sys_root);

    // LOAD_LIBRARY_AS_IMAGE_RESOURCE maps sections at their RVAs (no DllMain),
    // which lets us navigate PE structures using virtual addresses directly.
    let hmod = unsafe {
        LoadLibraryExA(
            windows::core::PCSTR(path.as_ptr()),
            None,
            LOAD_LIBRARY_AS_IMAGE_RESOURCE,
        )?
    };

    // The low 2 bits of the handle are used as type flags; clear them to get
    // the actual mapping base.
    let base = (hmod.0 as usize & !3) as *const u8;

    let result = (|| -> Result<u32> {
        unsafe {
            let dos = &*(base as *const PeDosHeader);
            anyhow::ensure!(dos.e_magic == 0x5A4D, "invalid DOS signature");

            // PE signature (4 bytes) + FileHeader (20 bytes) = 24 bytes of prefix
            // before OptionalHeader.
            let opt = base.add(dos.e_lfanew as usize + 24);

            // OptionalHeader.Magic must be 0x010B (PE32) for a 32-bit DLL.
            anyhow::ensure!(*(opt as *const u16) == 0x010B, "not a PE32 image");

            // DataDirectory[0] (export directory RVA) is at OptionalHeader + 96.
            let export_rva = *(opt.add(96) as *const u32);
            anyhow::ensure!(export_rva != 0, "no export directory");

            let exp = &*(base.add(export_rva as usize) as *const PeExportDir);
            let names = base.add(exp.address_of_names as usize) as *const u32;
            let ordinals = base.add(exp.address_of_name_ordinals as usize) as *const u16;
            let funcs = base.add(exp.address_of_functions as usize) as *const u32;

            for i in 0..exp.number_of_names as usize {
                let name_ptr = base.add(*names.add(i) as usize) as *const i8;
                let name = CStr::from_ptr(name_ptr);
                if name.to_bytes() == b"LoadLibraryW" {
                    // AddressOfNameOrdinals[i] is a zero-based index into
                    // AddressOfFunctions; no ordinal base adjustment is needed.
                    let ordinal = *ordinals.add(i) as usize;
                    return Ok(*funcs.add(ordinal));
                }
            }
        }

        anyhow::bail!("LoadLibraryW not found in SysWOW64\\kernel32.dll");
    })();

    unsafe {
        let _ = FreeLibrary(hmod);
    }
    result
}

/// Returns the runtime address of `LoadLibraryW` in the given 32-bit process.
///
/// Uses `TH32CS_SNAPMODULE32` to enumerate the target's 32-bit module list
/// and locate `kernel32.dll`.
#[cfg(target_os = "windows")]
fn loadlibraryw_in_process(pid: u32) -> Result<u32> {
    let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, pid)? };

    let mut me = MODULEENTRY32W {
        dwSize: std::mem::size_of::<MODULEENTRY32W>() as u32,
        ..Default::default()
    };

    let mut k32_base: u32 = 0;

    unsafe {
        if Module32FirstW(snap, &mut me).is_ok() {
            loop {
                let end = me.szModule.iter().position(|&c| c == 0).unwrap_or(0);
                let name = String::from_utf16_lossy(&me.szModule[..end]);
                if name.eq_ignore_ascii_case("kernel32.dll") {
                    // The 32-bit base address fits in u32 even on a 64-bit host.
                    k32_base = me.modBaseAddr as u32;
                    break;
                }
                if Module32NextW(snap, &mut me).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
    }

    anyhow::ensure!(k32_base != 0, "kernel32.dll not found in PID {}", pid);

    Ok(k32_base + loadlibraryw_rva()?)
}

/// Injects a single DLL into the target process via `CreateRemoteThread`.
#[cfg(target_os = "windows")]
fn inject_one(hprocess: HANDLE, path: &str, loadlibraryw: u32) -> Result<()> {
    // Null-terminated UTF-16 path for LoadLibraryW.
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0u16)).collect();
    let byte_len = wide.len() * 2;
    let bytes: Vec<u8> = wide.iter().flat_map(|c| c.to_le_bytes()).collect();

    let remote = unsafe {
        VirtualAllocEx(
            hprocess,
            None,
            byte_len,
            MEM_COMMIT | MEM_RESERVE,
            PAGE_READWRITE,
        )
    };
    anyhow::ensure!(!remote.is_null(), "VirtualAllocEx failed");

    if unsafe { WriteProcessMemory(hprocess, remote, bytes.as_ptr().cast(), byte_len, None) }
        .is_err()
    {
        unsafe {
            let _ = VirtualFreeEx(hprocess, remote, 0, MEM_RELEASE);
        }
        anyhow::bail!("WriteProcessMemory failed");
    }

    let start_routine: LPTHREAD_START_ROUTINE =
        unsafe { std::mem::transmute(loadlibraryw as usize) };

    let hthread =
        unsafe { CreateRemoteThread(hprocess, None, 0, start_routine, Some(remote), 0, None) };

    // Wait for LoadLibraryW to return; the thread exit code is its return value (HMODULE).
    let load_result = match hthread {
        Ok(ht) => {
            let wait = unsafe { WaitForSingleObject(ht, 8000) };
            if wait != WAIT_OBJECT_0 {
                // Timeout or wait failure: close the thread handle but do NOT free the
                // remote buffer — the thread may still be reading it. The buffer will be
                // reclaimed when the target process exits.
                unsafe {
                    let _ = CloseHandle(ht);
                }
                anyhow::bail!(
                    "WaitForSingleObject returned 0x{:X} (timeout or error)",
                    wait.0
                );
            }
            let mut exit_code: u32 = 0;
            unsafe {
                let _ = GetExitCodeThread(ht, &mut exit_code);
                let _ = CloseHandle(ht);
            }
            exit_code
        }
        Err(e) => {
            unsafe {
                let _ = VirtualFreeEx(hprocess, remote, 0, MEM_RELEASE);
            }
            return Err(e.into());
        }
    };

    unsafe {
        let _ = VirtualFreeEx(hprocess, remote, 0, MEM_RELEASE);
    }

    anyhow::ensure!(
        load_result != 0,
        "LoadLibraryW returned NULL (DLL missing, wrong architecture, or missing runtime)"
    );

    Ok(())
}

/// Reads all `.dll` files from `plugins_dir` and injects them into the target process.
#[cfg(target_os = "windows")]
pub fn inject_plugins(pid: u32, plugins_dir: &str) -> Result<()> {
    let launcher_dir = std::env::current_exe()?
        .parent()
        .ok_or_else(|| anyhow::anyhow!("cannot determine launcher directory"))?
        .to_path_buf();

    let plugins_dir = launcher_dir.join(plugins_dir);
    if !plugins_dir.exists() {
        return Ok(());
    }

    let dlls: Vec<std::path::PathBuf> = std::fs::read_dir(&plugins_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("dll"))
        })
        .map(|e| e.path())
        .collect();

    if dlls.is_empty() {
        return Ok(());
    }

    // The target process may not have finished loading kernel32 yet.
    let loadlibraryw = 'find: {
        for attempt in 0..20u32 {
            match loadlibraryw_in_process(pid) {
                Ok(addr) => break 'find addr,
                Err(e) => {
                    if attempt < 19 {
                        tracing::debug!(
                            "waiting for target module list (attempt {}): {}",
                            attempt + 1,
                            e
                        );
                        std::thread::sleep(std::time::Duration::from_millis(250));
                    } else {
                        return Err(e);
                    }
                }
            }
        }
        unreachable!()
    };

    let hprocess = unsafe {
        OpenProcess(
            PROCESS_CREATE_THREAD | PROCESS_VM_OPERATION | PROCESS_VM_WRITE | PROCESS_VM_READ,
            false,
            pid,
        )?
    };

    let mut log = format!("pid={} llw=0x{:08X}\n", pid, loadlibraryw);

    for dll in &dlls {
        let abs = match dll.canonicalize() {
            Ok(p) => p,
            Err(e) => {
                let line = format!("[FAIL] {}: {}\n", dll.display(), e);
                tracing::warn!("{}", line.trim());
                log.push_str(&line);
                continue;
            }
        };
        let s = abs.to_string_lossy();
        // canonicalize() prepends \\?\ on Windows; LoadLibraryW does not
        // accept extended-length paths, so strip the prefix.
        let path = s.strip_prefix("\\\\?\\").unwrap_or(&s);

        match inject_one(hprocess, path, loadlibraryw) {
            Ok(()) => {
                let line = format!("[OK] {}\n", path);
                tracing::info!("injected: {}", path);
                log.push_str(&line);
            }
            Err(e) => {
                let line = format!("[FAIL] {}: {}\n", path, e);
                tracing::warn!("inject failed {}: {}", path, e);
                log.push_str(&line);
            }
        }
    }

    unsafe {
        let _ = CloseHandle(hprocess);
    }

    // Write results to a log file beside the launcher exe for post-mortem diagnosis.
    let log_path = launcher_dir.join("plugin_inject.log");
    let _ = std::fs::write(&log_path, log.as_bytes());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "windows")]
    fn test_get_mac_address() {
        match get_mac_address() {
            Ok(mac) => {
                assert!(!mac.is_empty());
                assert!(mac.contains('-'));
            }
            Err(e) => {
                tracing::warn!("Failed to get MAC address: {}", e);
            }
        }
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_is_process_running() {
        let result = is_process_running("nonexistent_process_12345.exe");
        assert!(result.is_ok());
        assert!(!result.unwrap());

        let result = is_process_running("explorer.exe");
        if let Ok(running) = result {
            let _ = running;
        }
    }
}
