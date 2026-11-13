//! Tanılama: gerçek zamanlı bağlantıyı arayüzsüz dener ve gelen olayları yazar.
//!   P2_TRACE_SOCKET=1 cargo run -p p2-client-core --example rt_probe -- <api> <kullanıcı> <parola> [saniye]
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::StreamExt;
use p2_client_core::{Api, realtime};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [api_url, user, pass, ..] = args.as_slice() else {
        eprintln!("kullanım: rt_probe <api> <kullanıcı> <parola> [saniye]");
        std::process::exit(2);
    };
    let secs: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(10);
    let api = Arc::new(Api::new(api_url, "rt-probe"));
    let auth = api.login(user, pass, false, None).expect("giriş");
    api.set_auth(&auth, false);
    let org = api.organizations().expect("kuruluşlar").into_iter().next().expect("kuruluş yok");
    println!("bağlanılıyor: {} ({})", org.name, org.id);
    let (_rt, mut rx) = realtime::connect(api, org.id);
    let start = Instant::now();
    futures::executor::block_on(async {
        while start.elapsed() < Duration::from_secs(secs) {
            let next = futures::future::select(rx.next(), futures_timer(Duration::from_millis(200))).await;
            if let futures::future::Either::Left((Some(ev), _)) = next {
                println!("[+{} ms] {ev:?}", start.elapsed().as_millis());
            }
        }
    });
}

fn futures_timer(d: Duration) -> impl std::future::Future<Output = ()> + Unpin {
    let (tx, rx) = futures::channel::oneshot::channel::<()>();
    std::thread::spawn(move || {
        std::thread::sleep(d);
        let _ = tx.send(());
    });
    Box::pin(async move {
        let _ = rx.await;
    })
}
