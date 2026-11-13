//! Mesaj zengin metni: `**kalın**`, `*italik*`, `` `kod` ``, `@bahsetme`, ```` ``` ```` kod blokları.
//! (`MessageRichText.tsx`'in Faz 0 kapsamındaki alt kümesi.)

use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Span {
    Bold,
    Italic,
    Code,
    Mention,
}

#[derive(Debug, PartialEq)]
pub enum Block {
    /// Düz metin ve stil aralıkları (bayt aralıkları, işaretler çıkarılmış metne göre).
    Text { text: String, spans: Vec<(Range<usize>, Span)> },
    Code { lang: String, code: String },
    /// `[[p2-file|ad|tür|boyut|yol]]` (attachments.tsx `attachmentMarker`).
    File { name: String, mime: String, size: u64, url: String },
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Bir satır tam olarak dosya işaretiyse ayrıştırır.
fn file_marker(line: &str) -> Option<Block> {
    let inner = line.trim().strip_prefix("[[p2-file|")?.strip_suffix("]]")?;
    let mut parts = inner.splitn(4, '|');
    let (name, mime, size, url) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    if !(url.starts_with('/') || url.starts_with("http://") || url.starts_with("https://")) {
        return None;
    }
    Some(Block::File { name: percent_decode(name), mime: percent_decode(mime), size: size.parse().ok()?, url: url.to_string() })
}

/// Dosya işaretlerini satır satır ayırıp kalan metni zengin metin olarak ayrıştırır.
pub fn parse_message(source: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut text = String::new();
    for line in source.split('\n') {
        if let Some(file) = file_marker(line) {
            if !text.trim().is_empty() {
                blocks.extend(parse(&text));
            }
            text.clear();
            blocks.push(file);
        } else {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(line);
        }
    }
    if !text.trim().is_empty() {
        blocks.extend(parse(&text));
    }
    blocks
}

pub fn parse(source: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut rest = source;
    while let Some(start) = rest.find("```") {
        push_text(&mut blocks, &rest[..start]);
        let after = &rest[start + 3..];
        let Some(end) = after.find("```") else {
            push_text(&mut blocks, &rest[start..]);
            rest = "";
            break;
        };
        let inner = &after[..end];
        let (lang, code) = match inner.split_once('\n') {
            Some((lang, code)) => (lang.trim().to_string(), code.trim_end_matches('\n').to_string()),
            None => (String::new(), inner.to_string()),
        };
        blocks.push(Block::Code { lang, code });
        rest = &after[end + 3..];
    }
    push_text(&mut blocks, rest);
    blocks
}

fn push_text(blocks: &mut Vec<Block>, source: &str) {
    let source = source.trim_matches('\n');
    if source.is_empty() {
        return;
    }
    let (text, spans) = inline(source);
    blocks.push(Block::Text { text, spans });
}

fn inline(source: &str) -> (String, Vec<(Range<usize>, Span)>) {
    let mut out = String::with_capacity(source.len());
    let mut spans = Vec::new();
    let mut i = 0;
    let bytes = source.as_bytes();
    while i < source.len() {
        let rest = &source[i..];
        let delimited = [("**", Span::Bold), ("`", Span::Code), ("*", Span::Italic)]
            .into_iter()
            .find_map(|(mark, span)| {
                let body = rest.strip_prefix(mark)?;
                let end = body.find(mark)?;
                (end > 0).then(|| (mark.len(), &body[..end], span))
            });
        if let Some((mark_len, body, span)) = delimited {
            let start = out.len();
            out.push_str(body);
            spans.push((start..out.len(), span));
            i += mark_len * 2 + body.len();
            continue;
        }
        if bytes[i] == b'@' && (i == 0 || bytes[i - 1].is_ascii_whitespace()) {
            let len = rest[1..]
                .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-'))
                .unwrap_or(rest.len() - 1);
            if len > 0 {
                let start = out.len();
                out.push_str(&rest[..len + 1]);
                spans.push((start..out.len(), Span::Mention));
                i += len + 1;
                continue;
            }
        }
        let ch = rest.chars().next().expect("i bir karakter sınırında");
        out.push(ch);
        i += ch.len_utf8();
    }
    (out, spans)
}

// ---------------------------------------------------------------- MessageRichText.tsx (birebir)

/// Satır içi parça (MessageRichText.tsx `inline`): `tokenPattern` ile bölünen metin.
#[derive(Clone, Debug, PartialEq)]
pub enum Inline {
    Text(String),
    Bold(String),
    Italic(String),
    /// `[etiket](url)` ya da çıplak `https://…`
    Link { label: String, url: String },
    /// `@kullanıcı` (başında @ ile)
    Mention(String),
}

/// `rich-message` bloğu: `<p>` (satırlar, `<br/>` ile) ya da ```` ``` ```` kod bloğu.
#[derive(Clone, Debug, PartialEq)]
pub enum RichBlock {
    Para(Vec<Vec<Inline>>),
    Code(String),
}

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')
}

/// `tokenPattern` alternatiflerini (sırasıyla) `s`'in başında dener; eşleşmenin bayt uzunluğu.
fn token_at(s: &str) -> Option<(usize, Inline)> {
    // \*\*[^*\n]+\*\*
    if let Some(body) = s.strip_prefix("**") {
        let end = body.find(|c| c == '*' || c == '\n');
        if let Some(e) = end.filter(|e| *e > 0) {
            if body[e..].starts_with("**") {
                return Some((e + 4, Inline::Bold(body[..e].to_string())));
            }
        }
    }
    // \*[^*\n]+\*
    if let Some(body) = s.strip_prefix('*') {
        let end = body.find(|c| c == '*' || c == '\n');
        if let Some(e) = end.filter(|e| *e > 0) {
            if body[e..].starts_with('*') {
                return Some((e + 2, Inline::Italic(body[..e].to_string())));
            }
        }
    }
    // \[[^\]]+\]\(https?:\/\/[^\s)]+\)
    if let Some(body) = s.strip_prefix('[') {
        if let Some(close) = body.find(']').filter(|e| *e > 0) {
            let rest = &body[close + 1..];
            if let Some(inner) = rest.strip_prefix('(') {
                if inner.starts_with("http://") || inner.starts_with("https://") {
                    let url_len = inner.find(|c: char| c.is_whitespace() || c == ')').unwrap_or(inner.len());
                    let scheme = if inner.starts_with("https://") { 8 } else { 7 };
                    if url_len > scheme && inner[url_len..].starts_with(')') {
                        let total = 1 + close + 1 + 1 + url_len + 1;
                        return Some((total, Inline::Link { label: body[..close].to_string(), url: inner[..url_len].to_string() }));
                    }
                }
            }
        }
    }
    // https?:\/\/[^\s]+
    if s.starts_with("http://") || s.starts_with("https://") {
        let scheme = if s.starts_with("https://") { 8 } else { 7 };
        let len = s.find(char::is_whitespace).unwrap_or(s.len());
        if len > scheme {
            return Some((len, Inline::Link { label: s[..len].to_string(), url: s[..len].to_string() }));
        }
    }
    // @[a-zA-Z0-9_.-]+
    if let Some(body) = s.strip_prefix('@') {
        let len = body.find(|c: char| !is_ident(c)).unwrap_or(body.len());
        if len > 0 {
            return Some((len + 1, Inline::Mention(s[..len + 1].to_string())));
        }
    }
    None
}

/// Bir satırı `tokenPattern` ile böler (`text.split(tokenPattern).filter(Boolean)`).
pub fn inline_tokens(line: &str) -> Vec<Inline> {
    let mut out = Vec::new();
    let mut plain = String::new();
    let mut i = 0;
    while i < line.len() {
        if let Some((len, token)) = token_at(&line[i..]) {
            if !plain.is_empty() {
                out.push(Inline::Text(std::mem::take(&mut plain)));
            }
            out.push(token);
            i += len;
            continue;
        }
        let ch = line[i..].chars().next().expect("karakter sınırı");
        plain.push(ch);
        i += ch.len_utf8();
    }
    if !plain.is_empty() {
        out.push(Inline::Text(plain));
    }
    out
}

/// `withoutCodeFences`: baştaki/sondaki çit satırlarını (``` ya da ''') ayıklar.
pub fn without_code_fences(value: &str) -> String {
    let mut clean = value.trim().to_string();
    loop {
        let previous = clean.clone();
        // ^(?:`+|'{3,})(?:[a-zA-Z0-9_+-]+)?\s*(?:\r?\n|$)
        let ticks = clean.chars().take_while(|c| *c == '`').count();
        let quotes = clean.chars().take_while(|c| *c == '\'').count();
        let fence = if ticks > 0 { ticks } else if quotes >= 3 { quotes } else { 0 };
        if fence > 0 {
            let rest = &clean[fence..];
            let lang = rest.find(|c: char| !(c.is_ascii_alphanumeric() || "_+-".contains(c))).unwrap_or(rest.len());
            let after = &rest[lang..];
            let ws = after.find(|c: char| !c.is_whitespace()).unwrap_or(after.len());
            if after[..ws].contains('\n') || ws == after.len() {
                // \s* sonrasında satır sonu ya da metin sonu: ilk satır sonuna kadar kesilir.
                let cut = match after[..ws].find('\n') {
                    Some(n) => fence + lang + n + 1,
                    None => clean.len(),
                };
                clean = clean[cut..].to_string();
            }
        }
        // (?:^|\r?\n)\s*(?:`+|'{3,})\s*$
        let trimmed = clean.trim_end();
        let line_start = trimmed.rfind('\n').map_or(0, |n| n + 1);
        let last = trimmed[line_start..].trim();
        if !last.is_empty() && (last.chars().all(|c| c == '`') || (last.len() >= 3 && last.chars().all(|c| c == '\''))) {
            clean = trimmed[..line_start].trim_end_matches(['\n', '\r']).to_string();
        }
        clean = clean.trim().to_string();
        if clean == previous {
            return clean;
        }
    }
}

/// `MessageRichText` bloklar: `visible.split(/(```[\s\S]*?```)/g).filter(Boolean)`.
pub fn rich_blocks(visible: &str) -> Vec<RichBlock> {
    let mut parts: Vec<&str> = Vec::new();
    let mut rest = visible;
    while let Some(start) = rest.find("```") {
        let Some(end) = rest[start + 3..].find("```") else { break };
        parts.push(&rest[..start]);
        parts.push(&rest[start..start + 3 + end + 3]);
        rest = &rest[start + 3 + end + 3..];
    }
    parts.push(rest);
    parts
        .into_iter()
        .filter(|b| !b.is_empty())
        .map(|block| {
            if block.len() >= 6 && block.starts_with("```") && block.ends_with("```") {
                let inner = &block[3..block.len() - 3];
                let inner = inner.strip_prefix('\n').unwrap_or(inner);
                let inner = inner.strip_suffix('\n').unwrap_or(inner);
                RichBlock::Code(without_code_fences(inner))
            } else {
                // <p>: satırlar <br/> ile; sondaki tek <br/> HTML'de yeni satır açmaz.
                let mut lines: Vec<Vec<Inline>> = block.split('\n').map(inline_tokens).collect();
                if lines.len() > 1 && lines.last().is_some_and(Vec::is_empty) {
                    lines.pop();
                }
                RichBlock::Para(lines)
            }
        })
        .collect()
}

/// `12,4 KB` biçiminde boyut.
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut v = bytes as f64;
    let mut unit = 0;
    while v >= 1024. && unit < UNITS.len() - 1 {
        v /= 1024.;
        unit += 1;
    }
    if unit == 0 { format!("{bytes} B") } else { format!("{:.1} {}", v, UNITS[unit]).replace('.', ",") }
}

/// Mesaj içeriğini (MessageRichText.tsx alt kümesi) P2 stiliyle çizer.
pub fn render_message(content: &str, p: &crate::tokens::Palette) -> gpui_kit::AnyElement {
    use gpui_kit::prelude::*;
    use gpui_kit::{FontStyle, FontWeight, HighlightStyle, StyledText, div, px};

    use crate::tokens::{FONT_MONO, font_size, radius};

    let highlight = |span: Span| match span {
        Span::Bold => HighlightStyle { font_weight: Some(FontWeight::BOLD), color: Some(p.text_strong), ..Default::default() },
        Span::Italic => HighlightStyle { font_style: Some(FontStyle::Italic), ..Default::default() },
        Span::Code => HighlightStyle { background_color: Some(p.code_bg), color: Some(p.text_accent), ..Default::default() },
        Span::Mention => HighlightStyle { background_color: Some(p.mention_bg), color: Some(p.text_accent), font_weight: Some(FontWeight::MEDIUM), ..Default::default() },
    };
    div()
        .flex()
        .flex_col()
        .gap_1()
        .children(parse_message(content).into_iter().map(|block| match block {
            Block::Text { text, spans } => div()
                .text_size(font_size::LG)
                .text_color(p.text)
                .child(StyledText::new(text).with_highlights(spans.into_iter().map(|(r, s)| (r, highlight(s)))))
                .into_any_element(),
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
            Block::File { name, size, .. } => div()
                .flex()
                .items_center()
                .gap_2()
                .max_w(px(420.))
                .px_3()
                .py_2()
                .rounded(radius::MD)
                .border_1()
                .border_color(p.border)
                .bg(p.bg_3)
                .child(div().text_size(px(18.)).child("📎"))
                .child(div().flex_1().min_w_0().flex().flex_col().child(div().truncate().text_color(p.text_strong).child(name)).child(div().text_size(font_size::XS).text_color(p.text_4).child(human_size(size))))
                .into_any_element(),
        }))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_markers_and_sizes() {
        let blocks = parse_message("bak:\n[[p2-file|rapor%20son.pdf|application%2Fpdf|12700|/uploads/a.pdf]]\nsonra");
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[1], Block::File { name: "rapor son.pdf".into(), mime: "application/pdf".into(), size: 12700, url: "/uploads/a.pdf".into() });
        assert_eq!(human_size(12700), "12,4 KB");
        assert_eq!(human_size(512), "512 B");
    }

    #[test]
    fn inline_styles() {
        let blocks = parse("a **kalın** ve *eğik* `kod` @cem!");
        let Block::Text { text, spans } = &blocks[0] else { panic!() };
        assert_eq!(text, "a kalın ve eğik kod @cem!");
        let styled: Vec<_> = spans.iter().map(|(r, s)| (&text[r.clone()], *s)).collect();
        assert_eq!(
            styled,
            [("kalın", Span::Bold), ("eğik", Span::Italic), ("kod", Span::Code), ("@cem", Span::Mention)]
        );
    }

    #[test]
    fn code_block() {
        let blocks = parse("önce\n```rust\nfn a() {}\n```\nsonra");
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[1], Block::Code { lang: "rust".into(), code: "fn a() {}".into() });
    }

    #[test]
    fn electron_tokens() {
        let t = inline_tokens("Merhaba **ekip**! _İtalik_, ~~x~~ `kod` https://example.com/p2 @rust.bot [a](https://b.c)");
        assert_eq!(t[0], Inline::Text("Merhaba ".into()));
        assert_eq!(t[1], Inline::Bold("ekip".into()));
        assert!(t.contains(&Inline::Link { label: "https://example.com/p2".into(), url: "https://example.com/p2".into() }));
        assert!(t.contains(&Inline::Mention("@rust.bot".into())));
        assert!(t.contains(&Inline::Link { label: "a".into(), url: "https://b.c".into() }));
        // Satır içi kod ve alt çizgi Electron'da düz metin kalır.
        assert!(matches!(&t[2], Inline::Text(s) if s.contains("`kod`") && s.contains("_İtalik_")));
        let b = rich_blocks("şu:\n```js\nconst a = 1\n```");
        assert_eq!(b, vec![RichBlock::Para(vec![vec![Inline::Text("şu:".into())]]), RichBlock::Code("js\nconst a = 1".into())]);
    }

    #[test]
    fn email_is_not_mention_and_unclosed_marks_stay() {
        let blocks = parse("a@b.com *yarım");
        let Block::Text { text, spans } = &blocks[0] else { panic!() };
        assert_eq!(text, "a@b.com *yarım");
        assert!(spans.is_empty());
    }
}
