//! Görev panosu: veri tipleri ve uçlar (server/src/tasks.ts, src/api/tasks.ts ile birebir).

use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::api::{Api, ApiError};
use crate::models::Reaction;

/// BoardPage.tsx `COLUMNS` sırası; BACKLOG sprint birikimidir, panoda "Yapılacak" sütununda görünür.
pub const BOARD_COLUMNS: [&str; 4] = ["TODO", "IN_PROGRESS", "REVIEW", "DONE"];
pub const TASK_PRIORITIES: [&str; 4] = ["LOW", "MEDIUM", "HIGH", "URGENT"];
pub const TASK_KINDS: [&str; 5] = ["TASK", "BUG", "FEATURE", "IMPROVEMENT", "DOCS"];

pub fn status_label(status: &str) -> String {
    match status {
        "BACKLOG" | "TODO" | "IN_PROGRESS" | "REVIEW" | "DONE" => p2_i18n::t(&format!("core.status.{status}")),
        _ => "—".into(),
    }
}

pub fn priority_label(priority: &str) -> String {
    match priority {
        "LOW" | "MEDIUM" | "HIGH" | "URGENT" => p2_i18n::t(&format!("core.priority.{priority}")),
        _ => "—".into(),
    }
}

pub fn kind_label(kind: &str) -> String {
    match kind {
        "BUG" | "FEATURE" | "IMPROVEMENT" | "DOCS" => p2_i18n::t(&format!("core.kind.{kind}")),
        _ => p2_i18n::t("core.kind.TASK"),
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BoardInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub position: i32,
    #[serde(default)]
    pub project_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Sprint {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub goal: Option<String>,
    /// PLANNED | ACTIVE | DONE
    pub status: String,
    #[serde(default)]
    pub starts_at: Option<String>,
    #[serde(default)]
    pub ends_at: Option<String>,
    #[serde(default)]
    pub board_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub number: i64,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    pub status: String,
    pub priority: String,
    #[serde(default)]
    pub kind: Option<String>,
    /// "ui,ses" (virgülle ayrılmış)
    #[serde(default)]
    pub labels: Option<String>,
    #[serde(default)]
    pub points: Option<i64>,
    #[serde(default)]
    pub position: i64,
    #[serde(default)]
    pub assignee_id: Option<String>,
    #[serde(default)]
    pub created_by_id: String,
    #[serde(default)]
    pub sprint_id: Option<String>,
    #[serde(default)]
    pub board_id: String,
    #[serde(default)]
    pub due_at: Option<String>,
    #[serde(default)]
    pub created_at: String,
}

impl Task {
    /// Panoda hangi sütunda: BACKLOG durumu "Yapılacak"ta görünür.
    pub fn column(&self) -> &str {
        if self.status == "BACKLOG" { "TODO" } else { &self.status }
    }

    pub fn kind(&self) -> &str {
        self.kind.as_deref().filter(|k| TASK_KINDS.contains(k)).unwrap_or("TASK")
    }

    pub fn label_list(&self) -> Vec<String> {
        self.labels.as_deref().unwrap_or_default().split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BoardContent {
    pub board: BoardInfo,
    #[serde(default)]
    pub sprints: Vec<Sprint>,
    #[serde(default)]
    pub tasks: Vec<Task>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaskComment {
    pub id: String,
    #[serde(default)]
    pub task_id: String,
    pub author_id: String,
    pub content: String,
    pub created_at: String,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub reactions: Vec<Reaction>,
}

/// parseLabels (api/tasks.ts): virgül/satır ayrımı, boşluk sadeleşir, 24 karakter, tekrar yok, en çok 10.
pub fn parse_labels(raw: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for part in raw.split([',', '\n']) {
        let label: String = part.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(24).collect();
        if !label.is_empty() && !out.iter().any(|l| l.to_lowercase() == label.to_lowercase()) {
            out.push(label);
        }
        if out.len() == 10 {
            break;
        }
    }
    out
}

fn root(org: &str, project: &str) -> String {
    format!("/organizations/{org}/projects/{project}/boards")
}

impl Api {
    /// Projedeki panolar; hiç yoksa sunucu "Genel" panosunu açar.
    pub fn boards(&self, org: &str, project: &str) -> Result<Vec<BoardInfo>, ApiError> {
        self.typed(Method::GET, &root(org, project), None, true)
    }

    pub fn create_board(&self, org: &str, project: &str, name: &str) -> Result<BoardInfo, ApiError> {
        self.typed(Method::POST, &root(org, project), Some(json!({ "name": name.trim() })), true)
    }

    pub fn rename_board(&self, org: &str, project: &str, board: &str, name: &str) -> Result<BoardInfo, ApiError> {
        self.typed(Method::PATCH, &format!("{}/{board}", root(org, project)), Some(json!({ "name": name.trim() })), true)
    }

    pub fn delete_board(&self, org: &str, project: &str, board: &str) -> Result<(), ApiError> {
        self.request(Method::DELETE, &format!("{}/{board}", root(org, project)), None, true).map(|_| ())
    }

    pub fn board(&self, org: &str, project: &str, board: &str) -> Result<BoardContent, ApiError> {
        self.typed(Method::GET, &format!("{}/{board}", root(org, project)), None, true)
    }

    /// `input`: CreateTaskDto alanları (title zorunlu).
    pub fn create_task(&self, org: &str, project: &str, board: &str, input: Value) -> Result<Task, ApiError> {
        self.typed(Method::POST, &format!("{}/{board}/tasks", root(org, project)), Some(input), true)
    }

    /// `patch`: değişen alanlar; `assigneeId: ""` atamayı kaldırır, `dueAt: null` tarihi siler.
    pub fn update_task(&self, org: &str, project: &str, board: &str, task: &str, patch: Value) -> Result<Task, ApiError> {
        self.typed(Method::PATCH, &format!("{}/{board}/tasks/{task}", root(org, project)), Some(patch), true)
    }

    pub fn delete_task(&self, org: &str, project: &str, board: &str, task: &str) -> Result<(), ApiError> {
        self.request(Method::DELETE, &format!("{}/{board}/tasks/{task}", root(org, project)), None, true).map(|_| ())
    }

    /// Sütun değiştir ve hedef sütunun yeni sırası (`order`: görev kimlikleri).
    pub fn move_task(&self, org: &str, project: &str, board: &str, task: &str, status: &str, order: &[String]) -> Result<Vec<Task>, ApiError> {
        let v = self.request(Method::PATCH, &format!("{}/{board}/tasks/{task}/move", root(org, project)), Some(json!({ "status": status, "order": order })), true)?;
        serde_json::from_value(v["tasks"].clone()).map_err(|e| ApiError { message: p2_i18n::tv("core.err.unexpected", &[("error", &e.to_string())]), ..Default::default() })
    }

    pub fn task_comments(&self, org: &str, project: &str, board: &str, task: &str) -> Result<Vec<TaskComment>, ApiError> {
        self.typed(Method::GET, &format!("{}/{board}/tasks/{task}/comments", root(org, project)), None, true)
    }

    pub fn add_task_comment(&self, org: &str, project: &str, board: &str, task: &str, content: &str) -> Result<TaskComment, ApiError> {
        self.typed(Method::POST, &format!("{}/{board}/tasks/{task}/comments", root(org, project)), Some(json!({ "content": content.trim() })), true)
    }

    pub fn delete_task_comment(&self, org: &str, project: &str, board: &str, task: &str, comment: &str) -> Result<(), ApiError> {
        self.request(Method::DELETE, &format!("{}/{board}/tasks/{task}/comments/{comment}", root(org, project)), None, true).map(|_| ())
    }

    pub fn create_sprint(&self, org: &str, project: &str, board: &str, input: Value) -> Result<Sprint, ApiError> {
        self.typed(Method::POST, &format!("{}/{board}/sprints", root(org, project)), Some(input), true)
    }

    pub fn update_sprint(&self, org: &str, project: &str, board: &str, sprint: &str, patch: Value) -> Result<Sprint, ApiError> {
        self.typed(Method::PATCH, &format!("{}/{board}/sprints/{sprint}", root(org, project)), Some(patch), true)
    }

    pub fn delete_sprint(&self, org: &str, project: &str, board: &str, sprint: &str) -> Result<(), ApiError> {
        self.request(Method::DELETE, &format!("{}/{board}/sprints/{sprint}", root(org, project)), None, true).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_normalized() {
        assert_eq!(parse_labels(" ui ,Ses,  ui,\nseviye   3 "), vec!["ui", "Ses", "seviye 3"]);
        assert_eq!(parse_labels(&(0..15).map(|i| format!("e{i}")).collect::<Vec<_>>().join(",")).len(), 10);
    }

    #[test]
    fn task_defaults() {
        let t: Task = serde_json::from_value(json!({ "id": "t", "number": 3, "title": "Ses", "status": "BACKLOG", "priority": "HIGH", "labels": "ui, ses" })).unwrap();
        assert_eq!((t.column(), t.kind(), t.label_list()), ("TODO", "TASK", vec!["ui".to_string(), "ses".to_string()]));
    }
}
