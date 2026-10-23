import { describe, expect, it } from 'vitest'
import { pageArgs } from './paging'

describe('pageArgs', () => {
  it('parametre yokken varsayılan sayfa boyutu ve boş where döner', () => {
    expect(pageArgs()).toEqual({ take: 100, where: {} })
    expect(pageArgs(undefined, undefined, { take: 200, max: 200 })).toEqual({ take: 200, where: {} })
  })

  it('limit 1..max aralığına kıstırılır, geçersiz değerler varsayılana düşer', () => {
    expect(pageArgs(undefined, '50').take).toBe(50)
    expect(pageArgs(undefined, '0').take).toBe(1)
    expect(pageArgs(undefined, '-5').take).toBe(1)
    expect(pageArgs(undefined, '999').take).toBe(100)
    expect(pageArgs(undefined, '12.9').take).toBe(12)
    expect(pageArgs(undefined, 'abc').take).toBe(100)
    expect(pageArgs(undefined, '').take).toBe(100)
    expect(pageArgs(undefined, '300', { take: 200, max: 200 }).take).toBe(200)
  })

  it('varsayılan sayfa boyutu da max ile sınırlanır', () => {
    expect(pageArgs(undefined, undefined, { take: 500, max: 200 }).take).toBe(200)
  })

  it('geçerli ISO imleç createdAt < imleç koşuluna çevrilir', () => {
    const iso = '2026-10-23T10:00:00.000Z'
    const args = pageArgs(iso, '50')
    expect(args.where).toEqual({ createdAt: { lt: new Date(iso) } })
    expect(args.take).toBe(50)
  })

  it('geçersiz veya boş imleç yok sayılır', () => {
    expect(pageArgs('not-a-date').where).toEqual({})
    expect(pageArgs('').where).toEqual({})
  })
})
