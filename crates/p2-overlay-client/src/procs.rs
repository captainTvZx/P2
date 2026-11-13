//! Süreç yardımcıları: süreç listesi (pid, ebeveyn, ad, 32-bit mi), bekleme ve çıkış kodu.

use windows::Win32::Foundation::{CloseHandle, HANDLE, STILL_ACTIVE, WAIT_OBJECT_0};
use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS};
use windows::Win32::System::SystemInformation::IMAGE_FILE_MACHINE_UNKNOWN;
use windows::Win32::System::Threading::{
    GetExitCodeProcess, IsWow64Process2, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, WaitForSingleObject,
};

#[derive(Clone, Debug)]
pub struct ProcInfo {
    pub pid: u32,
    pub ppid: u32,
    pub name: String,
}

pub fn processes() -> Vec<ProcInfo> {
    let mut out = Vec::new();
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return out };
        let mut entry = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        if Process32FirstW(snap, &mut entry).is_ok() {
            loop {
                let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
                out.push(ProcInfo { pid: entry.th32ProcessID, ppid: entry.th32ParentProcessID, name: String::from_utf16_lossy(&entry.szExeFile[..len]) });
                if Process32NextW(snap, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
    }
    out
}

/// 64-bit Windows'ta 32-bit (WOW64) süreç mi? (DLL yalnız 64-bit.)
pub fn is_wow64(pid: u32) -> bool {
    unsafe {
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return false };
        let mut process = IMAGE_FILE_MACHINE_UNKNOWN;
        let mut native = IMAGE_FILE_MACHINE_UNKNOWN;
        let ok = IsWow64Process2(h, &mut process, Some(&mut native)).is_ok();
        let _ = CloseHandle(h);
        ok && process != IMAGE_FILE_MACHINE_UNKNOWN
    }
}

/// Bekleme/çıkış kodu için açık süreç tutamacı.
pub struct Proc(HANDLE);

unsafe impl Send for Proc {}
unsafe impl Sync for Proc {}

impl Proc {
    pub fn open(pid: u32) -> Option<Self> {
        unsafe { OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok().map(Proc)
    }

    pub fn exited(&self) -> bool {
        unsafe { WaitForSingleObject(self.0, 0) == WAIT_OBJECT_0 }
    }

    pub fn exit_code(&self) -> Option<i32> {
        let mut code = 0u32;
        unsafe { GetExitCodeProcess(self.0, &mut code) }.ok()?;
        (code != STILL_ACTIVE.0 as u32).then_some(code as i32)
    }
}

impl Drop for Proc {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
