export type PermState='allow'|'deny'|'inherit'

// Yetki satırı. Tek tıkla döngüye giren gizemli bir kutu yerine üç ayrı seçenek:
// engelle · varsayılan · izin ver. Hangi durumda olduğu bakışta anlaşılır ve
// istediğin duruma doğrudan geçersin.
export function PermissionRow({label,hint,state,disabled,onChange}:{
  label:string
  hint:string
  state:PermState
  disabled?:boolean
  onChange:(next:PermState)=>void
}){
  const options:Array<{value:PermState;glyph:string;title:string}>=[
    {value:'deny',glyph:'✕',title:'Engelle'},
    {value:'inherit',glyph:'／',title:'Varsayılan'},
    {value:'allow',glyph:'✓',title:'İzin ver'},
  ]
  return <div className={`perm-line state-${state}${disabled?' locked':''}`}>
    <span className="perm-line-text"><b>{label}</b><small>{hint}</small></span>
    <span className="perm-switch" role="radiogroup" aria-label={label}>
      {options.map(option=><button
        type="button" key={option.value} className={state===option.value?`on ${option.value}`:option.value}
        role="radio" aria-checked={state===option.value} title={option.title} disabled={disabled}
        onClick={()=>{if(state!==option.value)onChange(option.value)}}
      >{option.glyph}</button>)}
    </span>
  </div>
}

// İki durumlu satır (rol yetkileri gibi: ya var ya yok).
export function PermissionToggle({label,hint,on,disabled,onChange}:{
  label:string
  hint:string
  on:boolean
  disabled?:boolean
  onChange:(next:boolean)=>void
}){
  return <button type="button" className={`perm-line toggle${on?' state-allow':''}${disabled?' locked':''}`} disabled={disabled} onClick={()=>onChange(!on)} role="switch" aria-checked={on}>
    <span className="perm-line-text"><b>{label}</b><small>{hint}</small></span>
    <i className="perm-knob"/>
  </button>
}
