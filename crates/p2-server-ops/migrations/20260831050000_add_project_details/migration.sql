ALTER TABLE "Project" ADD COLUMN "slug" TEXT;
ALTER TABLE "Project" ADD COLUMN "engine" TEXT;
UPDATE "Project" SET "slug" = LOWER(REPLACE("name", ' ', '-')) || '-' || SUBSTRING("id", 1, 6) WHERE "slug" IS NULL;
ALTER TABLE "Project" ALTER COLUMN "slug" SET NOT NULL;
CREATE UNIQUE INDEX "Project_organizationId_slug_key" ON "Project"("organizationId", "slug");
