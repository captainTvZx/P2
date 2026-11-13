//! Tek örnek (Electron `requestSingleInstanceLock` + `second-instance` → pencereyi göster).
//! Adlandırılmış mutex ilk örneği belirler; ikinci örnek adlandırılmış olayı tetikleyip çıkar,
//! ilk örnek olayı bekleyen iş parçacığından `SHOW` bayrağını kaldırır (ana döngü pencereyi öne alır).

use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{CreateEventW, CreateMutexW, INFINITE, OpenEventW, SetEvent, WaitForSingleObject, EVENT_MODIFY_STATE};
use windows::core::w;

pub static SHOW: AtomicBool = AtomicBool::new(false);

/// İlk örnekse `true`. İkinci örnek ilkini uyarır ve `false` döner.
pub fn acquire() -> bool {
    unsafe {
        let mutex = CreateMutexW(None, false, w!("Local\\P2ServerConsoleRust"));
        let first = mutex.is_ok() && GetLastError() != ERROR_ALREADY_EXISTS;
        if !first {
            if let Ok(event) = OpenEventW(EVENT_MODIFY_STATE, false, w!("Local\\P2ServerConsoleRustShow")) {
                let _ = SetEvent(event);
            }
            return false;
        }
        // Mutex tutamağı süreç boyunca açık kalır (bilerek sızdırılır).
        std::mem::forget(mutex);
        if let Ok(event) = CreateEventW(None, false, false, w!("Local\\P2ServerConsoleRustShow")) {
            let raw = event.0 as usize;
            std::thread::Builder::new()
                .name("p2-tek-ornek".into())
                .spawn(move || loop {
                    let handle = windows::Win32::Foundation::HANDLE(raw as _);
                    if WaitForSingleObject(handle, INFINITE) != WAIT_OBJECT_0 {
                        return;
                    }
                    SHOW.store(true, Ordering::SeqCst);
                })
                .ok();
        }
        true
    }
}
