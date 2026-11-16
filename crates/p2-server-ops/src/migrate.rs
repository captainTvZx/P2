//! `applyMigrations` (server-main.cjs) / `deploy/linux/migrate.cjs`: birebir aynı algoritma.
//!
//! 1. `_p2_migrations` yoksa oluşturulur.
//! 2. Her göç (ad sırasıyla) zaten kayıtlıysa atlanır.
//! 3. `_prisma_migrations` varsa ve göç orada bitmiş/geri alınmamışsa yalnız kaydedilir (Prisma'dan devralma).
//! 4. `*_init`: `"User"` tablosu zaten varsa (eski kurulum) yalnız kaydedilir; yoksa artık enum tipleri düşürülür.
//! 5. Göç SQL'i ve kaydı tek işlemde (BEGIN/COMMIT) uygulanır; hata olursa ROLLBACK ve hata yukarı.

use sqlx::{Connection as _, PgConnection};

include!(concat!(env!("OUT_DIR"), "/migrations.rs"));

const CREATE_TABLE: &str = r#"CREATE TABLE IF NOT EXISTS "_p2_migrations" ("name" TEXT PRIMARY KEY, "appliedAt" TIMESTAMPTZ NOT NULL DEFAULT NOW())"#;
const DROP_LEGACY_TYPES: &str = r#"DROP TYPE IF EXISTS "UserRole" CASCADE; DROP TYPE IF EXISTS "ProjectRole" CASCADE; DROP TYPE IF EXISTS "FriendshipStatus" CASCADE; DROP TYPE IF EXISTS "UserStatus" CASCADE;"#;

fn raw(sql: &str) -> sqlx::RawSql {
    sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
}

/// Ne yapıldığı (testler ve CLI çıktısı için).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Report {
    /// SQL'i çalıştırılan göçler.
    pub applied: Vec<String>,
    /// `_prisma_migrations`'tan ya da eski kurulumdan devralınıp yalnız kaydedilenler.
    pub adopted: Vec<String>,
}

async fn record(conn: &mut PgConnection, name: &str) -> sqlx::Result<()> {
    sqlx::query(r#"INSERT INTO "_p2_migrations" ("name") VALUES ($1)"#).bind(name).execute(&mut *conn).await?;
    Ok(())
}

/// Gömülü göçleri uygular.
pub async fn apply(conn: &mut PgConnection) -> anyhow::Result<Report> {
    apply_list(conn, MIGRATIONS).await
}

/// Verilen göç listesini uygular (testler sahte liste verebilir).
pub async fn apply_list(conn: &mut PgConnection, migrations: &[(&str, &str)]) -> anyhow::Result<Report> {
    let mut report = Report::default();
    raw(CREATE_TABLE).execute(&mut *conn).await?;
    for (name, sql) in migrations {
        let done: Option<i32> = sqlx::query_scalar(r#"SELECT 1 FROM "_p2_migrations" WHERE "name" = $1"#).bind(name).fetch_optional(&mut *conn).await?;
        if done.is_some() {
            continue;
        }
        let prisma: Option<String> = sqlx::query_scalar(r#"SELECT to_regclass('public."_prisma_migrations"')::text AS table_name"#).fetch_one(&mut *conn).await?;
        if prisma.is_some() {
            let finished: Option<i32> = sqlx::query_scalar(
                r#"SELECT 1 FROM "_prisma_migrations" WHERE "migration_name" = $1 AND "finished_at" IS NOT NULL AND "rolled_back_at" IS NULL"#,
            )
            .bind(name)
            .fetch_optional(&mut *conn)
            .await?;
            if finished.is_some() {
                record(conn, name).await?;
                report.adopted.push(name.to_string());
                continue;
            }
        }
        if name.ends_with("_init") {
            let legacy: Option<String> = sqlx::query_scalar(r#"SELECT to_regclass('public."User"')::text AS table_name"#).fetch_one(&mut *conn).await?;
            if legacy.is_some() {
                record(conn, name).await?;
                report.adopted.push(name.to_string());
                continue;
            }
            raw(DROP_LEGACY_TYPES).execute(&mut *conn).await?;
        }
        raw("BEGIN").execute(&mut *conn).await?;
        let result: anyhow::Result<()> = async {
            raw(sql).execute(&mut *conn).await?;
            record(conn, name).await?;
            raw("COMMIT").execute(&mut *conn).await?;
            Ok(())
        }
        .await;
        if let Err(error) = result {
            let _ = raw("ROLLBACK").execute(&mut *conn).await;
            return Err(error.context(format!("göç uygulanamadı: {name}")));
        }
        report.applied.push(name.to_string());
    }
    Ok(report)
}

/// Bağlantı adresiyle (Prisma'nın `schema` parametresi atılarak) bağlanıp uygular.
pub async fn apply_url(database_url: &str) -> anyhow::Result<Report> {
    let url = crate::config::clean_database_url(database_url)?;
    let mut conn = PgConnection::connect(&url).await?;
    let result = apply(&mut conn).await;
    let _ = conn.close().await;
    result
}

/// Bloklayan sürüm (CLI, konsol, hizmet açılışı).
pub fn apply_url_blocking(database_url: &str) -> anyhow::Result<Report> {
    crate::block_on(apply_url(database_url))
}

#[cfg(test)]
mod tests {
    use sha2::{Digest as _, Sha256};

    /// Gömülü göç listesinin parmak izi: her satır `<ad> <sha256(migration.sql)>\n`, ad sırasıyla;
    /// izin kendisi bu metnin SHA-256'sı. Üretimde uygulanmış göçlerin adı ve içeriği değişmemeli
    /// (taşıma öncesi `server/prisma/migrations` ile aynı ilk 49 göç + Rust dönemi göçleri). Yeni göç eklenince beklenen
    /// değer bilinçli olarak güncellenir; `--nocapture` listeyi yazar.
    #[test]
    fn embedded_migrations_fingerprint() {
        let mut listing = String::new();
        for (name, sql) in super::MIGRATIONS {
            listing.push_str(&format!("{name} {}\n", hex::encode(Sha256::digest(sql.as_bytes()))));
        }
        eprint!("{listing}");
        assert_eq!(super::MIGRATIONS.len(), 50);
        assert_eq!(super::MIGRATIONS.first().map(|m| m.0), Some("20260831004247_init"));
        assert_eq!(super::MIGRATIONS.last().map(|m| m.0), Some("20261116100000_project_communities"));
        assert_eq!(hex::encode(Sha256::digest(listing.as_bytes())), "eeb71c12af0569c94164a50e4e46af1f22a277d381b2813ace966541fb8dd081");
    }
}
