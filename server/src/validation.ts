import { BadRequestException, Body, Type, ValidationPipe } from '@nestjs/common'
import { ValidationError } from 'class-validator'

// Yalnızca bizim yazdığımız (context.safe) mesajlar kullanıcıya gösterilir,
// class-validator'ın varsayılan İngilizce metinleri değil.
export function validationException(errors: ValidationError[]) {
  const safe = errors.map(error => {
    const key = Object.keys(error.constraints || {}).find(name => (error.contexts as any)?.[name]?.safe)
    return key ? { message: error.constraints![key], field: error.property } : null
  }).find(Boolean)
  return new BadRequestException({ code: 'VALIDATION_ERROR', message: safe?.message || 'Girdi alanlarını kontrol edin.', field: safe?.field || errors[0]?.property })
}

// tsx/esbuild `emitDecoratorMetadata` üretmediği için geliştirme modunda global
// ValidationPipe gövde tipini göremiyor ve doğrulamayı sessizce atlıyordu.
// DTO sınıfını açıkça vererek doğrulama hem geliştirmede hem üretimde çalışır.
export const Dto = (type: Type<any>) => Body(new ValidationPipe({ whitelist: true, transform: true, expectedType: type, exceptionFactory: validationException }))
