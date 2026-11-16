//! Veritabanı testleri — test PostgreSQL'i (:55432, `C:/p2-pg/test-env.json` DATABASE_URL ya da
//! `P2_OPS_TEST_DB`) üzerinde geçici veritabanları açar ve sonunda siler. Kullanıcının gerçek verisine
//! dokunmaz. Test sunucusu yoksa testler not düşüp geçer.
//!
//!   cargo test --release --target-dir C:/p2-rust-target -p p2-server-ops -- --test-threads=1 --nocapture

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use p2_server_ops::backup::{self, CreateOptions, RestoreOptions};
use p2_server_ops::config::ConfigureInput;
use p2_server_ops::migrate;
use p2_server_ops::ops::{Ops, OpsOptions};
use p2_server_ops::service::NoService;
use sqlx::{Connection as _, PgConnection};

const PG_BIN: &str = "C:/p2-pg/pgsql/bin";

fn base_url() -> Option<String> {
    if let Ok(url) = std::env::var("P2_OPS_TEST_DB") {
        return Some(url);
    }
    let text = std::fs::read_to_string("C:/p2-pg/test-env.json").ok()?;
    let value: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    value["DATABASE_URL"].as_str().map(String::from)
}

fn url_for(base: &str, db: &str) -> String {
    let mut url = url::Url::parse(base).unwrap();
    url.set_path(&format!("/{db}"));
    url.to_string()
}

fn unique(prefix: &str) -> String {
    let mut b = [0u8; 4];
    getrandom::fill(&mut b).unwrap();
    format!("{prefix}_{}", hex::encode(b))
}

fn admin(base: &str, sql: &str) {
    let sql = sql.to_string();
    let base = base.to_string();
    p2_server_ops::block_on(async move {
        let mut c = PgConnection::connect(&base).await.unwrap();
        sqlx::raw_sql(sqlx::AssertSqlSafe(sql)).execute(&mut c).await.unwrap();
        c.close().await.unwrap();
    });
}

fn exec(url: &str, sql: &str) {
    admin(url, sql)
}

struct Db {
    base: String,
    name: String,
}
impl Db {
    fn new(base: &str, prefix: &str) -> Self {
        let name = unique(prefix);
        admin(base, &format!("CREATE DATABASE \"{name}\" ENCODING 'UTF8'"));
        Self { base: base.into(), name }
    }
    fn url(&self) -> String {
        url_for(&self.base, &self.name)
    }
}
impl Drop for Db {
    fn drop(&mut self) {
        admin(&self.base, &format!("DROP DATABASE IF EXISTS \"{}\" WITH (FORCE)", self.name));
    }
}

fn temp(prefix: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(unique(prefix));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Tablo → sıralı satır metinleri (`_p2_migrations` için yalnız ad: `appliedAt` mikro saniye taşır).
fn snapshot(url: &str) -> BTreeMap<String, Vec<String>> {
    let url = url.to_string();
    p2_server_ops::block_on(async move {
        let mut c = PgConnection::connect(&url).await.unwrap();
        let names: Vec<String> = sqlx::query_scalar("SELECT tablename::text FROM pg_tables WHERE schemaname='public' ORDER BY 1").fetch_all(&mut c).await.unwrap();
        let mut out = BTreeMap::new();
        for name in names {
            let select = if name == "_p2_migrations" { "SELECT \"name\" FROM \"_p2_migrations\"".to_string() } else { format!("SELECT * FROM \"{name}\"") };
            let rows: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT row_to_json(t)::text FROM ({select}) t ORDER BY 1"))).fetch_all(&mut c).await.unwrap();
            out.insert(name, rows);
        }
        c.close().await.unwrap();
        out
    })
}

const SEED: &str = r#"
INSERT INTO "User"(id,email,username,"displayName","passwordHash","createdAt","updatedAt",role) VALUES ('u1','a@x.test','a','Ahmet Şimşek','h','2026-01-02 03:04:05.678','2026-01-02 03:04:05.678','ADMIN');
INSERT INTO "Organization"(id,name,slug,"createdAt","updatedAt") VALUES ('o1','Org','org','2026-01-02 03:04:05.678','2026-01-02 03:04:05.678');
INSERT INTO "Role"(id,"organizationId",handle,name,permissions,"createdById","createdAt","updatedAt") VALUES ('r1','o1','h','R',ARRAY['a','b "q"','c,d'],'u1','2026-05-06 07:08:09.001','2026-05-06 07:08:09.001');
INSERT INTO p2ops_types VALUES
 (1,'2026-03-04 05:06:07.089','2026-03-04 05:06:07.089+00',9007199254740993,'{"a":[1,2,{"b":null}],"s":"ş"}','{"x":1}',ARRAY['x','y'],'\x00ff10','ADMIN',ARRAY['ADMIN','USER']::"UserRole"[],true,1.5,'2026-03-04',12.50),
 (2,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
"#;
const TYPES_TABLE: &str = r#"CREATE TABLE p2ops_types (id int primary key, ts timestamp(3), tz timestamptz(3), big bigint, js jsonb, j json, arr text[], bin bytea, role "UserRole", roles "UserRole"[], flag boolean, num double precision, d date, n numeric)"#;

/// Göç edilmiş + tip tablosu olan boş hedef (yükleme hedefi); TRUNCATE'i görmek için bir artık satır.
fn prepared(base: &str, prefix: &str) -> Db {
    let db = Db::new(base, prefix);
    migrate::apply_url_blocking(&db.url()).unwrap();
    exec(&db.url(), TYPES_TABLE);
    exec(&db.url(), r#"INSERT INTO "User"(id,email,username,"displayName","passwordHash","updatedAt") VALUES ('junk','j@x','j','J','h',now())"#);
    db
}

fn seeded(base: &str) -> Db {
    let db = Db::new(base, "p2ops_src");
    migrate::apply_url_blocking(&db.url()).unwrap();
    exec(&db.url(), TYPES_TABLE);
    exec(&db.url(), SEED);
    db
}

#[test]
fn migrations_fresh_idempotent_and_adoption() {
    let Some(base) = base_url() else {
        eprintln!("test PostgreSQL yok; atlandı");
        return;
    };
    // 1) Taze kurulum: hepsi uygulanır; ikinci çalıştırma hiçbir şey yapmaz.
    let fresh = Db::new(&base, "p2ops_mig");
    let report = migrate::apply_url_blocking(&fresh.url()).unwrap();
    assert_eq!(report.applied.len(), migrate::MIGRATIONS.len());
    assert!(report.adopted.is_empty());
    let again = migrate::apply_url_blocking(&fresh.url()).unwrap();
    assert!(again.applied.is_empty() && again.adopted.is_empty());
    let names: Vec<&str> = migrate::MIGRATIONS.iter().map(|(n, _)| *n).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);

    // 2) Prisma'dan devralma: ilk 10 göç Prisma ile uygulanmış (biri geri alınmış sayılır).
    let prisma = Db::new(&base, "p2ops_prisma");
    exec(
        &prisma.url(),
        r#"CREATE TABLE "_prisma_migrations" ("id" VARCHAR(36) PRIMARY KEY, "checksum" VARCHAR(64) NOT NULL, "finished_at" TIMESTAMPTZ, "migration_name" VARCHAR(255) NOT NULL,
           "logs" TEXT, "rolled_back_at" TIMESTAMPTZ, "started_at" TIMESTAMPTZ NOT NULL DEFAULT now(), "applied_steps_count" INTEGER NOT NULL DEFAULT 0)"#,
    );
    for (i, (name, sql)) in migrate::MIGRATIONS.iter().take(10).enumerate() {
        exec(&prisma.url(), sql);
        exec(&prisma.url(), &format!(r#"INSERT INTO "_prisma_migrations" (id, checksum, finished_at, migration_name, applied_steps_count) VALUES ('{i}', 'x', now(), '{name}', 1)"#));
    }
    // Yarım kalmış bir Prisma kaydı (finished_at yok) devralınmamalı: 11. göç gerçekten uygulanır.
    let eleventh = migrate::MIGRATIONS[10].0;
    exec(&prisma.url(), &format!(r#"INSERT INTO "_prisma_migrations" (id, checksum, finished_at, migration_name) VALUES ('x11', 'x', NULL, '{eleventh}')"#));
    let report = migrate::apply_url_blocking(&prisma.url()).unwrap();
    assert_eq!(report.adopted, names[..10].iter().map(|s| s.to_string()).collect::<Vec<_>>());
    assert_eq!(report.applied, names[10..].iter().map(|s| s.to_string()).collect::<Vec<_>>());
    assert_eq!(snapshot(&prisma.url()).keys().filter(|k| !k.starts_with('_')).count(), snapshot(&fresh.url()).keys().filter(|k| !k.starts_with('_')).count());

    // 3) Eski kurulum (`"User"` var, kayıt tablosu yok): `_init` yalnız kaydedilir; hatalı göç geri alınır.
    let legacy = Db::new(&base, "p2ops_legacy");
    exec(&legacy.url(), r#"CREATE TABLE "User" (id text primary key)"#);
    let fake: &[(&str, &str)] = &[
        ("20200101000000_init", "CREATE TABLE \"ShouldNotExist\" (id int)"),
        ("20200102000000_ok", "CREATE TABLE p2ops_ok (id int); INSERT INTO p2ops_ok VALUES (1)"),
        ("20200103000000_bad", "CREATE TABLE p2ops_half (id int); SELECT * FROM yok_boyle_tablo"),
    ];
    let error = p2_server_ops::block_on(async {
        let mut c = PgConnection::connect(&legacy.url()).await.unwrap();
        let r = migrate::apply_list(&mut c, fake).await;
        c.close().await.unwrap();
        r
    })
    .unwrap_err();
    assert!(p2_server_ops::error_message(&error).contains("yok_boyle_tablo"), "{error:?}");
    let snap = snapshot(&legacy.url());
    assert!(!snap.contains_key("ShouldNotExist"));
    assert!(!snap.contains_key("p2ops_half"), "hatalı göç geri alınmalı");
    assert_eq!(snap["p2ops_ok"].len(), 1);
    assert_eq!(snap["_p2_migrations"], vec![r#"{"name":"20200101000000_init"}"#.to_string(), r#"{"name":"20200102000000_ok"}"#.to_string()]);
}

/// Eski (kuruluş düzeyi) modelde veri, `project_communities` göçünden önce.
const COMMUNITY_SEED: &str = r#"
INSERT INTO "User"(id,email,username,"displayName","passwordHash","updatedAt") VALUES
 ('u1','u1@x','u1','U1','h',now()), ('u2','u2@x','u2','U2','h',now()), ('u3','u3@x','u3','U3','h',now()),
 ('u4','u4@x','u4','U4','h',now()), ('u5','u5@x','u5','U5','h',now());
INSERT INTO "Organization"(id,name,slug,"createdAt","updatedAt") VALUES ('o1','Org','org',now(),now());
INSERT INTO "OrganizationMember"("organizationId","userId",role,permissions,"deniedPermissions") VALUES
 ('o1','u1','OWNER','{}','{}'), ('o1','u2','MEMBER',ARRAY['member.invite'],ARRAY['message.attach']), ('o1','u3','MEMBER','{}','{}'), ('o1','u5','MEMBER','{}','{}');
INSERT INTO "Project"(id,name,slug,"ownerId","organizationId","updatedAt") VALUES ('p1','P1','p1','u2','o1',now()), ('p2','P2','p2','u5','o1',now());
-- u4 kuruluştan çıkarılmış ama ProjectMember satırı kalmış (eskiden etkisiz); u5 p2'nin sahibi ama satırı yok.
INSERT INTO "ProjectMember"("projectId","userId",role) VALUES ('p1','u2','MEMBER'), ('p1','u4','MEMBER');
INSERT INTO "Role"(id,"organizationId",handle,name,everyone,founder,permissions,"createdById","updatedAt") VALUES
 ('e1','o1','everyone','Herkes',true,false,ARRAY['channel.view'],'u1',now()),
 ('f1','o1','kurucu','Kurucu',false,true,ARRAY['admin.panel'],'u1',now()),
 ('r1','o1','mod','Mod',false,false,ARRAY['member.kick'],'u1',now());
INSERT INTO "RoleMember"("roleId","userId") VALUES ('f1','u1'), ('r1','u2'), ('r1','u3');
INSERT INTO "UserGroup"(id,"organizationId",handle,name,"createdById","updatedAt") VALUES ('g1','o1','devs','Devs','u1',now());
INSERT INTO "UserGroupMember"("groupId","userId") VALUES ('g1','u2'), ('g1','u3');
INSERT INTO "RoleGroup"("roleId","groupId") VALUES ('r1','g1');
INSERT INTO "ChannelCategory"(id,name,position,"projectId") VALUES ('c1','GENEL',0,'p1');
INSERT INTO "ProjectChannel"(id,name,type,position,"categoryId") VALUES ('ch1','genel','TEXT',0,'c1');
INSERT INTO "ChannelPermission"(id,"channelId","targetType","targetId",allow,deny,"updatedAt") VALUES
 ('cp1','ch1','ROLE','r1',ARRAY['message.delete_any'],'{}',now()), ('cp2','ch1','GROUP','g1','{}',ARRAY['message.send'],now()), ('cp3','ch1','USER','u3',ARRAY['channel.view'],'{}',now());
INSERT INTO "OrganizationInvite"(id,code,"organizationId","createdById") VALUES ('i1','P2-OLDCODE1','o1','u1');
"#;

fn rows(url: &str, sql: &str) -> Vec<String> {
    let (url, sql) = (url.to_string(), sql.to_string());
    p2_server_ops::block_on(async move {
        let mut c = PgConnection::connect(&url).await.unwrap();
        let out: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(sql)).fetch_all(&mut c).await.unwrap();
        c.close().await.unwrap();
        out
    })
}

/// Proje bazlı yapı göçü (docs/proje-bazli-yapi.md): eski kuralla erişimi olan herkes üye olur, roller /
/// gruplar / kanal kuralları her projeye kopyalanır, eski kuruluş davetleri kapanır; tekrar çalışmak bir şey değiştirmez.
#[test]
fn project_communities_migration() {
    let Some(base) = base_url() else {
        eprintln!("test PostgreSQL yok; atlandı");
        return;
    };
    let db = Db::new(&base, "p2ops_comm");
    let (last, before) = migrate::MIGRATIONS.split_last().unwrap();
    assert_eq!(last.0, "20261116100000_project_communities");
    let url = db.url();
    p2_server_ops::block_on(async {
        let mut c = PgConnection::connect(&url).await.unwrap();
        migrate::apply_list(&mut c, before).await.unwrap();
        c.close().await.unwrap();
    });
    exec(&url, COMMUNITY_SEED);
    let report = migrate::apply_url_blocking(&url).unwrap();
    assert_eq!(report.applied, vec![last.0.to_string()]);

    // Üyelik: eski görünürlük kuralı (kuruluş sahibi her şeyi görür; proje sahibi; ProjectMember ∩ kuruluş üyesi).
    let members = rows(&url, r#"SELECT "projectId" || ':' || "userId" || ':' || role FROM "ProjectMember" ORDER BY 1"#);
    assert_eq!(members, vec!["p1:u1:OWNER", "p1:u2:OWNER", "p2:u1:OWNER", "p2:u5:OWNER"]);
    assert_eq!(rows(&url, r#"SELECT array_to_string(permissions, ',') || '|' || array_to_string("deniedPermissions", ',') FROM "ProjectMember" WHERE "userId" = 'u2'"#), vec!["member.invite|message.attach"]);

    // Rol ve grup kopyaları: her projede yeni kimlik, üyeler yalnız proje üyeleri.
    assert_eq!(rows(&url, r#"SELECT "projectId" || ':' || handle FROM "Role" WHERE "projectId" IS NOT NULL ORDER BY 1"#), vec!["p1:everyone", "p1:kurucu", "p1:mod", "p2:everyone", "p2:kurucu", "p2:mod"]);
    assert_eq!(rows(&url, r#"SELECT count(*)::text FROM "Role" WHERE "projectId" IS NULL"#), vec!["3"], "arşiv rolleri kalır");
    assert_eq!(rows(&url, r#"SELECT count(*)::text FROM "Role" WHERE "projectId" IS NOT NULL AND id IN ('e1','f1','r1')"#), vec!["0"]);
    assert_eq!(
        rows(&url, r#"SELECT r."projectId" || ':' || r.handle || ':' || m."userId" FROM "RoleMember" m JOIN "Role" r ON r.id = m."roleId" WHERE r."projectId" IS NOT NULL ORDER BY 1"#),
        vec!["p1:kurucu:u1", "p1:kurucu:u2", "p1:mod:u2", "p2:kurucu:u1", "p2:kurucu:u5"]
    );
    assert_eq!(
        rows(&url, r#"SELECT g."projectId" || ':' || m."userId" FROM "UserGroupMember" m JOIN "UserGroup" g ON g.id = m."groupId" WHERE g."projectId" IS NOT NULL ORDER BY 1"#),
        vec!["p1:u2"]
    );
    assert_eq!(
        rows(&url, r#"SELECT r."projectId" || ':' || g."projectId" FROM "RoleGroup" x JOIN "Role" r ON r.id = x."roleId" JOIN "UserGroup" g ON g.id = x."groupId" WHERE r."projectId" IS NOT NULL ORDER BY 1"#),
        vec!["p1:p1", "p2:p2"]
    );

    // Kanal kuralları kopya kimliklere döner; kişi kuralı aynen kalır.
    let targets = rows(
        &url,
        r#"SELECT cp."targetType" || ':' || coalesce(r."projectId" || '/' || r."sourceRoleId", g."projectId" || '/' || g."sourceGroupId", cp."targetId")
           FROM "ChannelPermission" cp LEFT JOIN "Role" r ON r.id = cp."targetId" AND cp."targetType" = 'ROLE' LEFT JOIN "UserGroup" g ON g.id = cp."targetId" AND cp."targetType" = 'GROUP' ORDER BY 1"#,
    );
    assert_eq!(targets, vec!["GROUP:p1/g1", "ROLE:p1/r1", "USER:u3"]);

    // Davetler: eski kod kapalı, her projenin kalıcı kodu var.
    assert_eq!(rows(&url, r#"SELECT ("revokedAt" IS NOT NULL)::text FROM "OrganizationInvite""#), vec!["true"]);
    assert_eq!(rows(&url, r#"SELECT "projectId" FROM "ProjectInvite" WHERE NOT revoked ORDER BY 1"#), vec!["p1", "p2"]);

    // Tekrar çalışmak hiçbir şeyi değiştirmez.
    let first = snapshot(&url);
    exec(&url, &format!("BEGIN; {} COMMIT;", last.1));
    assert_eq!(snapshot(&url), first);
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

fn set_pg_bin(on: bool) {
    // Testler tek iş parçacığında (--test-threads=1) çalışır; bu dosyada başka eşzamanlı okuyucu yok.
    unsafe {
        if on {
            std::env::set_var("P2_PG_BIN", PG_BIN);
        } else {
            std::env::remove_var("P2_PG_BIN");
        }
    }
}

fn check_restored_folder(data: &Path) {
    assert_eq!(read(&data.join("uploads/a.txt")), "merhaba");
    assert_eq!(read(&data.join("uploads/alt/klasör/b.bin")), "ikinci");
    assert_eq!(read(&data.join("releases/r.txt")), "sürüm");
    let before: Vec<String> = std::fs::read_dir(data).unwrap().filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.starts_with("uploads-before-restore-")).collect();
    assert_eq!(before.len(), 1, "eski uploads kenara alınmalı");
    assert_eq!(read(&data.join(&before[0]).join("eski.txt")), "eski");
}

fn target_data() -> PathBuf {
    let data = temp("p2ops_data_dst");
    write(&data.join("uploads/eski.txt"), "eski");
    data
}

#[test]
fn backup_roundtrips() {
    let Some(base) = base_url() else {
        eprintln!("test PostgreSQL yok; atlandı");
        return;
    };
    for (pg_bin, expected) in [(false, "p2-postgres-json-v1"), (true, "postgres-custom")] {
        set_pg_bin(pg_bin);
        eprintln!("--- biçim: {expected}");
        let source = seeded(&base);
        let src_snap = snapshot(&source.url());
        let data = temp("p2ops_data_src");
        write(&data.join("uploads/a.txt"), "merhaba");
        write(&data.join("uploads/alt/klasör/b.bin"), "ikinci");
        write(&data.join("releases/r.txt"), "sürüm");
        let work = temp("p2ops_files");

        // Rust → Rust
        let archive = work.join("rust.p2backup");
        let created = backup::create_backup(CreateOptions { database_url: &source.url(), data_dir: &data, destination: &archive, version: "test", include_releases: true }).unwrap();
        assert_eq!(created.manifest["databaseFormat"], expected);
        assert_eq!(created.manifest["includes"], serde_json::json!({ "database": true, "uploads": true, "releases": true }));
        let entries = backup::validate_archive(&archive).unwrap();
        assert!(entries.iter().any(|e| e == "uploads/alt/klasör/b.bin"), "{entries:?}");
        let target = prepared(&base, "p2ops_rr");
        let dst = target_data();
        let stops = std::cell::Cell::new(0);
        let starts = std::cell::Cell::new(0);
        let stop = || {
            stops.set(stops.get() + 1);
            p2_server_ops::Outcome::ok()
        };
        let start = || {
            starts.set(starts.get() + 1);
            p2_server_ops::Outcome::ok()
        };
        let manifest = backup::restore_backup(RestoreOptions { database_url: &target.url(), data_dir: &dst, source: &archive, stop: Some(&stop), start: Some(&start) }).unwrap();
        assert_eq!(manifest["product"], "P2 Server");
        assert_eq!((stops.get(), starts.get()), (1, 1));
        assert_eq!(snapshot(&target.url()), src_snap, "Rust→Rust ({expected})");
        check_restored_folder(&dst);
        eprintln!("Rust→Rust: tamam");

        // Güvensiz arşivler reddedilir.
        let bad = work.join("bad.p2backup");
        {
            let f = std::fs::File::create(&bad).unwrap();
            let mut b = tar::Builder::new(flate2::write::GzEncoder::new(f, flate2::Compression::default()));
            for (name, body) in [("manifest.json", "{}"), ("database.dump", "x"), ("evil.txt", "x")] {
                let mut h = tar::Header::new_gnu();
                h.set_size(body.len() as u64);
                h.set_mode(0o644);
                h.set_cksum();
                b.append_data(&mut h, name, body.as_bytes()).unwrap();
            }
            b.into_inner().unwrap().finish().unwrap();
        }
        assert_eq!(backup::validate_archive(&bad).unwrap_err().to_string(), "Yedekte beklenmeyen içerik var: evil.txt");
        let _ = std::fs::remove_dir_all(&work);
        let _ = std::fs::remove_dir_all(&data);
    }
    set_pg_bin(false);
}

/// Konsolun yaptığı akış (arayüzsüz): yapılandır (CREATE DATABASE + göç) → API başlar → durum →
/// durdur → başlat → yeni bir konsol örneği imzalı kapatma isteğiyle durdurur.
#[test]
fn ops_configure_start_stop() {
    let Some(base) = base_url() else {
        eprintln!("test PostgreSQL yok; atlandı");
        return;
    };
    let built = PathBuf::from("C:/p2-rust-target/release/p2-server.exe");
    if !built.exists() {
        eprintln!("p2-server.exe derlenmemiş; atlandı");
        return;
    }
    let exe = PathBuf::from("C:/p2-pg/srv-ops-4141.exe");
    std::fs::copy(&built, &exe).unwrap();
    unsafe { std::env::set_var("HOST", "127.0.0.1") };
    let data = temp("p2ops_life");
    let database = unique("p2ops_life");
    let options = || OpsOptions { data_dir: data.clone(), headless: false, service: Box::new(NoService), server_exe: exe.clone(), version: "9.9.9-test".into() };
    let ops = Ops::new(options());
    let state = ops.state();
    assert!(!state.configured && !state.running && !state.service);
    assert_eq!(state.address, "http://public-ip-bekleniyor:4000");
    assert_eq!(ops.start().message.as_deref(), Some("Önce PostgreSQL bağlantısını yapılandırın."));

    let url = url::Url::parse(&base).unwrap();
    let mut input = ConfigureInput {
        host: url.host_str().unwrap().into(),
        db_port: url.port().unwrap() as f64,
        api_port: 4141.,
        username: url.username().into(),
        password: "".into(),
        database: database.clone(),
        ssl: false,
    };
    assert_eq!(ops.configure(&input).message.as_deref(), Some("Veritabanı veya API portu ayarları geçersiz."));
    input.password = "parola".into(); // test sunucusu trust kimlik doğrulaması kullanıyor
    let outcome = ops.configure(&input);
    assert!(outcome.ok, "{outcome:?} / {}", ops.last_error());
    let config = ops.read_config().unwrap();
    assert!(config.database_url.as_deref().unwrap().ends_with(&format!("/{database}?schema=public")));
    assert_eq!(config.access_secret.as_ref().unwrap().len(), 96);
    let state = ops.state();
    assert!(state.configured && state.running && state.managed, "{state:?}");
    assert_eq!(state.port, 4141);
    assert!(state.log.contains("Yapılandırma kaydedildi; API portu 4141"));
    assert!(state.log.contains("P2 API başlatılıyor: 0.0.0.0:4141"));

    // Aynı gizli anahtarlar korunur (yeniden yapılandırma).
    let secret = config.access_secret.clone();
    assert!(ops.configure(&input).ok);
    assert_eq!(ops.read_config().unwrap().access_secret, secret);

    let stopped = ops.stop(false);
    assert!(stopped.ok, "{stopped:?}");
    assert!(!ops.api_alive());
    assert!(ops.start().ok);
    for _ in 0..50 {
        if ops.api_alive() {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(ops.api_alive());
    // Başka bir konsol (süreç tutmuyor): /updates/stop + HMAC.
    let other = Ops::new(options());
    assert!(!other.managed());
    assert_eq!(other.start().already_running, true);
    let stopped = other.stop(false);
    assert!(stopped.ok, "{stopped:?}");
    std::thread::sleep(Duration::from_millis(600));
    assert!(!ops.managed(), "izleyici süreç çıkışını görmeli");
    assert!(ops.state().log.contains("P2 API kapandı (kod 0)"), "beklenmeyen kapanış günlüğe düşmeli");

    // Yedek al / yükle (Ops üzerinden; yükleme API'yi durdurup yeniden başlatır).
    let file = data.join("yedek.p2backup");
    let made = ops.create_backup(&file, false);
    assert!(made.ok, "{made:?}");
    assert!(ops.start().ok);
    let restored = ops.restore_backup(&file);
    assert!(restored.ok, "{restored:?}");
    std::thread::sleep(Duration::from_millis(1500));
    assert!(ops.api_alive(), "yükleme sonrası API yeniden başlamalı");
    assert!(ops.state().log.contains("Yedek başarıyla yüklendi ("));
    assert!(ops.stop(false).ok);

    admin(&base, &format!("DROP DATABASE IF EXISTS \"{database}\" WITH (FORCE)"));
    let _ = std::fs::remove_dir_all(&data);
}
