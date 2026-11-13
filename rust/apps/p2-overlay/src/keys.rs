//! DLL'den gelen girdi (şema 4 InputMsg) → GPUI olayları. Electron `onInput` karşılığı:
//! fare (düğme, çift tık sayımı, tekerlek), tuşlar (VK → GPUI tuş adı), karakterler
//! (UTF-16 vekil çiftleri birleştirilir), değiştirici tuş durumu.

use std::time::Instant;

use gpui_kit::{
    Keystroke, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PlatformInput, Pixels, Point, ScrollDelta,
    ScrollEvent, point, px,
};
use p2_overlay_ipc::{InputMsg, input};

/// Pencereye gönderilecek şey: ham olay ya da tuş vuruşu (metin alanına karakter girer).
pub enum Out {
    Event(PlatformInput),
    Keystroke(Keystroke),
}

#[derive(Default)]
pub struct InputState {
    shift: bool,
    control: bool,
    alt: bool,
    left: bool,
    right: bool,
    middle: bool,
    surrogate: u16,
    last_click: Option<(Instant, f32, f32, usize)>,
    pub position: Point<Pixels>,
}

fn vk_name(vk: u32) -> Option<&'static str> {
    Some(match vk {
        0x08 => "backspace",
        0x09 => "tab",
        0x0d => "enter",
        0x1b => "escape",
        0x21 => "pageup",
        0x22 => "pagedown",
        0x23 => "end",
        0x24 => "home",
        0x25 => "left",
        0x26 => "up",
        0x27 => "right",
        0x28 => "down",
        0x2e => "delete",
        _ => return None,
    })
}

impl InputState {
    fn modifiers(&self) -> Modifiers {
        Modifiers { shift: self.shift, control: self.control, alt: self.alt, ..Default::default() }
    }

    fn pressed(&self) -> Option<MouseButton> {
        if self.left {
            Some(MouseButton::Left)
        } else if self.right {
            Some(MouseButton::Right)
        } else if self.middle {
            Some(MouseButton::Middle)
        } else {
            None
        }
    }

    /// Odak kaybı: basılı değiştiriciler ve düğmeler bırakılır.
    pub fn reset(&mut self) {
        let position = self.position;
        *self = Self { position, ..Default::default() };
    }

    pub fn translate(&mut self, msg: &InputMsg) -> Option<Out> {
        // DLL bayrakları: bit1 Shift, bit2 Ctrl, bit3 Alt (fare olaylarında güncel durum)
        if matches!(msg.kind, input::MOUSE_MOVE | input::MOUSE_BTN | input::MOUSE_WHEEL) {
            self.shift = msg.flags & 0x2 != 0;
            self.control = msg.flags & 0x4 != 0;
            self.alt = msg.flags & 0x8 != 0;
        }
        let pos = point(px(msg.x as f32), px(msg.y as f32));
        match msg.kind {
            input::MOUSE_MOVE => {
                self.position = pos;
                Some(Out::Event(PlatformInput::MouseMove(MouseMoveEvent { position: pos, pressed_button: self.pressed(), modifiers: self.modifiers() })))
            }
            input::MOUSE_BTN => {
                self.position = pos;
                let down = msg.flags & input::FLAG_DOWN != 0;
                let button = match msg.key {
                    2 => MouseButton::Right,
                    3 => MouseButton::Middle,
                    _ => MouseButton::Left,
                };
                match button {
                    MouseButton::Right => self.right = down,
                    MouseButton::Middle => self.middle = down,
                    _ => self.left = down,
                }
                let (x, y) = (msg.x as f32, msg.y as f32);
                let count = if down {
                    let now = Instant::now();
                    let n = match self.last_click {
                        Some((t, lx, ly, n)) if now.duration_since(t).as_millis() <= 500 && (x - lx).abs() <= 4. && (y - ly).abs() <= 4. => (n + 1).min(3),
                        _ => 1,
                    };
                    self.last_click = Some((now, x, y, n));
                    n
                } else {
                    self.last_click.map(|c| c.3).unwrap_or(1)
                };
                Some(Out::Event(if down {
                    PlatformInput::MouseDown(MouseDownEvent { button, position: pos, modifiers: self.modifiers(), click_count: count, first_mouse: false })
                } else {
                    PlatformInput::MouseUp(MouseUpEvent { button, position: pos, modifiers: self.modifiers(), click_count: count })
                }))
            }
            input::MOUSE_WHEEL => {
                let horizontal = msg.flags & 0x100 != 0;
                let lines = msg.wheel_delta as f32 / 120.;
                let delta = if horizontal { point(-lines, 0.) } else { point(0., lines) };
                Some(Out::Event(PlatformInput::Scroll(ScrollEvent {
                    position: self.position,
                    delta: ScrollDelta::Lines(delta),
                    modifiers: self.modifiers(),
                    touch_phase: Default::default(),
                    source: Default::default(),
                })))
            }
            input::KEY => {
                let down = msg.flags & input::FLAG_DOWN != 0;
                match msg.key {
                    0x10 | 0xa0 | 0xa1 => self.shift = down,
                    0x11 | 0xa2 | 0xa3 => self.control = down,
                    0x12 | 0xa4 | 0xa5 => self.alt = down,
                    _ => {}
                }
                if !down {
                    return None;
                }
                // Harf/rakam: yalnız Ctrl/Alt ile kısayol olarak (düz yazı WM_CHAR'dan gelir).
                let key = match vk_name(msg.key) {
                    Some(name) => name.to_string(),
                    None if (self.control || self.alt) && matches!(msg.key, 0x30..=0x39 | 0x41..=0x5a) => {
                        char::from(msg.key as u8).to_ascii_lowercase().to_string()
                    }
                    None => return None,
                };
                // Boşluk/Enter/Tab karakter olarak da gelir: çift girişi önlemek için tuşta karakter yok.
                Some(Out::Keystroke(Keystroke { modifiers: self.modifiers(), key, key_char: None }))
            }
            input::CHAR => {
                let unit = (msg.key & 0xffff) as u16;
                // Kontrol karakterleri (Ctrl+V → 0x16, Backspace 0x08, Enter 0x0d) tuş olarak işlendi.
                if unit < 0x20 {
                    return None;
                }
                if (0xd800..=0xdbff).contains(&unit) {
                    self.surrogate = unit;
                    return None;
                }
                let text = if (0xdc00..=0xdfff).contains(&unit) && self.surrogate != 0 {
                    let s = String::from_utf16_lossy(&[self.surrogate, unit]);
                    self.surrogate = 0;
                    s
                } else {
                    String::from_utf16_lossy(&[unit])
                };
                if text == " " {
                    return Some(Out::Keystroke(Keystroke { modifiers: Modifiers::default(), key: "space".into(), key_char: Some(" ".into()) }));
                }
                Some(Out::Keystroke(Keystroke {
                    modifiers: Modifiers { shift: self.shift, ..Default::default() },
                    key: text.to_lowercase(),
                    key_char: Some(text),
                }))
            }
            _ => None,
        }
    }
}
