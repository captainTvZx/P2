CREATE TABLE "UserPreference" ("userId" TEXT NOT NULL,"organizationId" TEXT NOT NULL,"kind" TEXT NOT NULL,"targetId" TEXT NOT NULL,"starred" BOOLEAN NOT NULL DEFAULT false,"notify" TEXT NOT NULL DEFAULT 'MENTIONS',"updatedAt" TIMESTAMP(3) NOT NULL,CONSTRAINT "UserPreference_pkey" PRIMARY KEY ("userId","kind","targetId"));
CREATE INDEX "UserPreference_userId_organizationId_idx" ON "UserPreference"("userId","organizationId");
ALTER TABLE "UserPreference" ADD CONSTRAINT "UserPreference_userId_fkey" FOREIGN KEY ("userId") REFERENCES "User"("id") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "UserPreference" ADD CONSTRAINT "UserPreference_organizationId_fkey" FOREIGN KEY ("organizationId") REFERENCES "Organization"("id") ON DELETE CASCADE ON UPDATE CASCADE;
