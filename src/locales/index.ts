import { de } from './de'
import { en, enGB } from './en'
import { es } from './es'
import { fr } from './fr'
import { ja } from './ja'
import { ptBR } from './pt'
import { tr } from './tr'
import { zhCN, zhTW } from './zh'
import type { Dict, LangId } from './types'

// Bölgesel varyantlar aynı kaynak sözlüğü paylaşır; eksik diller İngilizce
// sözlüğüyle başlar ve ayrı çeviriler geldikçe kendi dosyasına ayrılabilir.
export const DICTIONARIES:Record<LangId,Dict>={
  tr,en,'en-GB':enGB,de,es,'es-LA':es,fr,'pt-BR':ptBR,ja,'zh-CN':zhCN,'zh-TW':zhTW,
}
