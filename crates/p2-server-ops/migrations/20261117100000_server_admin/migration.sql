-- Sunucu (kurulum) yönetimi: sunucu çapında yetkiler, sunucu yasağı ve kurulum ayarları.
-- Yalnız ekler; tekrar çalıştırılsa da aynı sonucu verir.

-- Kişiye verilen sunucu yetkileri (project.create, server.*). Kurulum sahibi (kuruluş OWNER'ı ya da
-- ilk yönetici) bunların hepsine kendiliğinden sahiptir; satır gerekmez.
CREATE TABLE IF NOT EXISTS "ServerPermission" (
    "userId" TEXT NOT NULL,
    "permission" TEXT NOT NULL,
    "grantedById" TEXT,
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT "ServerPermission_pkey" PRIMARY KEY ("userId", "permission"),
    CONSTRAINT "ServerPermission_userId_fkey" FOREIGN KEY ("userId") REFERENCES "User"("id") ON DELETE CASCADE ON UPDATE CASCADE,
    CONSTRAINT "ServerPermission_grantedById_fkey" FOREIGN KEY ("grantedById") REFERENCES "User"("id") ON DELETE SET NULL ON UPDATE CASCADE
);

-- Sunucu yasağı: yasaklı kişi giriş yapamaz, oturumları kapatılır, gerekçe saklanır.
CREATE TABLE IF NOT EXISTS "ServerBan" (
    "userId" TEXT NOT NULL,
    "bannedById" TEXT,
    "reason" TEXT,
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT "ServerBan_pkey" PRIMARY KEY ("userId"),
    CONSTRAINT "ServerBan_userId_fkey" FOREIGN KEY ("userId") REFERENCES "User"("id") ON DELETE CASCADE ON UPDATE CASCADE,
    CONSTRAINT "ServerBan_bannedById_fkey" FOREIGN KEY ("bannedById") REFERENCES "User"("id") ON DELETE SET NULL ON UPDATE CASCADE
);

-- Kurulum ayarları (anahtar → değer): registrationPolicy ('open' | 'invite'),
-- projectCreation ('permission' | 'everyone').
CREATE TABLE IF NOT EXISTS "InstanceSetting" (
    "key" TEXT NOT NULL,
    "value" TEXT NOT NULL,
    "updatedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT "InstanceSetting_pkey" PRIMARY KEY ("key")
);

INSERT INTO "InstanceSetting" ("key", "value") VALUES ('registrationPolicy', 'open') ON CONFLICT DO NOTHING;
-- Proje açmak artık sunucu yetkisidir: varsayılan olarak yalnız yetkisi olanlar (ve kurulum sahibi) açar.
INSERT INTO "InstanceSetting" ("key", "value") VALUES ('projectCreation', 'permission') ON CONFLICT DO NOTHING;
