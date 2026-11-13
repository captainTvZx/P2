ALTER TABLE "Activity" ADD COLUMN IF NOT EXISTS "boardId" TEXT;
ALTER TABLE "Activity" ADD COLUMN IF NOT EXISTS "taskId" TEXT;
ALTER TABLE "Activity" ADD COLUMN IF NOT EXISTS "documentId" TEXT;
CREATE INDEX IF NOT EXISTS "Activity_taskId_idx" ON "Activity"("taskId");
CREATE INDEX IF NOT EXISTS "Activity_documentId_idx" ON "Activity"("documentId");
