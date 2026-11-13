//! Kütüphane parça deposu — `server/src/releaseStore.ts` karşılığı: içerik adresli (sha256)
//! en çok 4 MB parçalar diskte tutulur, aynı parça bütün sürümler/projeler arasında tek kez saklanır.
//!   <P2_RELEASE_DIR>/chunks/ab/cd/<hash>   kalıcı, değişmez
//!   <P2_RELEASE_DIR>/tmp/<hash>.<uuid>.part yazılırken; açılışta süpürülür

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use axum::body::{Body, HttpBody};
use axum::http::StatusCode;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use super::release_manifest::{CHUNK_SIZE, is_hash};
use crate::error::ApiError;

pub fn root() -> &'static Path {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(|| match std::env::var("P2_RELEASE_DIR") {
        Ok(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => std::env::current_dir().unwrap_or_default().join(".p2-data").join("releases"),
    })
}

pub fn chunk_path(hash: &str) -> PathBuf {
    root().join("chunks").join(&hash[0..2]).join(&hash[2..4]).join(hash)
}

fn tmp_dir() -> PathBuf {
    root().join("tmp")
}

pub fn ensure_dirs() -> std::io::Result<()> {
    std::fs::create_dir_all(root().join("chunks"))?;
    std::fs::create_dir_all(tmp_dir())
}

/// Yarım kalan yazımlar sunucu yeniden başlayınca anlamsızdır; istemci "eksik parçalar"
/// sorusuyla gerçek durumu öğrenip devam eder.
pub fn sweep_tmp() -> std::io::Result<()> {
    let _ = std::fs::remove_dir_all(tmp_dir());
    std::fs::create_dir_all(tmp_dir())
}

pub async fn chunk_exists(hash: &str) -> bool {
    tokio::fs::metadata(chunk_path(hash)).await.is_ok_and(|m| m.is_file() && m.len() > 0)
}

/// Diskte olmayan parçalar (64'lük kümeler hâlinde eşzamanlı bakılır, sıra korunur).
pub async fn missing_chunks(hashes: &[String]) -> Vec<String> {
    let mut missing = Vec::new();
    for batch in hashes.chunks(64) {
        let mut set = tokio::task::JoinSet::new();
        for (i, hash) in batch.iter().enumerate() {
            let hash = hash.clone();
            set.spawn(async move { (i, chunk_exists(&hash).await) });
        }
        let mut present = vec![false; batch.len()];
        while let Some(Ok((i, ok))) = set.join_next().await {
            present[i] = ok;
        }
        missing.extend(batch.iter().zip(present).filter(|(_, ok)| !ok).map(|(h, _)| h.clone()));
    }
    missing
}

fn too_large() -> ApiError {
    ApiError::new(StatusCode::PAYLOAD_TOO_LARGE, "CHUNK_TOO_LARGE", "Parça 4 MB sınırını aşıyor.")
}

/// Ham gövdeyi geçici dosyaya akıtır, sha256'yı yolda hesaplar; özet eşleşirse parça atomik
/// olarak kalıcı yerine taşınır. Aynı parçayı iki istemci aynı anda yüklerse ikisi de başarılı
/// sayılır, diskte tek dosya kalır.
pub async fn write_chunk(mut body: Body, hash: &str) -> Result<u64, ApiError> {
    let max = CHUNK_SIZE as u64;
    let target = chunk_path(hash);
    tokio::fs::create_dir_all(target.parent().unwrap()).await.map_err(ApiError::internal)?;
    tokio::fs::create_dir_all(tmp_dir()).await.map_err(ApiError::internal)?;
    let part = tmp_dir().join(format!("{hash}.{}.part", crate::ctx::uuid()));
    let mut out = tokio::fs::OpenOptions::new().write(true).create_new(true).open(&part).await.map_err(ApiError::internal)?;
    let mut digest = Sha256::new();
    let mut size: u64 = 0;
    let result: Result<(), ApiError> = async {
        loop {
            let frame = std::future::poll_fn(|cx| Pin::new(&mut body).poll_frame(cx)).await;
            let Some(frame) = frame else { break };
            let frame = frame.map_err(|_| ApiError::bad_request("CHUNK_ABORTED", "Yükleme kesildi."))?;
            let Ok(data) = frame.into_data() else { continue };
            size += data.len() as u64;
            if size > max {
                return Err(too_large());
            }
            digest.update(&data);
            out.write_all(&data).await.map_err(ApiError::internal)?;
        }
        out.flush().await.map_err(ApiError::internal)?;
        Ok(())
    }
    .await;
    drop(out);
    if let Err(error) = result {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(error);
    }
    if size == 0 {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(ApiError::bad_request("CHUNK_EMPTY", "Boş parça gönderilemez."));
    }
    if hex::encode(digest.finalize()) != hash {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, "CHUNK_HASH_MISMATCH", "Parça içeriği beklenen özetle uyuşmuyor."));
    }
    if tokio::fs::rename(&part, &target).await.is_err() {
        let _ = tokio::fs::remove_file(&part).await;
        if !chunk_exists(hash).await {
            return Err(ApiError::internal("CHUNK_WRITE_FAILED"));
        }
    }
    Ok(size)
}

// ------------------------------------------------------------------ çöp toplama
// Referans sayısı ReleaseChunkRef tablosundan türetilir: hiçbir sürümün başvurmadığı parça
// satırı silinir, dosyası kaldırılır. Ayrıca diskte olup tabloda olmayan (çökme artığı)
// dosyalar süpürülür. Tek seferde tek çalışır.

static GC_RUNNING: AtomicBool = AtomicBool::new(false);
static GC_TIMER: AtomicU64 = AtomicU64::new(0);
const YOUNG_MS: i64 = 10 * 60 * 1000;

/// Ertelenmiş (2 sn) GC; art arda çağrılar tek çalışmaya iner.
pub fn schedule_gc(db: sqlx::PgPool) {
    let ticket = GC_TIMER.fetch_add(1, Ordering::SeqCst) + 1;
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(2000)).await;
        if GC_TIMER.load(Ordering::SeqCst) == ticket {
            gc_chunks(&db, false).await;
        }
    });
}

pub async fn gc_chunks(db: &sqlx::PgPool, deep: bool) -> u64 {
    if GC_RUNNING.swap(true, Ordering::SeqCst) {
        return 0;
    }
    let removed = gc_once(db, deep).await.unwrap_or(0);
    GC_RUNNING.store(false, Ordering::SeqCst);
    removed
}

async fn gc_once(db: &sqlx::PgPool, deep: bool) -> Result<u64, sqlx::Error> {
    let now = crate::ctx::now();
    // Bir hafta dokunulmamış taslaklar terk edilmiş sayılır.
    sqlx::query(r#"delete from "Release" where status = 'DRAFT' and "updatedAt" < $1"#)
        .bind(now - chrono::Duration::days(7))
        .execute(db)
        .await?;
    let hashes: Vec<String> = sqlx::query_scalar(
        r#"delete from "ReleaseChunk" c where not exists (select 1 from "ReleaseChunkRef" r where r."hash" = c."hash") and c."createdAt" < $1 returning "hash""#,
    )
    .bind(now - chrono::Duration::milliseconds(YOUNG_MS))
    .fetch_all(db)
    .await?;
    let mut removed = 0;
    for hash in hashes {
        if is_hash(&hash) && tokio::fs::remove_file(chunk_path(&hash)).await.is_ok() {
            removed += 1;
        }
    }
    if deep {
        removed += sweep_orphans(db).await;
    }
    Ok(removed)
}

async fn read_names(dir: &Path) -> Option<Vec<String>> {
    let mut entries = tokio::fs::read_dir(dir).await.ok()?;
    let mut names = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    Some(names)
}

async fn sweep_orphans(db: &sqlx::PgPool) -> u64 {
    let mut removed = 0;
    let base = root().join("chunks");
    let Some(level1) = read_names(&base).await else { return 0 };
    for a in level1 {
        let Some(level2) = read_names(&base.join(&a)).await else { continue };
        for b in level2 {
            let Some(names) = read_names(&base.join(&a).join(&b)).await else { continue };
            let names: Vec<String> = names.into_iter().filter(|n| is_hash(n)).collect();
            for batch in names.chunks(500) {
                let Ok(known) = sqlx::query_scalar::<_, String>(r#"select hash from "ReleaseChunk" where hash = any($1)"#).bind(batch).fetch_all(db).await else { continue };
                for hash in batch {
                    if known.contains(hash) {
                        continue;
                    }
                    let file = base.join(&a).join(&b).join(hash);
                    let Ok(info) = tokio::fs::metadata(&file).await else { continue };
                    let young = info.modified().ok().and_then(|m| m.elapsed().ok()).is_none_or(|age| (age.as_millis() as i64) < YOUNG_MS);
                    if young {
                        continue;
                    }
                    if tokio::fs::remove_file(&file).await.is_ok() {
                        removed += 1;
                    }
                }
            }
        }
    }
    removed
}
