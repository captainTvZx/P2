ALTER TABLE "UserGroup" ADD COLUMN "permissions" TEXT[] NOT NULL DEFAULT '{}';
ALTER TABLE "OrganizationMember" ADD COLUMN "permissions" TEXT[] NOT NULL DEFAULT '{}';
ALTER TABLE "OrganizationMember" ADD COLUMN "deniedPermissions" TEXT[] NOT NULL DEFAULT '{}';
