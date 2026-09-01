// "Hasan Özoğlu" -> HÖ, "Hasan Özoğlu - 2" -> HÖ2, "captainTvZ" -> CA
export function userInitials(name:string){
  const parts=name.trim().split(/[\s._-]+/).filter(Boolean)
  const words=parts.filter(part=>/\p{L}/u.test(part))
  const digits=parts.filter(part=>/^\d+$/.test(part)).join('').slice(0,2)
  const letters=(words.length===1&&!digits?words[0].slice(0,2):words.slice(0,2).map(part=>part[0]).join('')).toLocaleUpperCase('tr-TR')
  return (letters+digits)||name.trim().slice(0,2).toLocaleUpperCase('tr-TR')||'P2'
}

// Her kişiye kimliğinden türeyen sabit bir renk: aynı kullanıcı her yerde aynı tonda görünür.
const AVATAR_TONES=10
export function avatarHue(key:string){
  let hash=0
  for(let index=0;index<key.length;index++)hash=(hash*31+key.charCodeAt(index))>>>0
  return `hue-${hash%AVATAR_TONES}`
}
