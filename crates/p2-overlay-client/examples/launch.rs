//! Elle deneme: test oyununu (p2-d3dtest) overlay ile başlatır, olayları yazdırır.
//!   P2_API=http://127.0.0.1:4110 cargo run --release -p p2-overlay-client --example launch
//! Giriş: p2test/Parola123 (test sunucusu). Host'un otomatik testi P2_OVERLAY_AUTOPILOT ile.
use std::sync::Arc;

use p2_overlay_client::{Context, Event, OverlayManager};
use p2_patcher::LaunchSpec;

fn main() {
    let api_base = std::env::var("P2_API").unwrap_or_else(|_| "http://127.0.0.1:4110".into());
    let api = p2_client_core::Api::new(&api_base, "1.0.0");
    let auth = api.login("p2test", "Parola123", false, None).expect("giriş");
    api.set_access_only(&auth.access_token); // kalıcı oturuma dokunmaz
    let org = api.organizations().expect("org")[0].clone();
    let project = api.projects(&org.id).expect("proje")[0].clone();
    let manager = OverlayManager::new(Arc::new(|e: Event| println!("olay: {e:?}")));
    println!("yollar: {:?}", manager.paths());
    manager.set_context(Context { api: api_base, client_version: "1.0.0".into(), access_token: auth.access_token.clone(), org: org.id.clone(), user_id: auth.user.id.clone(), user_name: auth.user.display_name.clone(), shot_dir: std::env::var("P2_SHOT_DIR").unwrap_or_else(|_| "C:/p2-pg/slice7/shots".into()), ..Default::default() });
    let game = std::env::current_exe().unwrap().parent().unwrap().parent().unwrap().join("p2-d3dtest.exe");
    let spec = LaunchSpec { project_id: project.id.clone(), project_name: project.name.clone(), exe: game.clone(), args: vec!["d3d11".into()], cwd: game.parent().unwrap().to_path_buf() };
    let launched = manager.launch(&spec).expect("overlay ile başlatılamadı");
    println!("pid {}", launched.pid);
    let secs: u64 = std::env::var("P2_SECONDS").ok().and_then(|v| v.parse().ok()).unwrap_or(20);
    let m = manager.clone();
    let pid = launched.pid;
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(4));
        m.toast(None, p2_overlay_client::Toast { kind: "message".into(), title: "Ayşe".into(), text: "Akşam test var mı?".into(), ..Default::default() });
        std::thread::sleep(std::time::Duration::from_secs(secs));
        let _ = std::process::Command::new("taskkill").args(["/PID", &pid.to_string(), "/T", "/F"]).status();
    });
    let code = (launched.wait)();
    println!("oyun bitti, kod {code}");
    std::thread::sleep(std::time::Duration::from_millis(500));
}
