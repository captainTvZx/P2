const fs = require('node:fs')
const path = require('node:path')
const { Client } = require('pg')

async function main() {
  const target = new Client({ connectionString: process.env.DATABASE_URL })
  await target.connect()
  try {
    await target.query('CREATE TABLE IF NOT EXISTS "_p2_migrations" ("name" TEXT PRIMARY KEY, "appliedAt" TIMESTAMPTZ NOT NULL DEFAULT NOW())')
    const root = path.join(__dirname, '..', 'server', 'prisma', 'migrations')
    const names = fs.readdirSync(root, { withFileTypes: true }).filter(item => item.isDirectory()).map(item => item.name).sort()
    for (const name of names) {
      if ((await target.query('SELECT 1 FROM "_p2_migrations" WHERE "name" = $1', [name])).rowCount) continue
      const prismaTable = await target.query('SELECT to_regclass(\'public."_prisma_migrations"\') AS table_name')
      if (prismaTable.rows[0]?.table_name) {
        const done = await target.query('SELECT 1 FROM "_prisma_migrations" WHERE "migration_name" = $1 AND "finished_at" IS NOT NULL AND "rolled_back_at" IS NULL', [name])
        if (done.rowCount) { await target.query('INSERT INTO "_p2_migrations" ("name") VALUES ($1)', [name]); continue }
      }
      if (name.endsWith('_init')) {
        const legacy = await target.query('SELECT to_regclass(\'public."User"\') AS table_name')
        if (legacy.rows[0]?.table_name) { await target.query('INSERT INTO "_p2_migrations" ("name") VALUES ($1)', [name]); continue }
        await target.query('DROP TYPE IF EXISTS "UserRole" CASCADE; DROP TYPE IF EXISTS "ProjectRole" CASCADE; DROP TYPE IF EXISTS "FriendshipStatus" CASCADE; DROP TYPE IF EXISTS "UserStatus" CASCADE;')
      }
      await target.query('BEGIN')
      try {
        await target.query(fs.readFileSync(path.join(root, name, 'migration.sql'), 'utf8'))
        await target.query('INSERT INTO "_p2_migrations" ("name") VALUES ($1)', [name])
        await target.query('COMMIT')
      } catch (error) { await target.query('ROLLBACK'); throw error }
    }
  } finally { await target.end() }
}
main().catch(error => { console.error(error?.stack || error); process.exit(1) })
