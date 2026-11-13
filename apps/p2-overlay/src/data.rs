//! Panel araçlarının sunucu verileri (görevler, notlar, ekip, sesli kanallar, sürüm). GPUI'nin ekran
//! dışı bağlamı deterministik yürütücüyle çalıştığı için ağ işleri düz iş parçacıklarında yapılır,
//! sonuçlar kanalla ana döngüye gelir.

use std::sync::Arc;
use std::sync::mpsc::Sender;

use p2_client_core::{Api, ApiError, BoardInfo, ChannelType, Member, ProjectNote, Task};

pub enum DataMsg {
    /// Projenin bütün panoları ve görevleri (TasksWindow "Tüm panolar").
    Tasks(Result<(Vec<BoardInfo>, Vec<(String, Vec<Task>)>), String>),
    Notes(Result<Vec<ProjectNote>, String>),
    /// Yeni not oluşturuldu (seçilip düzenleyiciye geçilir).
    NoteCreated(ProjectNote),
    NoteSaving(String, bool),
    Team(Result<Vec<Member>, String>),
    VoiceChannels(Vec<(String, String)>),
    Release(Option<(String, String, String)>),
    /// Vitrin: oynama süreleri (ben, ekip) ve son gönderiler (useGameOverview `hub`).
    Hub { mine: u64, team: u64, posts: Vec<crate::ui::HubPostView> },
    /// Kullanıcıya kısa bilgi (ör. "Görev eklendi").
    Info(String),
    Error(String),
    TokenExpired,
    /// HUD'daki "ms" ve "%kayıp": sunucu gidiş-dönüşü ve son yoklamalardaki hata oranı.
    Ping { ping: Option<u32>, loss: Option<f32> },
}

/// usePing.ts: HUD açıkken 5 sn'de bir GET /health (4 sn zaman aşımı); kayıp son 24 yoklamanın
/// hatalı olanlarının yüzdesi (en az 3 yoklamadan sonra).
pub fn start_ping(api: Arc<Api>, on: Arc<std::sync::atomic::AtomicBool>, tx: Sender<DataMsg>) {
    const WINDOW: usize = 24;
    std::thread::spawn(move || {
        let mut results: std::collections::VecDeque<bool> = std::collections::VecDeque::new();
        let mut was_on = false;
        loop {
            let enabled = on.load(std::sync::atomic::Ordering::Relaxed);
            if !enabled {
                if was_on {
                    results.clear();
                    if tx.send(DataMsg::Ping { ping: None, loss: None }).is_err() {
                        return;
                    }
                }
                was_on = false;
                std::thread::sleep(std::time::Duration::from_millis(500));
                continue;
            }
            was_on = true;
            let started = std::time::Instant::now();
            let ok = api.health().is_ok() && started.elapsed() < std::time::Duration::from_secs(4);
            let ping = ok.then(|| started.elapsed().as_millis() as u32);
            results.push_back(ok);
            if results.len() > WINDOW {
                results.pop_front();
            }
            let failed = results.iter().filter(|r| !**r).count();
            let loss = (results.len() >= 3).then(|| (failed as f32 / results.len() as f32 * 1000.).round() / 10.);
            if tx.send(DataMsg::Ping { ping, loss }).is_err() {
                return;
            }
            for _ in 0..50 {
                if !on.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    });
}

#[derive(Clone)]
pub struct Data {
    pub api: Arc<Api>,
    pub org: String,
    pub project: String,
    pub tx: Sender<DataMsg>,
}

fn err(e: ApiError) -> String {
    if e.message.is_empty() { format!("HTTP {}", e.status) } else { e.message }
}

impl Data {
    fn spawn(&self, job: impl FnOnce(&Data) -> Result<(), ApiError> + Send + 'static) {
        let this = self.clone();
        std::thread::spawn(move || {
            if let Err(e) = job(&this) {
                let _ = this.tx.send(if e.status == 401 { DataMsg::TokenExpired } else { DataMsg::Error(err(e)) });
            }
        });
    }

    fn fetch_tasks(&self) -> Result<(Vec<BoardInfo>, Vec<(String, Vec<Task>)>), ApiError> {
        let mut boards = self.api.boards(&self.org, &self.project)?;
        boards.sort_by_key(|b| b.position);
        let mut by_board = Vec::new();
        for b in &boards {
            let mut tasks = self.api.board(&self.org, &self.project, &b.id)?.tasks;
            tasks.sort_by_key(|t| t.position);
            by_board.push((b.id.clone(), tasks));
        }
        Ok((boards, by_board))
    }

    pub fn load_tasks(&self) {
        self.spawn(|d| {
            let r = d.fetch_tasks();
            if let Err(e) = &r {
                if e.status == 401 {
                    return Err(e.clone());
                }
            }
            let _ = d.tx.send(DataMsg::Tasks(r.map_err(err)));
            Ok(())
        });
    }

    pub fn create_task(&self, board: String, input: serde_json::Value) {
        self.spawn(move |d| {
            let task = d.api.create_task(&d.org, &d.project, &board, input)?;
            let _ = d.tx.send(DataMsg::Info(format!("Görev eklendi: #{} {}", task.number, task.title)));
            let _ = d.tx.send(DataMsg::Tasks(d.fetch_tasks().map_err(err)));
            Ok(())
        });
    }

    /// Sütun değişimi: TasksWindow `tasks.move` → PATCH …/move { status, order }.
    pub fn move_task(&self, board: String, task: String, status: String) {
        self.spawn(move |d| {
            let mut order: Vec<String> = d.api.board(&d.org, &d.project, &board)?.tasks.into_iter().filter(|t| t.status == status && t.id != task).map(|t| t.id).collect();
            order.push(task.clone());
            d.api.move_task(&d.org, &d.project, &board, &task, &status, &order)?;
            let _ = d.tx.send(DataMsg::Tasks(d.fetch_tasks().map_err(err)));
            Ok(())
        });
    }

    pub fn load_notes(&self) {
        self.spawn(|d| {
            let notes = d.api.notes(&d.org, &d.project)?;
            let _ = d.tx.send(DataMsg::Notes(Ok(notes)));
            Ok(())
        });
    }

    /// "Yeni not": boş özel not (ProjectNotesPanel `createNew`), sonra düzenleyicide açılır.
    pub fn create_note(&self) {
        self.spawn(move |d| {
            let note = d.api.create_note(&d.org, &d.project, "", "", false)?;
            let _ = d.tx.send(DataMsg::Info("Not oluşturuldu".into()));
            let notes = d.api.notes(&d.org, &d.project)?;
            let _ = d.tx.send(DataMsg::Notes(Ok(notes)));
            let _ = d.tx.send(DataMsg::NoteCreated(note));
            Ok(())
        });
    }

    pub fn save_note(&self, id: String, patch: serde_json::Value) {
        let _ = self.tx.send(DataMsg::NoteSaving(id.clone(), true));
        self.spawn(move |d| {
            let r = d.api.update_note(&d.org, &d.project, &id, patch);
            let _ = d.tx.send(DataMsg::NoteSaving(id.clone(), false));
            let note = r?;
            let _ = d.tx.send(DataMsg::Info(format!("Not kaydedildi: {}", note.title)));
            let notes = d.api.notes(&d.org, &d.project)?;
            let _ = d.tx.send(DataMsg::Notes(Ok(notes)));
            Ok(())
        });
    }

    pub fn delete_note(&self, id: String) {
        self.spawn(move |d| {
            d.api.delete_note(&d.org, &d.project, &id)?;
            let notes = d.api.notes(&d.org, &d.project)?;
            let _ = d.tx.send(DataMsg::Notes(Ok(notes)));
            Ok(())
        });
    }

    pub fn load_team(&self) {
        self.spawn(|d| {
            let r = d.api.members(&d.org, Some(&d.project));
            if let Err(e) = &r {
                if e.status == 401 {
                    return Err(e.clone());
                }
            }
            let _ = d.tx.send(DataMsg::Team(r.map_err(err)));
            Ok(())
        });
    }

    pub fn load_voice(&self) {
        self.spawn(|d| {
            let cats = d.api.channels(&d.org, &d.project)?;
            let mut rooms: Vec<(String, String)> = cats.into_iter().flat_map(|c| c.channels).filter(|c| c.kind == ChannelType::Voice).map(|c| (c.id, c.name)).collect();
            rooms.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
            let _ = d.tx.send(DataMsg::VoiceChannels(rooms));
            Ok(())
        });
    }

    pub fn load_release(&self, channel: String) {
        self.spawn(move |d| {
            let list = d.api.releases(&d.org, &d.project)?;
            let live = list.latest.get(&channel).cloned().flatten().or_else(|| list.latest.values().flatten().next().cloned());
            let _ = d.tx.send(DataMsg::Release(live.map(|r| (r.version, r.channel, r.notes.unwrap_or_default()))));
            Ok(())
        });
    }
}

impl Data {
    pub fn load_hub(&self) {
        self.spawn(|d| {
            let hub = d.api.hub(&d.org, &d.project)?;
            let posts = hub
                .posts
                .into_iter()
                .take(5)
                .map(|p| crate::ui::HubPostView { author_id: p.author.id, author: p.author.display_name, release: p.kind == "RELEASE", content: p.content, created_at: p.created_at })
                .collect();
            let _ = d.tx.send(DataMsg::Hub { mine: hub.playtime.mine, team: hub.playtime.team, posts });
            Ok(())
        });
    }
}
