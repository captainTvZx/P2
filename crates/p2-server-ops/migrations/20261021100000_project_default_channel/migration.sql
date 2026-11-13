-- Proje açılınca ilk gösterilecek kanal (isteğe bağlı). Kanal silinirse
-- uygulama tarafı alanı yok sayar; zorunlu bir FK tutulmaz.
ALTER TABLE "Project" ADD COLUMN IF NOT EXISTS "defaultChannelId" TEXT;
