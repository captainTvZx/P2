//! Performans ölçümleri (Electron `systemStats` + `gameStats`): oyun FPS'i (DLL'in Present
//! sayacı), oyun sürecinin CPU'su ve belleği, sistem CPU'su ve belleği. 1 Hz örneklenir.

use std::collections::VecDeque;
use std::time::Instant;

use windows::Win32::Foundation::{CloseHandle, FILETIME, HANDLE};
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows::Win32::System::SystemInformation::{GetSystemInfo, GlobalMemoryStatusEx, MEMORYSTATUSEX, SYSTEM_INFO};
use windows::Win32::System::Threading::{GetProcessTimes, GetSystemTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

fn ft(t: FILETIME) -> u64 {
    (t.dwHighDateTime as u64) << 32 | t.dwLowDateTime as u64
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sample {
    pub fps: f32,
    pub game_cpu: Option<f32>,
    pub game_ram_mb: Option<u64>,
    pub sys_cpu: Option<f32>,
    pub sys_ram_used_mb: u64,
    pub sys_ram_total_mb: u64,
}

pub struct Perf {
    process: Option<HANDLE>,
    cores: u32,
    last_frames: u32,
    last_at: Instant,
    last_game: Option<(u64, Instant)>,
    last_sys: Option<(u64, u64)>,
    pub history: VecDeque<f32>,
    pub current: Sample,
}

impl Drop for Perf {
    fn drop(&mut self) {
        if let Some(h) = self.process.take() {
            unsafe {
                let _ = CloseHandle(h);
            }
        }
    }
}

impl Perf {
    pub const HISTORY: usize = 60;

    pub fn new(pid: u32, frames: u32) -> Self {
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok();
        let mut info = SYSTEM_INFO::default();
        unsafe { GetSystemInfo(&mut info) };
        let mut perf = Self {
            process,
            cores: info.dwNumberOfProcessors.max(1),
            last_frames: frames,
            last_at: Instant::now(),
            last_game: None,
            last_sys: None,
            history: VecDeque::with_capacity(Self::HISTORY),
            current: Sample::default(),
        };
        perf.sample_cpu(); // ilk örnek: sonraki tikte yüzde hesaplanabilsin
        perf
    }

    fn sample_cpu(&mut self) -> (Option<f32>, Option<f32>) {
        let now = Instant::now();
        let mut game = None;
        if let Some(h) = self.process {
            let (mut c, mut e, mut k, mut u) = Default::default();
            if unsafe { GetProcessTimes(h, &mut c, &mut e, &mut k, &mut u) }.is_ok() {
                let busy = ft(k) + ft(u); // 100 ns
                if let Some((prev, at)) = self.last_game {
                    let wall = now.duration_since(at).as_secs_f64() * 1e7;
                    if wall > 0. {
                        game = Some((((busy - prev) as f64 / wall) / self.cores as f64 * 100.).clamp(0., 100.) as f32);
                    }
                }
                self.last_game = Some((busy, now));
            }
        }
        let mut sys = None;
        let (mut idle, mut kernel, mut user) = Default::default();
        if unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)) }.is_ok() {
            let (idle, total) = (ft(idle), ft(kernel) + ft(user)); // çekirdek süresi boşta süreyi içerir
            if let Some((pi, pt)) = self.last_sys {
                let (di, dt) = (idle - pi, total - pt);
                if dt > 0 {
                    sys = Some(((1. - di as f64 / dt as f64) * 100.).clamp(0., 100.) as f32);
                }
            }
            self.last_sys = Some((idle, total));
        }
        (game, sys)
    }

    /// Saniyede bir çağrılır; `frames` DLL'in Present sayacı.
    pub fn tick(&mut self, frames: u32) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_at).as_secs_f32().max(0.001);
        let fps = frames.wrapping_sub(self.last_frames) as f32 / dt;
        self.last_frames = frames;
        self.last_at = now;
        let (game_cpu, sys_cpu) = self.sample_cpu();
        let game_ram_mb = self.process.and_then(|h| {
            let mut pmc = PROCESS_MEMORY_COUNTERS { cb: size_of::<PROCESS_MEMORY_COUNTERS>() as u32, ..Default::default() };
            unsafe { GetProcessMemoryInfo(h, &mut pmc, pmc.cb) }.ok().map(|_| (pmc.WorkingSetSize / 1_048_576) as u64)
        });
        let mut mem = MEMORYSTATUSEX { dwLength: size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
        let _ = unsafe { GlobalMemoryStatusEx(&mut mem) };
        self.current = Sample {
            fps,
            game_cpu,
            game_ram_mb,
            sys_cpu,
            sys_ram_used_mb: (mem.ullTotalPhys - mem.ullAvailPhys) / 1_048_576,
            sys_ram_total_mb: mem.ullTotalPhys / 1_048_576,
        };
        if self.history.len() == Self::HISTORY {
            self.history.pop_front();
        }
        self.history.push_back(fps);
    }

    /// Ortalama ve "en düşük %1" (son 60 sn).
    pub fn summary(&self) -> (f32, f32) {
        if self.history.is_empty() {
            return (0., 0.);
        }
        let avg = self.history.iter().sum::<f32>() / self.history.len() as f32;
        let mut sorted: Vec<f32> = self.history.iter().copied().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        (avg, sorted[(sorted.len() / 100).min(sorted.len() - 1)])
    }
}
