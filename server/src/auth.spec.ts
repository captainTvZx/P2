import { describe, expect, it } from 'vitest'
import { validate } from 'class-validator'
import { plainToInstance } from 'class-transformer'
import { RegisterDto } from './auth'

describe('RegisterDto', () => {
  it('geçerli kayıt verisini kabul eder', async () => {
    const dto = plainToInstance(RegisterDto, { email: 'dev@p2.local', username: 'developer', displayName: 'Developer', password: 'Secure-pass1', passwordConfirm: 'Secure-pass1' })
    expect(await validate(dto)).toHaveLength(0)
  })
  it('boşluklu kullanıcı adını ve büyük harf/rakam içermeyen parolayı reddeder', async () => {
    const dto = plainToInstance(RegisterDto, { email: 'dev@p2.local', username: 'hasan ozoglu', displayName: 'Dev', password: 'sadecekucuk', passwordConfirm: 'sadecekucuk' })
    const errors = await validate(dto)
    expect(errors.map(e => e.property)).toEqual(expect.arrayContaining(['username', 'password']))
    expect(Object.values(errors.find(e => e.property === 'password')!.constraints!).join()).toContain('büyük harf')
  })
  it('büyük/küçük harf ve rakam içeren parolayı kabul eder', async () => {
    const dto = plainToInstance(RegisterDto, { email: 'dev@p2.local', username: 'Hasan.Ozoglu', displayName: 'Hasan Özoğlu', password: 'Parola123', passwordConfirm: 'Parola123' })
    expect(await validate(dto)).toHaveLength(0)
  })
  it('geçersiz e-posta ve kısa parolayı reddeder', async () => {
    const dto = plainToInstance(RegisterDto, { email: 'wrong', username: 'ok_user', displayName: 'Dev', password: 'short', passwordConfirm: 'short' })
    const fields = (await validate(dto)).map(e => e.property)
    expect(fields).toContain('email'); expect(fields).toContain('password')
  })
})
