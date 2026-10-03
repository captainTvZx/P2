import { createRequire } from 'node:module'
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { execFileSync } from 'node:child_process'

const require = createRequire(import.meta.url)
const { validateArchive, cleanDatabaseUrl } = require('../server/ops/backup.cjs')
const root = mkdtempSync(join(tmpdir(), 'p2-backup-test-'))
const tar = process.platform === 'win32' ? 'tar.exe' : 'tar'
let failed = 0
const check = (label, ok) => { console.log(`${ok ? 'OK  ' : 'FAIL'} ${label}`); if (!ok) failed++ }

try {
  const valid = join(root, 'valid')
  mkdirSync(join(valid, 'uploads'), { recursive: true })
  writeFileSync(join(valid, 'manifest.json'), JSON.stringify({ format: 1, product: 'P2 Server' }))
  writeFileSync(join(valid, 'database.dump'), 'PGDMP-test')
  writeFileSync(join(valid, 'uploads', 'sample.txt'), 'sample')
  const validArchive = join(root, 'valid.p2backup')
  execFileSync(tar, ['-czf', validArchive, '-C', valid, 'manifest.json', 'database.dump', 'uploads'])
  await validateArchive(validArchive)
  check('geçerli P2 yedeği doğrulandı', true)

  const invalid = join(root, 'invalid')
  mkdirSync(invalid)
  writeFileSync(join(invalid, 'manifest.json'), '{}')
  writeFileSync(join(invalid, 'database.dump'), 'x')
  writeFileSync(join(invalid, 'unexpected.txt'), 'x')
  const invalidArchive = join(root, 'invalid.p2backup')
  execFileSync(tar, ['-czf', invalidArchive, '-C', invalid, 'manifest.json', 'database.dump', 'unexpected.txt'])
  let rejected = false
  try { await validateArchive(invalidArchive) } catch { rejected = true }
  check('beklenmeyen arşiv içeriği reddedildi', rejected)

  const cleaned = cleanDatabaseUrl('postgresql://user:pass@127.0.0.1:5432/p2?schema=public&sslmode=require')
  check('Prisma schema parametresi pg_dump adresinden çıkarıldı', !cleaned.includes('schema=') && cleaned.includes('sslmode=require'))
} finally {
  rmSync(root, { recursive: true, force: true })
}
process.exitCode = failed ? 1 : 0
