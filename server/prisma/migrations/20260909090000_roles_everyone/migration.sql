ALTER TABLE "UserGroup" ADD COLUMN "everyone" BOOLEAN NOT NULL DEFAULT false;

-- Her kuruluşa, kuruluştaki herkesi kapsayan silinemez @everyone rolü.
INSERT INTO "UserGroup" ("id", "organizationId", "handle", "name", "description", "color", "everyone", "permissions", "createdById", "createdAt", "updatedAt")
SELECT
  md5(random()::text || clock_timestamp()::text || o."id")::uuid,
  o."id", 'everyone', 'Herkes', 'Kuruluştaki herkes bu rolün yetkilerini alır.', '#79818c', true,
  ARRAY['channel.view','channel.manage','message.send','message.attach','message.react','message.forward','thread.reply','call.start','voice.connect','voice.speak','voice.video','screen.share','screen.annotate','direct.send','group.create']::text[],
  (SELECT m."userId" FROM "OrganizationMember" m WHERE m."organizationId" = o."id" ORDER BY (m."role" = 'OWNER') DESC, m."joinedAt" ASC LIMIT 1),
  NOW(), NOW()
FROM "Organization" o
WHERE EXISTS (SELECT 1 FROM "OrganizationMember" m WHERE m."organizationId" = o."id")
  AND NOT EXISTS (SELECT 1 FROM "UserGroup" g WHERE g."organizationId" = o."id" AND g."everyone" = true);

-- Eski ADMIN rolü artık sabit değil: aynı yetkileri taşıyan bir "Yönetici" rolü kurulur.
INSERT INTO "UserGroup" ("id", "organizationId", "handle", "name", "description", "color", "everyone", "permissions", "createdById", "createdAt", "updatedAt")
SELECT
  md5(random()::text || clock_timestamp()::text || o."id" || 'admin')::uuid,
  o."id", 'yonetici', 'Yönetici', 'Eski yönetici rolünden taşınan yetkiler.', '#5865f2', false,
  ARRAY['admin.panel','member.invite','member.kick','usergroup.manage','message.delete_any','message.mention_all','channel.permissions']::text[],
  (SELECT m."userId" FROM "OrganizationMember" m WHERE m."organizationId" = o."id" ORDER BY (m."role" = 'OWNER') DESC, m."joinedAt" ASC LIMIT 1),
  NOW(), NOW()
FROM "Organization" o
WHERE EXISTS (SELECT 1 FROM "OrganizationMember" m WHERE m."organizationId" = o."id" AND m."role" = 'ADMIN')
  AND NOT EXISTS (SELECT 1 FROM "UserGroup" g WHERE g."organizationId" = o."id" AND g."handle" = 'yonetici');

INSERT INTO "UserGroupMember" ("groupId", "userId", "addedAt")
SELECT g."id", m."userId", NOW()
FROM "UserGroup" g
JOIN "OrganizationMember" m ON m."organizationId" = g."organizationId" AND m."role" = 'ADMIN'
WHERE g."handle" = 'yonetici' AND g."everyone" = false
ON CONFLICT DO NOTHING;

-- Sahip dışındaki herkes artık düz üye; yetkiler rollerden gelir.
UPDATE "OrganizationMember" SET "role" = 'MEMBER' WHERE "role" = 'ADMIN';

-- Sabit rollere bağlı kanal kuralları anlamını yitirdi.
DELETE FROM "ChannelPermission" WHERE "targetType" = 'ROLE';
