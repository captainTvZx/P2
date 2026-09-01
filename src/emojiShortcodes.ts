import emojiData from 'emoji-picker-react/dist/data/emojis-en'

let shortcodeMap:Map<string,string>|null=null

function unicodeFromUnified(unified:string){return unified.split('-').map(code=>String.fromCodePoint(Number.parseInt(code,16))).join('')}
function normalize(name:string){return name.toLocaleLowerCase('en-US').trim().replace(/&/g,'and').replace(/['’.]/g,'').replace(/[^a-z0-9+-]+/g,'_').replace(/^_+|_+$/g,'')}

function getMap(){
  if(shortcodeMap)return shortcodeMap
  shortcodeMap=new Map<string,string>()
  for(const emojis of Object.values(emojiData.emojis))for(const item of emojis){
    const emoji=unicodeFromUnified(item.u)
    for(const name of [...item.n].reverse()){const key=normalize(name);if(key&&!shortcodeMap.has(key))shortcodeMap.set(key,emoji)}
  }
  const common:Record<string,string>={'+1':'👍','thumbsup':'👍','-1':'👎','thumbsdown':'👎','heart':'❤️','red_heart':'❤️','smile':'😄','grin':'😀','joy':'😂','rofl':'🤣','wink':'😉','blush':'😊','thinking':'🤔','sob':'😭','cry':'😢','fire':'🔥','tada':'🎉','rocket':'🚀','eyes':'👀','ok_hand':'👌','clap':'👏','pray':'🙏','muscle':'💪','wave':'👋','100':'💯','star':'⭐','warning':'⚠️','white_check_mark':'✅','x':'❌'}
  for(const[key,value]of Object.entries(common))shortcodeMap.set(key,value)
  return shortcodeMap
}

export function expandEmojiShortcodes(text:string){return text.replace(/:([a-zA-Z0-9_+\-]+):/g,(match,name)=>getMap().get(name.toLocaleLowerCase('en-US'))||match)}

export type EmojiShortcodeSuggestion={shortcode:string;emoji:string}
const popular=['smile','joy','heart','+1','fire','tada','rocket','eyes']
export function suggestEmojiShortcodes(query:string,limit=8):EmojiShortcodeSuggestion[]{
  const map=getMap(),needle=query.toLocaleLowerCase('en-US')
  const keys=needle?[...map.keys()].filter(key=>key.startsWith(needle)).sort((a,b)=>a.length-b.length):popular
  const seen=new Set<string>(),result:EmojiShortcodeSuggestion[]=[]
  for(const key of keys){const emoji=map.get(key);if(!emoji||seen.has(emoji))continue;seen.add(emoji);result.push({shortcode:key,emoji});if(result.length>=limit)break}
  return result
}
