-- Görev yorumlarına tepki (sohbet mesajlarındaki gibi).
CREATE TABLE "TaskCommentReaction" (
  "id" TEXT NOT NULL,
  "commentId" TEXT NOT NULL,
  "userId" TEXT NOT NULL,
  "emoji" TEXT NOT NULL,
  "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT "TaskCommentReaction_pkey" PRIMARY KEY ("id")
);
CREATE UNIQUE INDEX "TaskCommentReaction_commentId_userId_emoji_key" ON "TaskCommentReaction"("commentId", "userId", "emoji");
CREATE INDEX "TaskCommentReaction_commentId_createdAt_idx" ON "TaskCommentReaction"("commentId", "createdAt");
ALTER TABLE "TaskCommentReaction" ADD CONSTRAINT "TaskCommentReaction_commentId_fkey" FOREIGN KEY ("commentId") REFERENCES "TaskComment"("id") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "TaskCommentReaction" ADD CONSTRAINT "TaskCommentReaction_userId_fkey" FOREIGN KEY ("userId") REFERENCES "User"("id") ON DELETE CASCADE ON UPDATE CASCADE;
