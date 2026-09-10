-- Roller ayrı bir tabloya taşınır; gruplar yalnızca kişi kümesi olarak kalır.
CREATE TABLE "Role" (
  "id" TEXT NOT NULL,
  "organizationId" TEXT NOT NULL,
  "handle" TEXT NOT NULL,
  "name" TEXT NOT NULL,
  "description" TEXT,
  "color" TEXT,
  "everyone" BOOLEAN NOT NULL DEFAULT false,
  "founder" BOOLEAN NOT NULL DEFAULT false,
  "permissions" TEXT[] NOT NULL DEFAULT '{}',
  "createdById" TEXT NOT NULL,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "updatedAt" TIMESTAMP(3) NOT NULL,
  CONSTRAINT "Role_pkey" PRIMARY KEY ("id")
);
CREATE UNIQUE INDEX "Role_organizationId_handle_key" ON "Role"("organizationId", "handle");
CREATE INDEX "Role_organizationId_idx" ON "Role"("organizationId");
ALTER TABLE "Role" ADD CONSTRAINT "Role_organizationId_fkey" FOREIGN KEY ("organizationId") REFERENCES "Organization"("id") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "Role" ADD CONSTRAINT "Role_createdById_fkey" FOREIGN KEY ("createdById") REFERENCES "User"("id") ON DELETE CASCADE ON UPDATE CASCADE;

CREATE TABLE "RoleMember" (
  "roleId" TEXT NOT NULL,
  "userId" TEXT NOT NULL,
  "addedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "RoleMember_pkey" PRIMARY KEY ("roleId", "userId")
);
CREATE INDEX "RoleMember_userId_idx" ON "RoleMember"("userId");
ALTER TABLE "RoleMember" ADD CONSTRAINT "RoleMember_roleId_fkey" FOREIGN KEY ("roleId") REFERENCES "Role"("id") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "RoleMember" ADD CONSTRAINT "RoleMember_userId_fkey" FOREIGN KEY ("userId") REFERENCES "User"("id") ON DELETE CASCADE ON UPDATE CASCADE;

CREATE TABLE "RoleGroup" (
  "roleId" TEXT NOT NULL,
  "groupId" TEXT NOT NULL,
  "addedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "RoleGroup_pkey" PRIMARY KEY ("roleId", "groupId")
);
CREATE INDEX "RoleGroup_groupId_idx" ON "RoleGroup"("groupId");
ALTER TABLE "RoleGroup" ADD CONSTRAINT "RoleGroup_roleId_fkey" FOREIGN KEY ("roleId") REFERENCES "Role"("id") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "RoleGroup" ADD CONSTRAINT "RoleGroup_groupId_fkey" FOREIGN KEY ("groupId") REFERENCES "UserGroup"("id") ON DELETE CASCADE ON UPDATE CASCADE;

-- Var olan kayıtlar: temel roller ve yetki taşıyan gruplar Role tablosuna taşınır.
INSERT INTO "Role" ("id", "organizationId", "handle", "name", "description", "color", "everyone", "founder", "permissions", "createdById", "createdAt", "updatedAt")
SELECT g."id", g."organizationId", g."handle", g."name", g."description", g."color", g."everyone", g."founder", g."permissions", g."createdById", g."createdAt", NOW()
FROM "UserGroup" g
WHERE g."everyone" = true OR g."founder" = true OR array_length(g."permissions", 1) > 0;

INSERT INTO "RoleMember" ("roleId", "userId", "addedAt")
SELECT m."groupId", m."userId", m."addedAt"
FROM "UserGroupMember" m
JOIN "Role" r ON r."id" = m."groupId";

-- Taşınan kayıtlardan yalnızca temel roller gruptan silinir; yetkili özel gruplar
-- hem grup hem rol olarak yaşamaya devam eder ve grubu role bağlanır.
INSERT INTO "RoleGroup" ("roleId", "groupId", "addedAt")
SELECT r."id", r."id", NOW()
FROM "Role" r
WHERE r."everyone" = false AND r."founder" = false
  AND EXISTS (SELECT 1 FROM "UserGroup" g WHERE g."id" = r."id");

DELETE FROM "UserGroup" WHERE "everyone" = true OR "founder" = true;

ALTER TABLE "UserGroup" DROP COLUMN "permissions";
ALTER TABLE "UserGroup" DROP COLUMN "everyone";
ALTER TABLE "UserGroup" DROP COLUMN "founder";

-- Roller artık kendi kimlikleriyle hedeflenir; eski kurallar temizlenir.
DELETE FROM "ChannelPermission" WHERE "targetType" NOT IN ('ROLE', 'GROUP', 'USER');
