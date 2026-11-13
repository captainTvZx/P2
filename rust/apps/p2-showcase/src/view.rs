//! Vitrin görünümleri.
//!
//! Mimari kural (Faz 0 bulgusu): GPUI, bir görünüm bildirim aldığında (`cx.notify`)
//! pencereyi yeniden çizer ve önbelleksiz bütün görünümleri baştan kurar. Animasyonlar
//! her karede kendi görünümlerine bildirim gönderir. Bu yüzden:
//! - Ağır bölmeler (sohbet listesi) ayrı bir varlık olur ve `cached()` ile gömülür;
//!   yalnız kendisi bildirim alınca yeniden kurulur.
//! - Sürekli animasyonlar (çevrimiçi nabzı) küçük, ayrı görünümlerde yaşar.

use std::time::Duration;

use gpui_kit::component::TitleBar;
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnimationExt as _, AnyElement, ClickEvent, Context, Entity, FontStyle, FontWeight,
    HighlightStyle, ListAlignment, ListState, SharedString, StyleRefinement, StyledText,
    Subscription, Window, div, list, px,
};
use p2_ui::motion::{M_STAGGER, pulse, pulse_opacity};
use p2_ui::theme::ActiveP2 as _;
use p2_ui::tokens::{FONT_MONO, font_size, radius};
use p2_ui::{Entrance, P2Motion as _, Palette};

use crate::data::{self, CHANNELS, Message, PEOPLE, Status};
use p2_ui::rich::{self, Block, Span};

const MESSAGE_COUNT: usize = 10_000;
const TOAST_WIDTH: f32 = 320.;

fn trace(what: &str) {
    if std::env::var_os("P2_TRACE").is_some() {
        if let Some(started) = crate::STARTED.get() {
            println!("{what} t={}ms", started.elapsed().as_millis());
        }
    }
}

// =================================================================== kök

struct Toast {
    id: u64,
    title: SharedString,
    body: SharedString,
}

pub struct Showcase {
    channel: usize,
    chat: Entity<ChatPane>,
    members: Entity<MemberPane>,
    modal_epoch: Option<u64>,
    toasts: Vec<Toast>,
    next_id: u64,
    first_frame_logged: bool,
}

impl Showcase {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            channel: 0,
            chat: cx.new(|cx| ChatPane::new(window, cx)),
            members: cx.new(|cx| MemberPane::new(window, cx)),
            modal_epoch: None,
            toasts: Vec::new(),
            next_id: 1,
            first_frame_logged: false,
        }
    }

    fn switch_channel(&mut self, channel: usize, cx: &mut Context<Self>) {
        if channel == self.channel {
            return;
        }
        self.channel = channel;
        self.chat.update(cx, |chat, cx| chat.set_channel(channel, cx));
        cx.notify();
    }

    fn push_toast(&mut self, cx: &mut Context<Self>) {
        let id = self.next_id;
        self.next_id += 1;
        let person = &PEOPLE[(id as usize * 5) % PEOPLE.len()];
        self.toasts.push(Toast {
            id,
            title: format!("{} · #{}", person.name, CHANNELS[self.channel]).into(),
            body: "Yeni derleme hazır, beta kanalına attım 🚀".into(),
        });
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(4)).await;
            this.update(cx, |this, cx| {
                this.toasts.retain(|t| t.id != id);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn toggle_theme(&mut self, cx: &mut Context<Self>) {
        let next = if cx.p2().dark { Palette::p2_light() } else { Palette::p2_dark() };
        // `apply` pencereyi önbelleği yok sayarak tazeler; önbellekli bölmeler de yeni renge geçer.
        p2_ui::theme::apply(next, cx);
    }

    fn title_bar(&self, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        TitleBar::new().bg(p.bg_0).border_color(p.border_subtle).child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .w_full()
                .pr_2()
                .child(
                    div()
                        .px_1p5()
                        .rounded(radius::XS)
                        .bg(p.accent)
                        .text_color(p.text_on_accent)
                        .text_size(font_size::SM)
                        .font_weight(FontWeight::BOLD)
                        .child("P2"),
                )
                .child(div().text_size(font_size::SM).text_color(p.text_3).child("Faz 0 · GPUI vitrini"))
                .child(div().flex_1())
                .child(chip("theme", if p.dark { "☀ Açık tema" } else { "☾ Koyu tema" }, p).on_click(cx.listener(|this, _, _, cx| this.toggle_theme(cx))))
                .child(chip("toast", "Toast", p).on_click(cx.listener(|this, _, _, cx| this.push_toast(cx))))
                .child(chip("modal", "Modal", p).on_click(cx.listener(|this, _, _, cx| {
                    this.modal_epoch = Some(this.next_id);
                    this.next_id += 1;
                    cx.notify();
                }))),
        )
    }

    fn rail(&self, p: &Palette) -> impl IntoElement {
        let projects = ["1950s Tow Truck", "MineTastic", "Kara Kutu", "Proje X"];
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_2()
            .w(px(64.))
            .pt_3()
            .bg(p.bg_1)
            .border_r_1()
            .border_color(p.border_subtle)
            .children(projects.iter().enumerate().map(|(i, name)| {
                let (bg, fg) = p.hue(i + 2);
                let active = i == 0;
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(div().w(px(3.)).h(if active { px(24.) } else { px(0.) }).rounded_r(px(3.)).bg(p.accent))
                    .child(
                        div()
                            .size(px(42.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(if active { radius::LG } else { px(21.) })
                            .bg(bg)
                            .text_color(fg)
                            .font_weight(FontWeight::BOLD)
                            .child(data::initials(name)),
                    )
                    .p2_enter(("rail", i), Entrance::SlideLeft, M_STAGGER * i as u32)
            }))
    }

    fn sidebar(&self, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .w(px(232.))
            .bg(p.bg_1)
            .border_r_1()
            .border_color(p.border_subtle)
            .child(
                div()
                    .h(px(48.))
                    .px_4()
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(p.border_subtle)
                    .text_color(p.text_strong)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("1950s Tow Truck"),
            )
            .child(section_label("PROJE KANALLARI", p))
            .children(CHANNELS.iter().enumerate().map(|(i, name)| {
                let selected = i == self.channel;
                div()
                    .id(("channel", i))
                    .mx_2()
                    .px_2()
                    .py_1()
                    .rounded(radius::SM)
                    .text_size(font_size::MD)
                    .text_color(if selected { p.text_strong } else { p.text_3 })
                    .when(selected, |s| s.bg(p.bg_selected))
                    .hover(|s| s.bg(p.bg_hover).text_color(p.text))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| this.switch_channel(i, cx)))
                    .child(format!("# {name}"))
            }))
            .child(section_label(&format!("KİŞİLER — {}", PEOPLE.len()), p))
            .child(div().flex_1().min_h_0().child(self.members.clone()))
    }

    fn modal(&self, epoch: u64, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let close = |this: &mut Self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>| {
            this.modal_epoch = None;
            cx.notify();
        };
        div()
            .id("modal-layer")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(p.overlay)
            .occlude()
            .on_click(cx.listener(close))
            .child(
                div()
                    .id("modal")
                    .w(px(420.))
                    .p_6()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .rounded(radius::XL)
                    .border_1()
                    .border_color(p.border_strong)
                    .bg(p.bg_2)
                    .shadow_lg()
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(div().text_size(font_size::XXL).font_weight(FontWeight::BOLD).text_color(p.text_strong).child("Projeyi sil?"))
                    .child(
                        div()
                            .text_size(font_size::MD)
                            .text_color(p.text_2)
                            .child("Bu işlem projeyi ve kanallarını geri dönüşüm süresine taşır. Elastik giriş: p2-pop-elastic (ölçek yerine kayma)."),
                    )
                    .child(div().flex().justify_end().gap_2().pt_2().child(chip("modal-close", "Vazgeç", p).on_click(cx.listener(close))))
                    .p2_enter(("modal-card", epoch as usize), Entrance::PopElastic, Duration::ZERO),
            )
    }

    fn toasts(&self, p: &Palette) -> impl IntoElement {
        div()
            .absolute()
            .right(px(16.))
            .bottom(px(88.))
            .w(px(TOAST_WIDTH))
            .flex()
            .flex_col()
            .gap_2()
            .children(self.toasts.iter().map(|toast| {
                div()
                    .p_3()
                    .rounded(radius::LG)
                    .border_1()
                    .border_color(p.border_strong)
                    .bg(p.bg_2)
                    .shadow_lg()
                    .child(div().text_size(font_size::SM).font_weight(FontWeight::SEMIBOLD).text_color(p.text_strong).child(toast.title.clone()))
                    .child(div().text_size(font_size::SM).text_color(p.text_2).child(toast.body.clone()))
                    .p2_enter(("toast", toast.id as usize), Entrance::SlideRightElastic { width: TOAST_WIDTH }, Duration::ZERO)
            }))
    }
}

impl Render for Showcase {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.first_frame_logged {
            self.first_frame_logged = true;
            if let Some(started) = crate::STARTED.get() {
                println!("ilk_cizim_ms={}", started.elapsed().as_millis());
            }
        }
        trace("render kok");
        let p = cx.p2().clone();
        let mut chat_style = StyleRefinement::default();
        chat_style.flex_grow = Some(1.);
        chat_style.min_size.width = Some(px(0.).into());
        chat_style.size.height = Some(gpui_kit::relative(1.).into());
        div()
            .size_full()
            .flex()
            .flex_col()
            .relative()
            .bg(p.bg_0)
            .text_color(p.text)
            .font_family(p2_ui::tokens::FONT)
            .child(self.title_bar(&p, cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.rail(&p))
                    .child(self.sidebar(&p, cx))
                    // Önbellekli: kök her karede kurulsa da sohbet yalnız kendisi bildirim alınca kurulur.
                    .child(self.chat.clone().cached(chat_style)),
            )
            .child(self.toasts(&p))
            .when_some(self.modal_epoch, |s, epoch| s.child(self.modal(epoch, &p, cx)))
    }
}

// =================================================================== kişiler

/// Kişi listesi. Nabız animasyonları her karede yalnız bu küçük görünümü kurar.
pub struct MemberPane {
    _activation: Subscription,
}

impl MemberPane {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            // Pencere etkinliği değişince nabızlar eklenir/kaldırılır.
            _activation: cx.observe_window_activation(window, |_, window, cx| {
                trace(if window.is_window_active() { "activation true" } else { "activation false" });
                cx.notify()
            }),
        }
    }
}

impl Render for MemberPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        trace("render kisiler");
        let p = cx.p2().clone();
        let animate = window.is_window_active();
        div().id("members").size_full().overflow_y_scroll().children(PEOPLE.iter().enumerate().map(|(i, person)| {
            let (bg, fg) = p.hue(i);
            div()
                .flex()
                .items_center()
                .gap_2()
                .mx_2()
                .px_2()
                .py_1()
                .rounded(radius::SM)
                .hover(|s| s.bg(p.hover_soft))
                .child(div().relative().child(avatar(person.name, bg, fg, 28.)).child(presence_dot(person.status, i, &p, animate)))
                .child(
                    div()
                        .text_size(font_size::MD)
                        .text_color(if person.status == Status::Offline { p.text_4 } else { p.text_2 })
                        .child(person.name),
                )
        }))
    }
}

// =================================================================== sohbet

pub struct ChatPane {
    channel: usize,
    /// Kanal her değiştiğinde artar; sayfa geçiş animasyonunu yeniden başlatır.
    page_epoch: u64,
    messages: Vec<Message>,
    /// Bu oturumda gönderilenler bu indeksten başlar (yalnız onlar "rise" ile girer).
    first_sent: usize,
    list: ListState,
    composer: Entity<TextareaState>,
    _subscriptions: Vec<Subscription>,
}

impl ChatPane {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let messages = data::messages(0, MESSAGE_COUNT);
        let list = ListState::new(messages.len(), ListAlignment::Bottom, px(600.));
        let composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, 6)
                .submit_on_enter(true)
                .placeholder("#genel kanalına mesaj gönder")
        });
        let subscriptions = vec![cx.subscribe_in(&composer, window, |this, _, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { shift: false, .. } = event {
                this.send(window, cx);
            }
        })];
        Self {
            channel: 0,
            page_epoch: 0,
            first_sent: messages.len(),
            messages,
            list,
            composer,
            _subscriptions: subscriptions,
        }
    }

    fn set_channel(&mut self, channel: usize, cx: &mut Context<Self>) {
        self.channel = channel;
        self.page_epoch += 1;
        self.messages = data::messages(channel, MESSAGE_COUNT);
        self.first_sent = self.messages.len();
        self.list.reset(self.messages.len());
        cx.notify();
    }

    fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let body = self.composer.read(cx).value().trim().to_string();
        if body.is_empty() {
            return;
        }
        let minute = self.messages.last().map_or(0, |m| m.minute + 1);
        let at = self.messages.len();
        self.messages.push(Message { author: 0, minute, body });
        self.list.splice(at..at, 1);
        self.list.scroll_to_end();
        self.composer.update(cx, |state, cx| state.clean(window, cx));
        cx.notify();
    }

    fn render_message(&self, ix: usize, p: &Palette) -> AnyElement {
        let message = &self.messages[ix];
        let previous = ix.checked_sub(1).map(|i| &self.messages[i]);
        let grouped = previous.is_some_and(|prev| prev.author == message.author && message.minute - prev.minute < 5);
        let person = &PEOPLE[message.author];
        let (bg, fg) = p.hue(message.author);

        let body = div().flex().flex_col().gap_1().children(rich::parse(&message.body).into_iter().map(|block| match block {
            Block::Text { text, spans } => {
                let highlights = spans.into_iter().map(|(range, span)| (range, highlight(span, p)));
                div()
                    .text_size(font_size::LG)
                    .text_color(p.text)
                    .child(StyledText::new(text).with_highlights(highlights))
                    .into_any_element()
            }
            Block::Code { code, .. } => div()
                .px_3()
                .py_2()
                .rounded(radius::SM)
                .bg(p.code_bg)
                .border_1()
                .border_color(p.border_subtle)
                .font_family(FONT_MONO)
                .text_size(font_size::SM)
                .text_color(p.text_2)
                .child(code)
                .into_any_element(),
            // Vitrin dosya ekini yalnız ad + boyut satırı olarak gösterir.
            Block::File { name, size, .. } => div()
                .px_3()
                .py_2()
                .rounded(radius::SM)
                .bg(p.bg_3)
                .border_1()
                .border_color(p.border)
                .text_size(font_size::SM)
                .text_color(p.text_2)
                .child(format!("{name} · {size} B"))
                .into_any_element(),
        }));

        let row = div()
            .flex()
            .gap_3()
            .px_4()
            .pt(if grouped { px(2.) } else { px(12.) })
            .pb(px(2.))
            .hover(|s| s.bg(p.hover_soft))
            .child(div().w(px(36.)).flex_shrink_0().when(!grouped, |s| s.child(avatar(person.name, bg, fg, 36.))))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .when(!grouped, |s| {
                        s.child(
                            div()
                                .flex()
                                .items_baseline()
                                .gap_2()
                                .child(div().text_size(font_size::LG).font_weight(FontWeight::SEMIBOLD).text_color(fg).child(person.name))
                                .child(div().text_size(font_size::XS).text_color(p.text_4).child(data::clock(message.minute))),
                        )
                    })
                    .child(body),
            );

        if ix >= self.first_sent {
            row.p2_enter(("sent", ix), Entrance::Rise, Duration::ZERO).into_any_element()
        } else {
            row.into_any_element()
        }
    }
}

impl Render for ChatPane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        trace("render sohbet");
        let p = cx.p2().clone();
        let palette = p.clone();
        div()
            .flex()
            .flex_col()
            .size_full()
            .child(
                div()
                    .h(px(48.))
                    .px_4()
                    .flex()
                    .items_center()
                    .gap_2()
                    .border_b_1()
                    .border_color(p.border_subtle)
                    .child(div().text_color(p.text_strong).font_weight(FontWeight::SEMIBOLD).child(format!("# {}", CHANNELS[self.channel])))
                    .child(div().text_size(font_size::SM).text_color(p.text_4).child(format!("{} mesaj · sanal liste", self.messages.len()))),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .child(list(self.list.clone(), cx.processor(move |this, ix, _, _| this.render_message(ix, &palette))).flex_1().pb_2())
                    .p2_enter(("page", self.page_epoch as usize), Entrance::Page, Duration::ZERO),
            )
            .child(
                div().px_4().pb_4().child(
                    div().rounded(radius::LG).border_1().border_color(p.border).bg(p.bg_3).px_1().child(Textarea::new(&self.composer).appearance(false)),
                ),
            )
    }
}

// =================================================================== küçük parçalar

fn chip(id: &'static str, label: &'static str, p: &Palette) -> gpui_kit::Stateful<gpui_kit::Div> {
    div()
        .id(id)
        .px_2p5()
        .py_0p5()
        .rounded(radius::SM)
        .border_1()
        .border_color(p.border)
        .bg(p.bg_3)
        .text_size(font_size::SM)
        .text_color(p.text_2)
        .cursor_pointer()
        .hover(|s| s.bg(p.bg_hover).border_color(p.border_strong))
        .child(label)
}

fn section_label(text: &str, p: &Palette) -> impl IntoElement {
    div()
        .px_4()
        .pt_4()
        .pb_1()
        .text_size(font_size::XS)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(p.text_4)
        .child(SharedString::from(text.to_string()))
}

fn avatar(name: &str, bg: gpui_kit::Hsla, fg: gpui_kit::Hsla, size: f32) -> impl IntoElement {
    div()
        .size(px(size))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(bg)
        .text_color(fg)
        .text_size(px(size * 0.36))
        .font_weight(FontWeight::BOLD)
        .child(data::initials(name))
}

/// Çevrimiçi noktası. 0.9.9'da sürekli yanıp sönüyor ve pencere gizliyken de kare
/// ürettiriyordu; burada yalnız pencere etkinken nabız atar ve kare döngüsüne bağlı
/// olduğu için pencere gizlenince kendiliğinden durur.
fn presence_dot(status: Status, index: usize, p: &Palette, animate: bool) -> AnyElement {
    let color = match status {
        Status::Online => p.success,
        Status::Away => p.warning,
        Status::Busy => p.danger,
        Status::Offline => p.text_4,
    };
    let dot = div()
        .absolute()
        .right(px(-1.))
        .bottom(px(-1.))
        .size(px(10.))
        .rounded_full()
        .border_2()
        .border_color(p.bg_1)
        .bg(color);
    if status == Status::Online && animate {
        dot.with_animation(("pulse", index), pulse(), |el, t| el.opacity(pulse_opacity(t))).into_any_element()
    } else {
        dot.into_any_element()
    }
}

fn highlight(span: Span, p: &Palette) -> HighlightStyle {
    match span {
        Span::Bold => HighlightStyle { font_weight: Some(FontWeight::BOLD), color: Some(p.text_strong), ..Default::default() },
        Span::Italic => HighlightStyle { font_style: Some(FontStyle::Italic), ..Default::default() },
        Span::Code => HighlightStyle { background_color: Some(p.code_bg), color: Some(p.text_accent), ..Default::default() },
        Span::Mention => HighlightStyle {
            background_color: Some(p.mention_bg),
            color: Some(p.text_accent),
            font_weight: Some(FontWeight::MEDIUM),
            ..Default::default()
        },
    }
}
