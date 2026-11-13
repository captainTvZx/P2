-- Existing installations were created before project visibility was enforced.
-- Preserve today's behaviour once by granting every current organization member
-- access to every current project. New projects start with their creator only.
INSERT INTO "ProjectMember" ("projectId", "userId", "role")
SELECT
  project."id",
  member."userId",
  CASE WHEN project."ownerId" = member."userId" THEN 'OWNER'::"ProjectRole" ELSE 'MEMBER'::"ProjectRole" END
FROM "Project" AS project
JOIN "OrganizationMember" AS member
  ON member."organizationId" = project."organizationId"
ON CONFLICT ("projectId", "userId") DO NOTHING;
