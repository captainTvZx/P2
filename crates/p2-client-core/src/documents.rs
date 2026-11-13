//! Proje dokümanları (server/src/documents.ts, DocumentsPage.tsx): klasörler, sayfa ve tuval
//! dokümanları, paylaşım yetkileri; sayfa HTML'i için kayıpsız alt küme dönüştürücü.
//!
//! İçerik Electron'la ortak olduğundan bilinmeyen alanlar `extra` ile korunur: Rust istemcisi
//! anlamadığı bir alanı hiç silmez.

use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::api::{Api, ApiError};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DocOwner {
    pub id: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub username: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDocument {
    pub id: String,
    #[serde(default)]
    pub project_id: String,
    pub owner_id: String,
    #[serde(default)]
    pub folder_id: Option<String>,
    pub title: String,
    /// CANVAS | PAGE
    pub kind: String,
    #[serde(default)]
    pub content: Value,
    #[serde(default)]
    pub updated_at: String,
    /// OWNER | WRITE | READ
    #[serde(default)]
    pub access: String,
    #[serde(default)]
    pub owner: Option<DocOwner>,
}

impl ProjectDocument {
    pub fn writable(&self) -> bool {
        self.access == "OWNER" || self.access == "WRITE"
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentFolder {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub position: i32,
}

/// Paylaşım penceresinin verisi.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocPermissions {
    pub owner: DocOwner,
    #[serde(default)]
    pub members: Vec<DocOwner>,
    #[serde(default)]
    pub permissions: Vec<DocPermission>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocPermission {
    pub user_id: String,
    pub access: String,
}

// ------------------------------------------------------------------ tuval

/// Tuval düğümü. Bilinen alanlar tipli; geri kalanı (`emoji`, `fontFamily`…) `extra`'da korunur.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CanvasNode {
    pub id: String,
    /// BOX | SPHERE | TRIANGLE | IMAGE | TEXT | NOTE (eski; BOX sayılır)
    #[serde(rename = "type")]
    pub kind: String,
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub w: f64,
    #[serde(default)]
    pub h: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CanvasWire {
    pub id: String,
    pub from: String,
    pub to: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CanvasContent {
    #[serde(default = "one")]
    pub version: i64,
    #[serde(default)]
    pub nodes: Vec<CanvasNode>,
    #[serde(default)]
    pub connections: Vec<CanvasWire>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

fn one() -> i64 {
    1
}

impl CanvasContent {
    pub fn from_value(v: &Value) -> Self {
        serde_json::from_value(v.clone()).unwrap_or(Self { version: 1, nodes: Vec::new(), connections: Vec::new(), extra: Map::new() })
    }

    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or_default()
    }
}

// ------------------------------------------------------------------ sayfa

/// Sayfa içeriği: `pages` (her biri HTML gövde) ya da eski tek `body`.
pub fn page_bodies(content: &Value) -> Vec<String> {
    if let Some(pages) = content["pages"].as_array().filter(|p| !p.is_empty()) {
        return pages.iter().map(|p| p["body"].as_str().unwrap_or_default().to_string()).collect();
    }
    vec![content["body"].as_str().unwrap_or_default().to_string()]
}

/// Sayfa gövdelerini içeriğe yazar; diğer alanlar (sayfa boyu, kenar boşlukları) korunur.
pub fn set_page_bodies(content: &Value, bodies: &[String]) -> Value {
    let mut out = content.as_object().cloned().unwrap_or_default();
    let ids: Vec<String> = content["pages"].as_array().map(|p| p.iter().map(|x| x["id"].as_str().unwrap_or_default().to_string()).collect()).unwrap_or_default();
    let pages: Vec<Value> = bodies.iter().enumerate().map(|(i, b)| json!({ "id": ids.get(i).cloned().filter(|s| !s.is_empty()).unwrap_or_else(|| format!("p{}-{}", i + 1, b.len())), "body": b })).collect();
    out.insert("version".into(), out.get("version").cloned().unwrap_or(json!(1)));
    out.insert("pages".into(), Value::Array(pages));
    out.remove("body");
    Value::Object(out)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockKind {
    P,
    H1,
    H2,
    H3,
    Quote,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DocSpan {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub link: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub kind: BlockKind,
    pub align: Align,
    pub spans: Vec<DocSpan>,
}

impl Block {
    fn new(kind: BlockKind, align: Align) -> Self {
        Self { kind, align, spans: Vec::new() }
    }

    pub fn text(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }
}

fn decode_entities(s: &str) -> String {
    s.replace("&nbsp;", " ").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&#x27;", "'").replace("&amp;", "&")
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let at = lower.find(&format!("{name}="))? + name.len() + 1;
    let rest = &tag[at..];
    let quote = rest.chars().next()?;
    if quote == '"' || quote == '\'' {
        let end = rest[1..].find(quote)? + 1;
        Some(decode_entities(&rest[1..end]))
    } else {
        Some(rest.split(|c: char| c.is_whitespace() || c == '>').next().unwrap_or_default().to_string())
    }
}

fn align_of(tag: &str) -> Option<Align> {
    let style = attr(tag, "style").unwrap_or_default().to_ascii_lowercase();
    let value = attr(tag, "align").unwrap_or_default().to_ascii_lowercase();
    if style.contains("text-align: center") || style.contains("text-align:center") || value == "center" {
        Some(Align::Center)
    } else if style.contains("text-align: right") || style.contains("text-align:right") || value == "right" {
        Some(Align::Right)
    } else if style.contains("text-align") || value == "left" {
        Some(Align::Left)
    } else {
        None
    }
}

/// Sayfa HTML'ini bloklara çevirir. İkinci değer: alt kümenin dışında bir şey (görsel,
/// renk, tablo…) varsa `false` — o sayfa Rust'ta salt okunur açılır, Electron biçimi bozulmaz.
pub fn html_to_blocks(html: &str) -> (Vec<Block>, bool) {
    let mut blocks: Vec<Block> = Vec::new();
    let mut cur = Block::new(BlockKind::P, Align::Left);
    let (mut bold, mut italic, mut underline) = (0u32, 0u32, 0u32);
    let mut link: Option<String> = None;
    let mut lossless = true;
    let mut rest = html;
    let flush = |blocks: &mut Vec<Block>, cur: &mut Block| {
        if !cur.spans.is_empty() || cur.kind != BlockKind::P {
            blocks.push(std::mem::replace(cur, Block::new(BlockKind::P, Align::Left)));
        }
    };
    while !rest.is_empty() {
        if let Some(stripped) = rest.strip_prefix('<') {
            let Some(end) = stripped.find('>') else { break };
            let tag = &stripped[..end];
            rest = &stripped[end + 1..];
            let closing = tag.starts_with('/');
            let name = tag.trim_start_matches('/').split(|c: char| c.is_whitespace() || c == '/').next().unwrap_or_default().to_ascii_lowercase();
            match (name.as_str(), closing) {
                ("p" | "div" | "h1" | "h2" | "h3" | "blockquote", false) => {
                    flush(&mut blocks, &mut cur);
                    let kind = match name.as_str() {
                        "h1" => BlockKind::H1,
                        "h2" => BlockKind::H2,
                        "h3" => BlockKind::H3,
                        "blockquote" => BlockKind::Quote,
                        _ => BlockKind::P,
                    };
                    cur = Block::new(kind, align_of(tag).unwrap_or(Align::Left));
                    // Boş blok da korunur (kullanıcının bıraktığı boş satır).
                    if kind == BlockKind::P {
                        cur.spans.push(DocSpan::default());
                    }
                }
                ("p" | "div" | "h1" | "h2" | "h3" | "blockquote", true) => {
                    // Boş yer tutucu yalnız blok tümüyle boşsa kalır.
                    if !cur_is_only_empty(&cur.spans) {
                        cur.spans.retain(|s| !s.text.is_empty());
                    }
                    blocks.push(std::mem::replace(&mut cur, Block::new(BlockKind::P, Align::Left)));
                }
                ("br", _) => {
                    // <br> paragraf içinde satır sonu: yeni boş blok olarak korunur.
                    if cur.spans.iter().all(|s| s.text.is_empty()) {
                        continue;
                    }
                    let (kind, align) = (cur.kind, cur.align);
                    blocks.push(std::mem::replace(&mut cur, Block::new(kind, align)));
                }
                ("b" | "strong", c) => bold = if c { bold.saturating_sub(1) } else { bold + 1 },
                ("i" | "em", c) => italic = if c { italic.saturating_sub(1) } else { italic + 1 },
                ("u", c) => underline = if c { underline.saturating_sub(1) } else { underline + 1 },
                ("a", false) => link = attr(tag, "href"),
                ("a", true) => link = None,
                ("span", _) => {
                    if !closing && attr(tag, "style").is_some_and(|s| !s.trim().is_empty()) {
                        lossless = false;
                    }
                }
                _ => lossless = false,
            }
        } else {
            let end = rest.find('<').unwrap_or(rest.len());
            let text = decode_entities(&rest[..end]);
            rest = &rest[end..];
            if text.is_empty() || (text.trim().is_empty() && cur.spans.iter().all(|s| s.text.is_empty()) && text.contains('\n')) {
                continue;
            }
            let span = DocSpan { text, bold: bold > 0, italic: italic > 0, underline: underline > 0, link: link.clone() };
            match cur.spans.last_mut() {
                Some(last) if last.text.is_empty() => *last = span,
                Some(last) if (last.bold, last.italic, last.underline, &last.link) == (span.bold, span.italic, span.underline, &span.link) => last.text.push_str(&span.text),
                _ => cur.spans.push(span),
            }
        }
    }
    flush(&mut blocks, &mut cur);
    (blocks, lossless)
}

fn cur_is_only_empty(spans: &[DocSpan]) -> bool {
    spans.iter().all(|s| s.text.is_empty())
}

pub fn blocks_to_html(blocks: &[Block]) -> String {
    let mut out = String::new();
    for b in blocks {
        let tag = match b.kind {
            BlockKind::P => "p",
            BlockKind::H1 => "h1",
            BlockKind::H2 => "h2",
            BlockKind::H3 => "h3",
            BlockKind::Quote => "blockquote",
        };
        let style = match b.align {
            Align::Left => String::new(),
            Align::Center => " style=\"text-align: center;\"".into(),
            Align::Right => " style=\"text-align: right;\"".into(),
        };
        out.push_str(&format!("<{tag}{style}>"));
        let text = b.text();
        if text.is_empty() {
            out.push_str("<br>");
        }
        for s in b.spans.iter().filter(|s| !s.text.is_empty()) {
            let mut t = escape(&s.text);
            if s.underline {
                t = format!("<u>{t}</u>");
            }
            if s.italic {
                t = format!("<i>{t}</i>");
            }
            if s.bold {
                t = format!("<b>{t}</b>");
            }
            if let Some(href) = &s.link {
                t = format!("<a href=\"{}\">{t}</a>", escape(href));
            }
            out.push_str(&t);
        }
        out.push_str(&format!("</{tag}>"));
    }
    out
}

/// Düzenleme biçimi (satır başına bir blok):
/// `# `, `## `, `### ` başlık · `> ` alıntı · `[orta] ` / `[sağ] ` hizalama ·
/// `**kalın**`, `*italik*`, `__altı çizili__`, `[metin](adres)`.
pub fn blocks_to_markup(blocks: &[Block]) -> String {
    blocks
        .iter()
        .map(|b| {
            let mut line = String::new();
            match b.align {
                Align::Center => line.push_str("[orta] "),
                Align::Right => line.push_str("[sağ] "),
                Align::Left => {}
            }
            line.push_str(match b.kind {
                BlockKind::H1 => "# ",
                BlockKind::H2 => "## ",
                BlockKind::H3 => "### ",
                BlockKind::Quote => "> ",
                BlockKind::P => "",
            });
            for s in b.spans.iter().filter(|s| !s.text.is_empty()) {
                let mut t = s.text.clone();
                if s.underline {
                    t = format!("__{t}__");
                }
                if s.italic {
                    t = format!("*{t}*");
                }
                if s.bold {
                    t = format!("**{t}**");
                }
                if let Some(href) = &s.link {
                    t = format!("[{t}]({href})");
                }
                line.push_str(&t);
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_inline(text: &str) -> Vec<DocSpan> {
    let mut spans: Vec<DocSpan> = Vec::new();
    let (mut bold, mut italic, mut underline) = (false, false, false);
    let mut buf = String::new();
    let push = |spans: &mut Vec<DocSpan>, buf: &mut String, b: bool, i: bool, u: bool, link: Option<String>| {
        if !buf.is_empty() {
            spans.push(DocSpan { text: std::mem::take(buf), bold: b, italic: i, underline: u, link });
        }
    };
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let rest: String = chars[i..].iter().collect();
        if rest.starts_with("**") {
            push(&mut spans, &mut buf, bold, italic, underline, None);
            bold = !bold;
            i += 2;
        } else if rest.starts_with("__") {
            push(&mut spans, &mut buf, bold, italic, underline, None);
            underline = !underline;
            i += 2;
        } else if chars[i] == '*' {
            push(&mut spans, &mut buf, bold, italic, underline, None);
            italic = !italic;
            i += 1;
        } else if chars[i] == '[' {
            // [metin](adres)
            if let Some(close) = rest.find("](") {
                if let Some(end) = rest[close + 2..].find(')') {
                    push(&mut spans, &mut buf, bold, italic, underline, None);
                    let inner = &rest[1..close];
                    let href = &rest[close + 2..close + 2 + end];
                    for mut s in parse_inline(inner) {
                        s.bold |= bold;
                        s.italic |= italic;
                        s.underline |= underline;
                        s.link = Some(href.to_string());
                        spans.push(s);
                    }
                    i += rest[..close + 2 + end + 1].chars().count();
                    continue;
                }
            }
            buf.push('[');
            i += 1;
        } else {
            buf.push(chars[i]);
            i += 1;
        }
    }
    push(&mut spans, &mut buf, bold, italic, underline, None);
    spans
}

pub fn markup_to_blocks(text: &str) -> Vec<Block> {
    text.lines()
        .map(|line| {
            let mut l = line;
            let mut align = Align::Left;
            if let Some(r) = l.strip_prefix("[orta] ") {
                align = Align::Center;
                l = r;
            } else if let Some(r) = l.strip_prefix("[sağ] ") {
                align = Align::Right;
                l = r;
            }
            let (kind, body) = if let Some(r) = l.strip_prefix("### ") {
                (BlockKind::H3, r)
            } else if let Some(r) = l.strip_prefix("## ") {
                (BlockKind::H2, r)
            } else if let Some(r) = l.strip_prefix("# ") {
                (BlockKind::H1, r)
            } else if let Some(r) = l.strip_prefix("> ") {
                (BlockKind::Quote, r)
            } else {
                (BlockKind::P, l)
            };
            let mut spans = parse_inline(body);
            if spans.is_empty() {
                spans.push(DocSpan::default());
            }
            Block { kind, align, spans }
        })
        .collect()
}

// ------------------------------------------------------------------ uçlar

fn root(org: &str, project: &str) -> String {
    format!("/organizations/{org}/projects/{project}/documents")
}

impl Api {
    pub fn documents(&self, org: &str, project: &str) -> Result<Vec<ProjectDocument>, ApiError> {
        self.typed(Method::GET, &root(org, project), None, true)
    }

    pub fn document_folders(&self, org: &str, project: &str) -> Result<Vec<DocumentFolder>, ApiError> {
        self.typed(Method::GET, &format!("{}/folders", root(org, project)), None, true)
    }

    pub fn create_document_folder(&self, org: &str, project: &str, name: &str) -> Result<DocumentFolder, ApiError> {
        self.typed(Method::POST, &format!("{}/folders", root(org, project)), Some(json!({ "name": name.trim() })), true)
    }

    pub fn rename_document_folder(&self, org: &str, project: &str, folder: &str, name: &str) -> Result<DocumentFolder, ApiError> {
        self.typed(Method::PATCH, &format!("{}/folders/{folder}", root(org, project)), Some(json!({ "name": name.trim() })), true)
    }

    pub fn delete_document_folder(&self, org: &str, project: &str, folder: &str) -> Result<(), ApiError> {
        self.request(Method::DELETE, &format!("{}/folders/{folder}", root(org, project)), None, true).map(|_| ())
    }

    pub fn create_document(&self, org: &str, project: &str, title: &str, kind: &str, folder: Option<&str>) -> Result<ProjectDocument, ApiError> {
        self.typed(Method::POST, &root(org, project), Some(json!({ "title": title.trim(), "kind": kind, "folderId": folder })), true)
    }

    pub fn document(&self, org: &str, project: &str, id: &str) -> Result<ProjectDocument, ApiError> {
        self.typed(Method::GET, &format!("{}/{id}", root(org, project)), None, true)
    }

    /// `patch`: `title`, `content`, `folderId` (null = klasörden çıkar).
    pub fn update_document(&self, org: &str, project: &str, id: &str, patch: Value) -> Result<ProjectDocument, ApiError> {
        self.typed(Method::PATCH, &format!("{}/{id}", root(org, project)), Some(patch), true)
    }

    pub fn delete_document(&self, org: &str, project: &str, id: &str) -> Result<(), ApiError> {
        self.request(Method::DELETE, &format!("{}/{id}", root(org, project)), None, true).map(|_| ())
    }

    pub fn document_permissions(&self, org: &str, project: &str, id: &str) -> Result<DocPermissions, ApiError> {
        self.typed(Method::GET, &format!("{}/{id}/permissions", root(org, project)), None, true)
    }

    /// `access`: READ | WRITE | NONE (kaldır).
    pub fn set_document_access(&self, org: &str, project: &str, id: &str, user: &str, access: &str) -> Result<(), ApiError> {
        let path = format!("{}/{id}/permissions/{user}", root(org, project));
        if access == "NONE" {
            self.request(Method::DELETE, &path, None, true).map(|_| ())
        } else {
            self.request(Method::PUT, &path, Some(json!({ "access": access })), true).map(|_| ())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_subset_roundtrips_through_markup() {
        let html = "<h1>Oyun tasarımı</h1><p>Bu bir <b>kalın</b> ve <i>italik</i> <u>metin</u>, <a href=\"https://p2.dev\">bağlantı</a>.</p><p style=\"text-align: center;\">Ortada</p><blockquote>Alıntı</blockquote><p><br></p><h2>Alt başlık</h2>";
        let (blocks, lossless) = html_to_blocks(html);
        assert!(lossless);
        assert_eq!(blocks.len(), 6, "{blocks:#?}");
        assert_eq!(blocks[0].kind, BlockKind::H1);
        assert_eq!(blocks[2].align, Align::Center);
        let markup = blocks_to_markup(&blocks);
        assert_eq!(markup, "# Oyun tasarımı\nBu bir **kalın** ve *italik* __metin__, [bağlantı](https://p2.dev).\n[orta] Ortada\n> Alıntı\n\n## Alt başlık");
        let back = markup_to_blocks(&markup);
        assert_eq!(back, blocks);
        assert_eq!(html_to_blocks(&blocks_to_html(&back)).0, blocks);
    }

    #[test]
    fn unsupported_markup_is_flagged() {
        assert!(!html_to_blocks("<p>Görsel <img src=\"x.png\"></p>").1);
        assert!(!html_to_blocks("<p><span style=\"color:red\">kırmızı</span></p>").1);
        assert!(html_to_blocks("<p>düz &amp; temiz&nbsp;metin</p>").1);
        assert_eq!(html_to_blocks("<p>a &amp; b</p>").0[0].text(), "a & b");
    }

    #[test]
    fn canvas_keeps_unknown_fields() {
        let v = json!({ "version": 2, "grid": "DOTS", "viewport": { "x": 1, "y": 2, "zoom": 1.5 }, "nodes": [{ "id": "n", "type": "BOX", "x": 1.0, "y": 2.0, "w": 100.0, "h": 50.0, "text": "kutu", "emoji": "🎮", "fontSize": 18 }], "connections": [{ "id": "w", "from": "n", "to": "m", "fromSide": "right", "toSide": "left" }] });
        let c = CanvasContent::from_value(&v);
        assert_eq!(c.nodes[0].extra["emoji"], "🎮");
        assert_eq!(c.to_value(), v);
    }

    #[test]
    fn page_bodies_preserve_layout_fields() {
        let v = json!({ "version": 1, "pageSize": "A4", "pages": [{ "id": "a", "body": "<p>1</p>" }] });
        let out = set_page_bodies(&v, &["<p>1</p>".into(), "<p>2</p>".into()]);
        assert_eq!(out["pageSize"], "A4");
        assert_eq!(out["pages"][0]["id"], "a");
        assert_eq!(page_bodies(&out), vec!["<p>1</p>", "<p>2</p>"]);
        assert_eq!(page_bodies(&json!({ "body": "<p>eski</p>" })), vec!["<p>eski</p>"]);
    }
}
