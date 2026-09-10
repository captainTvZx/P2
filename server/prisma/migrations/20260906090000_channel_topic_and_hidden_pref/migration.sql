ALTER TABLE "ProjectChannel" ADD COLUMN "topic" TEXT;
ALTER TABLE "UserPreference" ADD COLUMN "hidden" BOOLEAN NOT NULL DEFAULT false;
