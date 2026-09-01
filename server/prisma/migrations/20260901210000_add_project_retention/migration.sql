ALTER TABLE "Project" ADD COLUMN "deletedAt" TIMESTAMP(3), ADD COLUMN "purgeAt" TIMESTAMP(3);
CREATE INDEX "Project_purgeAt_idx" ON "Project"("purgeAt");
