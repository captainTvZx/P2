-- Her proje kendi topluluğu: üyeler, roller, kullanıcı grupları, kanal izinleri, davetler ve
-- yasaklar projeye özeldir. Kuruluş yalnız hesap kabı olarak kalır (docs/proje-bazli-yapi.md).
-- Göç yalnız ekler; her adım tekrar çalıştırılsa da aynı sonucu verir.

-- ------------------------------------------------------------------ şema

ALTER TABLE "Role" ADD COLUMN IF NOT EXISTS "projectId" TEXT;
ALTER TABLE "Role" ADD COLUMN IF NOT EXISTS "sourceRoleId" TEXT;
ALTER TABLE "UserGroup" ADD COLUMN IF NOT EXISTS "projectId" TEXT;
ALTER TABLE "UserGroup" ADD COLUMN IF NOT EXISTS "sourceGroupId" TEXT;

DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'Role_projectId_fkey') THEN
    ALTER TABLE "Role" ADD CONSTRAINT "Role_projectId_fkey" FOREIGN KEY ("projectId") REFERENCES "Project"("id") ON DELETE CASCADE ON UPDATE CASCADE;
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'UserGroup_projectId_fkey') THEN
    ALTER TABLE "UserGroup" ADD CONSTRAINT "UserGroup_projectId_fkey" FOREIGN KEY ("projectId") REFERENCES "Project"("id") ON DELETE CASCADE ON UPDATE CASCADE;
  END IF;
END $$;

-- Kimlik (handle) artık proje içinde benzersiz; kuruluş düzeyindeki benzersizlik yalnız arşiv satırlarına.
DROP INDEX IF EXISTS "Role_organizationId_handle_key";
CREATE UNIQUE INDEX IF NOT EXISTS "Role_organizationId_handle_key" ON "Role"("organizationId", "handle") WHERE "projectId" IS NULL;
CREATE UNIQUE INDEX IF NOT EXISTS "Role_projectId_handle_key" ON "Role"("projectId", "handle") WHERE "projectId" IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS "Role_projectId_sourceRoleId_key" ON "Role"("projectId", "sourceRoleId");
CREATE INDEX IF NOT EXISTS "Role_projectId_idx" ON "Role"("projectId");

DROP INDEX IF EXISTS "UserGroup_organizationId_handle_key";
CREATE UNIQUE INDEX IF NOT EXISTS "UserGroup_organizationId_handle_key" ON "UserGroup"("organizationId", "handle") WHERE "projectId" IS NULL;
CREATE UNIQUE INDEX IF NOT EXISTS "UserGroup_projectId_handle_key" ON "UserGroup"("projectId", "handle") WHERE "projectId" IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS "UserGroup_projectId_sourceGroupId_key" ON "UserGroup"("projectId", "sourceGroupId");
CREATE INDEX IF NOT EXISTS "UserGroup_projectId_idx" ON "UserGroup"("projectId");

ALTER TABLE "ProjectMember" ADD COLUMN IF NOT EXISTS "permissions" TEXT[] NOT NULL DEFAULT '{}';
ALTER TABLE "ProjectMember" ADD COLUMN IF NOT EXISTS "deniedPermissions" TEXT[] NOT NULL DEFAULT '{}';
ALTER TABLE "ProjectMember" ADD COLUMN IF NOT EXISTS "joinedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP;
ALTER TABLE "ProjectMember" ADD COLUMN IF NOT EXISTS "inviteId" TEXT;
CREATE INDEX IF NOT EXISTS "ProjectMember_userId_idx" ON "ProjectMember"("userId");

CREATE TABLE IF NOT EXISTS "ProjectInvite" (
  "id" TEXT NOT NULL,
  "code" TEXT NOT NULL,
  "projectId" TEXT NOT NULL,
  "createdById" TEXT NOT NULL,
  "expiresAt" TIMESTAMP(3),
  "maxUses" INTEGER,
  "useCount" INTEGER NOT NULL DEFAULT 0,
  "revoked" BOOLEAN NOT NULL DEFAULT false,
  "revokedAt" TIMESTAMP(3),
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "ProjectInvite_pkey" PRIMARY KEY ("id"),
  CONSTRAINT "ProjectInvite_projectId_fkey" FOREIGN KEY ("projectId") REFERENCES "Project"("id") ON DELETE CASCADE ON UPDATE CASCADE,
  CONSTRAINT "ProjectInvite_createdById_fkey" FOREIGN KEY ("createdById") REFERENCES "User"("id") ON DELETE CASCADE ON UPDATE CASCADE
);
CREATE UNIQUE INDEX IF NOT EXISTS "ProjectInvite_code_key" ON "ProjectInvite"("code");
CREATE INDEX IF NOT EXISTS "ProjectInvite_projectId_idx" ON "ProjectInvite"("projectId");

CREATE TABLE IF NOT EXISTS "ProjectBan" (
  "projectId" TEXT NOT NULL,
  "userId" TEXT NOT NULL,
  "bannedById" TEXT,
  "reason" TEXT,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "ProjectBan_pkey" PRIMARY KEY ("projectId", "userId"),
  CONSTRAINT "ProjectBan_projectId_fkey" FOREIGN KEY ("projectId") REFERENCES "Project"("id") ON DELETE CASCADE ON UPDATE CASCADE,
  CONSTRAINT "ProjectBan_userId_fkey" FOREIGN KEY ("userId") REFERENCES "User"("id") ON DELETE CASCADE ON UPDATE CASCADE,
  CONSTRAINT "ProjectBan_bannedById_fkey" FOREIGN KEY ("bannedById") REFERENCES "User"("id") ON DELETE SET NULL ON UPDATE CASCADE
);
CREATE INDEX IF NOT EXISTS "ProjectBan_userId_idx" ON "ProjectBan"("userId");

ALTER TABLE "OrganizationInvite" ADD COLUMN IF NOT EXISTS "revokedAt" TIMESTAMP(3);

-- ------------------------------------------------------------------ üyelik (eski görünürlük kuralı)

-- Eski kural: kişi önce kuruluş üyesi olmalıydı (membership), sonra kuruluş sahibi ya da proje sahibi
-- ya da ProjectMember. Kuruluştan çıkarılmış kişilerin kalan satırları eskiden etkisizdi; yeni kuralda
-- satır tek başına erişim verdiği için silinir.
DELETE FROM "ProjectMember" pm
USING "Project" p
WHERE p."id" = pm."projectId" AND p."organizationId" IS NOT NULL
  AND NOT EXISTS (SELECT 1 FROM "OrganizationMember" om WHERE om."organizationId" = p."organizationId" AND om."userId" = pm."userId");

-- Proje sahibi (kuruluştaysa) sahip üye olur.
INSERT INTO "ProjectMember" ("projectId", "userId", "role")
SELECT p."id", p."ownerId", 'OWNER'::"ProjectRole"
FROM "Project" p
JOIN "OrganizationMember" om ON om."organizationId" = p."organizationId" AND om."userId" = p."ownerId"
ON CONFLICT ("projectId", "userId") DO UPDATE SET "role" = 'OWNER'::"ProjectRole";

-- Kuruluş sahibi eskiden her projeyi tam yetkiyle görüyordu: her projeye sahip üye olur.
INSERT INTO "ProjectMember" ("projectId", "userId", "role")
SELECT p."id", om."userId", 'OWNER'::"ProjectRole"
FROM "Project" p
JOIN "OrganizationMember" om ON om."organizationId" = p."organizationId" AND om."role" = 'OWNER'
ON CONFLICT ("projectId", "userId") DO UPDATE SET "role" = 'OWNER'::"ProjectRole";

-- Kişiye özel aç/kapat yetkileri her projedeki üyeliğe kopyalanır (yalnız henüz boşsa).
UPDATE "ProjectMember" pm
SET "permissions" = om."permissions", "deniedPermissions" = om."deniedPermissions"
FROM "Project" p, "OrganizationMember" om
WHERE p."id" = pm."projectId" AND om."organizationId" = p."organizationId" AND om."userId" = pm."userId"
  AND pm."permissions" = '{}' AND pm."deniedPermissions" = '{}'
  AND (om."permissions" <> '{}' OR om."deniedPermissions" <> '{}');

-- Kuruluşa katılma tarihi proje katılma tarihinin en iyi tahmini.
UPDATE "ProjectMember" pm
SET "joinedAt" = om."joinedAt"
FROM "Project" p, "OrganizationMember" om
WHERE p."id" = pm."projectId" AND om."organizationId" = p."organizationId" AND om."userId" = pm."userId"
  AND pm."inviteId" IS NULL AND pm."joinedAt" > om."joinedAt";

-- ------------------------------------------------------------------ rol ve grup kopyaları

INSERT INTO "Role" ("id", "organizationId", "projectId", "sourceRoleId", "handle", "name", "description", "color", "everyone", "founder", "permissions", "createdById", "createdAt", "updatedAt")
SELECT md5(random()::text || clock_timestamp()::text || p."id" || r."id")::uuid::text,
       r."organizationId", p."id", r."id", r."handle", r."name", r."description", r."color", r."everyone", r."founder", r."permissions", r."createdById", r."createdAt", NOW()
FROM "Project" p
JOIN "Role" r ON r."organizationId" = p."organizationId" AND r."projectId" IS NULL
ON CONFLICT DO NOTHING;

INSERT INTO "RoleMember" ("roleId", "userId", "addedAt")
SELECT nr."id", rm."userId", rm."addedAt"
FROM "Role" nr
JOIN "RoleMember" rm ON rm."roleId" = nr."sourceRoleId"
JOIN "ProjectMember" pm ON pm."projectId" = nr."projectId" AND pm."userId" = rm."userId"
WHERE nr."projectId" IS NOT NULL
ON CONFLICT DO NOTHING;

INSERT INTO "UserGroup" ("id", "organizationId", "projectId", "sourceGroupId", "handle", "name", "description", "color", "createdById", "createdAt", "updatedAt")
SELECT md5(random()::text || clock_timestamp()::text || p."id" || g."id")::uuid::text,
       g."organizationId", p."id", g."id", g."handle", g."name", g."description", g."color", g."createdById", g."createdAt", NOW()
FROM "Project" p
JOIN "UserGroup" g ON g."organizationId" = p."organizationId" AND g."projectId" IS NULL
ON CONFLICT DO NOTHING;

INSERT INTO "UserGroupMember" ("groupId", "userId", "addedAt")
SELECT ng."id", gm."userId", gm."addedAt"
FROM "UserGroup" ng
JOIN "UserGroupMember" gm ON gm."groupId" = ng."sourceGroupId"
JOIN "ProjectMember" pm ON pm."projectId" = ng."projectId" AND pm."userId" = gm."userId"
WHERE ng."projectId" IS NOT NULL
ON CONFLICT DO NOTHING;

INSERT INTO "RoleGroup" ("roleId", "groupId", "addedAt")
SELECT nr."id", ng."id", rg."addedAt"
FROM "RoleGroup" rg
JOIN "Role" nr ON nr."sourceRoleId" = rg."roleId" AND nr."projectId" IS NOT NULL
JOIN "UserGroup" ng ON ng."sourceGroupId" = rg."groupId" AND ng."projectId" = nr."projectId"
ON CONFLICT DO NOTHING;

-- ------------------------------------------------------------------ kanal kuralları

UPDATE "ChannelPermission" cp
SET "targetId" = nr."id"
FROM "ProjectChannel" ch
JOIN "ChannelCategory" c ON c."id" = ch."categoryId"
JOIN "Role" nr ON nr."projectId" = c."projectId"
WHERE cp."channelId" = ch."id" AND cp."targetType" = 'ROLE' AND nr."sourceRoleId" = cp."targetId";

UPDATE "ChannelPermission" cp
SET "targetId" = ng."id"
FROM "ProjectChannel" ch
JOIN "ChannelCategory" c ON c."id" = ch."categoryId"
JOIN "UserGroup" ng ON ng."projectId" = c."projectId"
WHERE cp."channelId" = ch."id" AND cp."targetType" = 'GROUP' AND ng."sourceGroupId" = cp."targetId";

-- ------------------------------------------------------------------ eksik temel roller ve davet kodu

INSERT INTO "Role" ("id", "organizationId", "projectId", "handle", "name", "description", "color", "everyone", "founder", "permissions", "createdById", "createdAt", "updatedAt")
SELECT md5(random()::text || clock_timestamp()::text || p."id" || 'everyone')::uuid::text,
       p."organizationId", p."id", 'everyone', 'Herkes', 'Projedeki herkes bu rolün yetkilerini alır.', '#79818c', true, false,
       ARRAY['channel.view','channel.manage','message.send','message.attach','message.react','message.forward','thread.reply','call.start','voice.connect','voice.speak','voice.video','screen.share','screen.annotate','direct.send','group.create','task.create','release.download']::text[],
       p."ownerId", NOW(), NOW()
FROM "Project" p
WHERE p."organizationId" IS NOT NULL
  AND NOT EXISTS (SELECT 1 FROM "Role" r WHERE r."projectId" = p."id" AND r."everyone")
  AND NOT EXISTS (SELECT 1 FROM "Role" r WHERE r."projectId" = p."id" AND r."handle" = 'everyone');

INSERT INTO "Role" ("id", "organizationId", "projectId", "handle", "name", "description", "color", "everyone", "founder", "permissions", "createdById", "createdAt", "updatedAt")
SELECT md5(random()::text || clock_timestamp()::text || p."id" || 'kurucu')::uuid::text,
       p."organizationId", p."id", 'kurucu', 'Kurucu', 'Projeyi kuran kişi: tüm yetkiler her zaman açıktır.', '#faa61a', false, true,
       ARRAY['admin.panel','organization.manage','member.roles','member.invite','member.kick','usergroup.manage','channel.view','channel.manage','channel.permissions','message.send','message.attach','message.react','message.forward','message.delete_any','message.delete_unlimited','message.mention_all','thread.reply','call.start','voice.connect','voice.speak','voice.video','screen.share','screen.annotate','task.create','board.manage','release.publish','release.download','direct.send','group.create']::text[],
       p."ownerId", NOW(), NOW()
FROM "Project" p
WHERE p."organizationId" IS NOT NULL
  AND NOT EXISTS (SELECT 1 FROM "Role" r WHERE r."projectId" = p."id" AND r."founder")
  AND NOT EXISTS (SELECT 1 FROM "Role" r WHERE r."projectId" = p."id" AND r."handle" = 'kurucu');

INSERT INTO "RoleMember" ("roleId", "userId", "addedAt")
SELECT r."id", pm."userId", NOW()
FROM "Role" r
JOIN "ProjectMember" pm ON pm."projectId" = r."projectId" AND pm."role" = 'OWNER'
WHERE r."founder" AND r."projectId" IS NOT NULL
ON CONFLICT DO NOTHING;

INSERT INTO "ProjectInvite" ("id", "code", "projectId", "createdById", "createdAt")
SELECT md5(random()::text || clock_timestamp()::text || p."id" || 'invite')::uuid::text,
       'P2-' || upper(substr(md5(random()::text || clock_timestamp()::text || p."id"), 1, 8)),
       p."id", p."ownerId", NOW()
FROM "Project" p
WHERE p."organizationId" IS NOT NULL
  AND NOT EXISTS (SELECT 1 FROM "ProjectInvite" i WHERE i."projectId" = p."id")
ON CONFLICT DO NOTHING;

-- ------------------------------------------------------------------ eski kuruluş davetleri

-- 2026-09-29'dan beri kuruluş koduyla katılan kişi hiçbir projeyi görmüyordu; kodlar artık bir şeye
-- karşılık gelmediği için kapatılır (gerekçe: docs/proje-bazli-yapi.md, "Göç planı" 6).
UPDATE "OrganizationInvite" SET "revokedAt" = NOW() WHERE "revokedAt" IS NULL;
