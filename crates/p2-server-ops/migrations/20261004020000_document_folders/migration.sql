CREATE TABLE IF NOT EXISTS "ProjectDocumentFolder" (
  "id" TEXT NOT NULL,
  "projectId" TEXT NOT NULL,
  "name" TEXT NOT NULL,
  "position" INTEGER NOT NULL DEFAULT 0,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "updatedAt" TIMESTAMP(3) NOT NULL,
  CONSTRAINT "ProjectDocumentFolder_pkey" PRIMARY KEY ("id")
);

ALTER TABLE "ProjectDocument" ADD COLUMN IF NOT EXISTS "folderId" TEXT;
CREATE INDEX IF NOT EXISTS "ProjectDocumentFolder_projectId_position_idx" ON "ProjectDocumentFolder"("projectId", "position");
CREATE INDEX IF NOT EXISTS "ProjectDocument_folderId_idx" ON "ProjectDocument"("folderId");

DO $$ BEGIN
  ALTER TABLE "ProjectDocumentFolder" ADD CONSTRAINT "ProjectDocumentFolder_projectId_fkey" FOREIGN KEY ("projectId") REFERENCES "Project"("id") ON DELETE CASCADE ON UPDATE CASCADE;
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
  ALTER TABLE "ProjectDocument" ADD CONSTRAINT "ProjectDocument_folderId_fkey" FOREIGN KEY ("folderId") REFERENCES "ProjectDocumentFolder"("id") ON DELETE SET NULL ON UPDATE CASCADE;
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;
