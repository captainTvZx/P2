//! Keşfet'in çalışma alanındaki yeri: raydaki pusula düğmesi ve "＋" menüsündeki "Projelere göz at"
//! `DiscoverView`'ü (discover.rs) proje rayının sağındaki alanın tamamına açar. Ray görünür kalır;
//! raya tıklamak (proje, pano, kütüphane…) Keşfet'i kapatır.

use gpui_kit::{AppContext as _, Context, Window};

use super::super::discover::{DiscoverEvent, DiscoverView, Host};
use super::WorkspaceScreen;

impl WorkspaceScreen {
    pub(super) fn open_discover(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.nav_menu = None;
        if self.discover.is_some() {
            return;
        }
        let api = self.api.clone();
        let view = cx.new(|cx| DiscoverView::new(api, Host::Workspace, window, cx));
        let sub = cx.subscribe_in(&view, window, |this, _, event: &DiscoverEvent, window, cx| {
            match event {
                DiscoverEvent::Close => {}
                DiscoverEvent::Invite => {
                    this.discover = None;
                    return this.open_join(None, window, cx);
                }
                DiscoverEvent::Joined(joined) => {
                    let name = joined.project.name.clone();
                    if joined.organization_id != this.org.id {
                        // Başka kurulum kabındaki proje: çalışma alanı o kuruluşla yeniden kurulur.
                        let (app, org) = (this.app.clone(), joined.organization.clone());
                        let _ = app.update(cx, |app, cx| app.enter_workspace(org, window, cx));
                        return;
                    }
                    this.adopt_project(joined.project.clone(), window, cx);
                    this.note(p2_i18n::tv(if joined.joined { "disc.note.joined" } else { "disc.note.already" }, &[("name", &name)]), cx);
                }
            }
            this.discover = None;
            cx.notify();
        });
        crate::trace("keşfet: çalışma alanında açıldı");
        self.discover = Some((view, sub));
        cx.notify();
    }

    /// Raydaki pusula: aç / kapat.
    pub(super) fn toggle_discover(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.discover.take().is_some() {
            cx.notify();
        } else {
            self.open_discover(window, cx);
        }
    }

    /// Esc: Keşfet'in açık penceresi/çekmecesi, yoksa Keşfet'in kendisi kapanır.
    pub(super) fn discover_escape(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((view, _)) = self.discover.as_ref() else { return false };
        if !view.update(cx, |v, cx| v.escape(cx)) {
            self.discover = None;
        }
        cx.notify();
        true
    }
}
