//! Windows dışı: oyun içi overlay (DLL enjeksiyonu) yok; süreç izleme boş döner.

#[derive(Clone, Debug)]
pub struct ProcInfo {
    pub pid: u32,
    pub ppid: u32,
    pub name: String,
}

pub fn processes() -> Vec<ProcInfo> {
    Vec::new()
}

pub fn is_wow64(_pid: u32) -> bool {
    false
}

pub struct Proc;

impl Proc {
    pub fn open(_pid: u32) -> Option<Self> {
        None
    }
    pub fn exited(&self) -> bool {
        true
    }
    pub fn exit_code(&self) -> Option<i32> {
        None
    }
}
