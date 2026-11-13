ALTER TABLE "UserGroup" ADD COLUMN "founder" BOOLEAN NOT NULL DEFAULT false;

-- Kurucu rolü: tüm yetkileri taşır, silinemez ve üyeliği kuruluş sahipliğini izler.
INSERT INTO "UserGroup" ("id", "organizationId", "handle", "name", "description", "color", "everyone", "founder", "permissions", "createdById", "createdAt", "updatedAt")
SELECT
  md5(random()::text || clock_timestamp()::text || o."id" || 'founder')::uuid,
  o."id", 'kurucu', 'Kurucu', 'Kuruluşu kuran kişi: tüm yetkiler her zaman açıktır.', '#faa61a', false, true,
  ARRAY['admin.panel','organization.manage','member.roles','member.invite','member.kick','usergroup.manage','channel.view','channel.manage','channel.permissions','message.send','message.attach','message.react','message.forward','message.delete_any','message.mention_all','thread.reply','call.start','voice.connect','voice.speak','voice.video','screen.share','screen.annotate','direct.send','group.create']::text[],
  (SELECT m."userId" FROM "OrganizationMember" m WHERE m."organizationId" = o."id" ORDER BY (m."role" = 'OWNER') DESC, m."joinedAt" ASC LIMIT 1),
  NOW(), NOW()
FROM "Organization" o
WHERE EXISTS (SELECT 1 FROM "OrganizationMember" m WHERE m."organizationId" = o."id")
  AND NOT EXISTS (SELECT 1 FROM "UserGroup" g WHERE g."organizationId" = o."id" AND g."founder" = true);

INSERT INTO "UserGroupMember" ("groupId", "userId", "addedAt")
SELECT g."id", m."userId", NOW()
FROM "UserGroup" g
JOIN "OrganizationMember" m ON m."organizationId" = g."organizationId" AND m."role" = 'OWNER'
WHERE g."founder" = true
ON CONFLICT DO NOTHING;
