//! Tanılama: gömülü Inter'in GPUI'de hangi aile adı/ağırlıklarla çözüldüğünü yazar.
//!   cargo run --release -p p2-ui --example fonts
use gpui_kit::{FontWeight, font};

fn main() {
    gpui_kit::application().run(|cx| {
        p2_ui::fonts::install(cx);
        let names: Vec<String> = cx.text_system().all_font_names().into_iter().filter(|n| n.to_lowercase().contains("inter")).collect();
        println!("aileler: {names:?}");
        for w in [400., 500., 600., 700., 800., 900.] {
            let mut f = font(std::env::args().nth(1).unwrap_or(p2_ui::tokens::FONT.into()));
            f.weight = FontWeight(w);
            let id = cx.text_system().resolve_font(&f);
            let width = cx.text_system().advance(id, gpui_kit::px(100.), 'W').map(|s| f32::from(s.width)).unwrap_or(0.);
            println!("ağırlık {w}: font_id {id:?} W genişliği {width:.2}");
        }
        cx.quit();
    });
}
