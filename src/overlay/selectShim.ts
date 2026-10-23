// Ekran dışı (OSR) pencerede <select> açılır listesi işletim sistemi penceresidir ve oyuna hiç çizilmez.
// Bu shim overlay modunda her <select>'e tıklamayı yakalar ve seçenekleri DOM içinde çizilen bir listeyle
// gösterir; seçim yapılınca değer React'in göreceği şekilde (native setter + change olayı) uygulanır.
const POP_CLASS='ow-select-pop'
let pop:HTMLDivElement|null=null
let current:HTMLSelectElement|null=null

function close(){
  pop?.remove();pop=null
  current?.classList.remove('ow-select-open');current=null
  document.removeEventListener('pointerdown',onOutside,true)
  document.removeEventListener('keydown',onKey,true)
  window.removeEventListener('resize',close)
}
function onOutside(event:Event){
  const target=event.target as HTMLElement|null
  if(pop&&target&&pop.contains(target))return
  close()
}
function onKey(event:KeyboardEvent){
  if(!pop)return
  if(event.key==='Escape'){event.preventDefault();event.stopPropagation();close();return}
  const items=Array.from(pop.querySelectorAll<HTMLButtonElement>('button:not(:disabled)'))
  const index=items.findIndex(item=>item===document.activeElement)
  if(event.key==='ArrowDown'){event.preventDefault();(items[Math.min(items.length-1,index+1)]||items[0])?.focus()}
  else if(event.key==='ArrowUp'){event.preventDefault();(items[Math.max(0,index-1)]||items[0])?.focus()}
  else if(event.key==='Enter'&&index>=0){event.preventDefault();items[index].click()}
}
function commit(select:HTMLSelectElement,value:string){
  const setter=Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value')?.set
  if(setter)setter.call(select,value);else select.value=value
  select.dispatchEvent(new Event('input',{bubbles:true}))
  select.dispatchEvent(new Event('change',{bubbles:true}))
}
function open(select:HTMLSelectElement){
  close()
  current=select
  select.classList.add('ow-select-open')
  const box=select.getBoundingClientRect()
  const menu=document.createElement('div')
  menu.className=POP_CLASS
  menu.setAttribute('role','listbox')
  const groups=Array.from(select.children)
  const addOption=(option:HTMLOptionElement)=>{
    const item=document.createElement('button')
    item.type='button';item.setAttribute('role','option')
    item.textContent=option.textContent||option.value
    item.disabled=option.disabled
    if(option.selected){item.classList.add('selected');item.setAttribute('aria-selected','true')}
    item.addEventListener('click',()=>{commit(select,option.value);close();select.focus()})
    menu.appendChild(item)
  }
  for(const child of groups){
    if(child instanceof HTMLOptGroupElement){
      const label=document.createElement('small');label.textContent=child.label;menu.appendChild(label)
      for(const option of Array.from(child.children))if(option instanceof HTMLOptionElement)addOption(option)
    }else if(child instanceof HTMLOptionElement)addOption(child)
  }
  document.body.appendChild(menu)
  // Konum: seçicinin altında; sığmazsa üstünde; yatay kıskaç
  const width=Math.max(box.width,160)
  const height=menu.offsetHeight
  let top=box.bottom+4,left=box.left
  if(top+height>window.innerHeight-8)top=Math.max(8,box.top-4-height)
  if(left+width>window.innerWidth-8)left=Math.max(8,window.innerWidth-8-width)
  Object.assign(menu.style,{left:`${Math.round(left)}px`,top:`${Math.round(top)}px`,minWidth:`${Math.round(width)}px`})
  pop=menu
  ;(menu.querySelector('button.selected')as HTMLButtonElement|null)?.focus()
  document.addEventListener('pointerdown',onOutside,true)
  document.addEventListener('keydown',onKey,true)
  window.addEventListener('resize',close)
}

export function startSelectShim(){
  document.addEventListener('mousedown',event=>{
    const target=event.target as HTMLElement|null
    const select=target?.closest?.('select') as HTMLSelectElement|null
    if(!select||select.multiple||select.disabled)return
    event.preventDefault()
    if(current===select){close();return}
    open(select)
  },true)
  // Klavyeyle açma (Space/Alt+↓) — yerel açılır liste yine engellenir
  document.addEventListener('keydown',event=>{
    const target=event.target as HTMLElement|null
    if(!(target instanceof HTMLSelectElement)||target.multiple)return
    if(event.key===' '||(event.altKey&&event.key==='ArrowDown')){event.preventDefault();open(target)}
  },true)
}
