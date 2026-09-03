// Grup adı üyelerin adlarından oluşur: "Ali, Cansu, Hasan". Ayrı dosyada tutuluyor
// çünkü hem grup denetleyicisi hem gerçek zamanlı ağ geçidi kullanıyor.
export function groupNameOf(names: string[]) {
  const list = [...new Set(names.filter(Boolean).map(name => name.trim()))].sort((a, b) => a.localeCompare(b, 'tr'))
  if (!list.length) return 'Yeni grup'
  const shown = list.slice(0, 4).join(', ')
  return (list.length > 4 ? `${shown} +${list.length - 4}` : shown).slice(0, 60)
}

// Ad elle mi verilmiş yoksa üyelerden mi türemiş? Türemişse üye değişince tazelenir.
export function isAutoName(name: string, names: string[]) {
  const parts = name.replace(/\s\+\d+$/, '').split(',').map(part => part.trim()).filter(Boolean)
  const pool = names.map(item => item.trim())
  return parts.length > 0 && parts.every(part => pool.includes(part))
}
