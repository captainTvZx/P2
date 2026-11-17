//! Proje toplulukları — çalışma alanı tarafı (docs/proje-bazli-yapi.md): gerçek zamanlı proje olayları
//! (çıkarıldım/yasaklandım, proje silindi/kalıcı silindi, projeye katılan/ayrılan kişi, yetki
//! değişikliği) ve projeden ayrılma. Katılma penceresi `join_project.rs`, rayın "＋" menüsü
//! `workspace_nav.rs`, proje menüsü ve onay pencereleri `manage_ui.rs`.

use gpui_kit::{Context, Window};
use p2_client_core::{Member, Project};
use serde_json::Value;

use super::WorkspaceScreen;
use crate::task;

impl WorkspaceScreen {
    fn project_name_of(&self, id: &str) -> Option<String> {
        self.projects.iter().find(|p| p.id == id).map(|p| p.name.clone())
    }

    /// `project:*` ve `permissions:changed` olayları.
    pub(super) fn on_project_event(&mut self, name: &str, payload: &Value, window: &mut Window, cx: &mut Context<Self>) {
        let pid = payload["projectId"].as_str().unwrap_or_default().to_string();
        match name {
            // Çıkarıldım / yasaklandım (bana özel olay).
            "project:removed" => {
                let Some(pname) = self.project_name_of(&pid) else { return };
                let banned = payload["banned"].as_bool().unwrap_or(false);
                crate::trace(&format!("topluluk: {pname} projesinden {}", if banned { "yasaklandın" } else { "çıkarıldın" }));
                let note = p2_i18n::tv(if banned { "toast.project.banned" } else { "toast.project.kicked" }, &[("name", &pname)]);
                self.remove_project(&pid, Some(note), window, cx);
            }
            "project:deleted" => {
                let Some(pname) = self.project_name_of(&pid) else { return };
                crate::trace(&format!("topluluk: {pname} silindi"));
                self.remove_project(&pid, Some(p2_i18n::tv("toast.project.deleted", &[("name", &pname)])), window, cx);
            }
            "project:purged" => {
                let pname = payload["name"].as_str().map(str::to_string).or_else(|| self.project_name_of(&pid));
                if self.projects.iter().any(|p| p.id == pid) {
                    crate::trace(&format!("topluluk: {} kalıcı silindi", pname.clone().unwrap_or_default()));
                    self.remove_project(&pid, Some(p2_i18n::tv("toast.project.purged", &[("name", &pname.unwrap_or_default())])), window, cx);
                }
            }
            // Katıldığım proje (kodla katılma) ya da başka bir istemcide açtığım proje: raya ekle.
            "project:created" => {
                if let Ok(project) = serde_json::from_value::<Project>(payload["project"].clone()) {
                    let same_org = payload["project"]["organizationId"].as_str().is_none_or(|o| o == self.org.id);
                    if same_org && !self.projects.iter().any(|p| p.id == project.id) {
                        crate::trace(&format!("topluluk: raya eklendi {}", project.name));
                        self.projects.push(project);
                        cx.notify();
                    }
                }
            }
            "project:member:joined" => {
                let Ok(member) = serde_json::from_value::<p2_client_core::ProjectMember>(payload["member"].clone()) else { return };
                if member.id == self.auth.user.id || self.project_id().as_deref() != Some(pid.as_str()) {
                    return;
                }
                crate::trace(&format!("topluluk: @{} projeye katıldı", member.username));
                crate::avatars::remember(&member.id, member.avatar_url.as_deref());
                let shown = member.display_name.clone();
                let m = Member::from(member);
                if !self.members.iter().any(|x| x.id == m.id) {
                    self.members.push(m.clone());
                    super::sort_members(&mut self.members);
                }
                if !self.org_members.iter().any(|x| x.id == m.id) {
                    self.org_members.push(m);
                }
                crate::toasts::info(p2_i18n::tv("toast.memberJoined", &[("name", &shown)]));
                cx.notify();
            }
            "project:member:removed" => {
                let user = payload["userId"].as_str().unwrap_or_default();
                if self.project_id().as_deref() == Some(pid.as_str()) && user != self.auth.user.id {
                    crate::trace(&format!("topluluk: {user} projeden ayrıldı"));
                    self.members.retain(|m| m.id != user);
                    cx.notify();
                }
                // Ayrılan kişiyle bu projedeki sohbetim kapanır (sunucu listeden düşürür): okunmamışı
                // projenin ray rozetinde asılı kalmasın diye sohbet listesi tazelenir.
                if user != self.auth.user.id && self.conversations.iter().any(|c| c.project_id.as_deref() == Some(pid.as_str()) && c.user.id == user) {
                    self.load_conversations(window, cx);
                }
            }
            // Yetkim değişti / projeye eklendim (erişim açıldı).
            "project:permissions" | "permissions:changed" => {
                if payload["visible"].as_bool() == Some(true) && !pid.is_empty() && !self.projects.iter().any(|p| p.id == pid) {
                    self.refresh_projects(window, cx);
                } else if pid.is_empty() || self.project_id().as_deref() == Some(pid.as_str()) {
                    self.load_project_perms(window, cx);
                    if pid.is_empty() {
                        self.load_rail_perms(None, window, cx);
                    }
                } else {
                    // Seçili olmayan projedeki yetkim değişti: raydaki sağ tık menüsü güncel kalsın.
                    self.load_rail_perms(Some(pid.clone()), window, cx);
                }
            }
            _ => {}
        }
    }

    /// Proje listesini sessizce tazele (yeni projeler raya eklenir, seçim değişmez).
    fn refresh_projects(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (api, org) = (self.api.clone(), self.org.id.clone());
        task::run(window, cx, move || api.projects(&org), |this, r, _, cx| {
            if let Ok(list) = r {
                for p in list {
                    if !this.projects.iter().any(|x| x.id == p.id) {
                        crate::trace(&format!("topluluk: raya eklendi {}", p.name));
                        this.projects.push(p);
                    }
                }
                cx.notify();
            }
        });
    }

    /// "Projeden ayrıl" onaylandı: `POST /projects/:p/leave`. Birincil sahip ayrılamaz (`OWNER_CANNOT_LEAVE`).
    pub(super) fn leave_project(&mut self, project: Project, window: &mut Window, cx: &mut Context<Self>) {
        let api = self.api.clone();
        let id = project.id.clone();
        task::run(window, cx, move || api.leave_project(&id), move |this, r, window, cx| match r {
            Ok(_) => {
                crate::trace(&format!("topluluk: {} projesinden ayrıldın", project.name));
                this.manage = None;
                this.remove_project(&project.id, Some(p2_i18n::tv("toast.project.left", &[("name", &project.name)])), window, cx);
            }
            Err(e) => {
                crate::trace(&format!("topluluk: ayrılamadın {}", e.code));
                let text = super::super::join_project::join_error(&e);
                this.manage_error(text, cx);
            }
        });
    }
}
