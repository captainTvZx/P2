export type PopoutKind='channel'|'person'|'group'

// Sohbeti ayrı bir pencerede açar. Masaüstünde ana süreç yeni bir pencere kurar;
// tarayıcıda aynı adres "popout" kipiyle yeni sekmede açılır.
export function openConversationWindow(kind:PopoutKind,id:string,name:string,extra:Record<string,string>={}){
  const query=new URLSearchParams({popout:'1',kind,id,...extra}).toString()
  const title=`${kind==='channel'?'#':''}${name} · P2`
  if(window.p2?.windowControls?.openPopout){window.p2.windowControls.openPopout(query,title);return}
  const base=window.location.href.split('?')[0].split('#')[0]
  window.open(`${base}?${query}`,`p2-${kind}-${id}`,'width=980,height=760')
}

// Açılan pencerenin hangi sohbeti göstereceği adresten okunur.
export const popoutTarget=(()=>{
  try{
    const query=new URLSearchParams(window.location.search)
    if(query.get('popout')!=='1')return null
    const kind=query.get('kind')
    if(kind!=='channel'&&kind!=='person'&&kind!=='group')return null
    return {kind:kind as PopoutKind,id:query.get('id')||'',projectId:query.get('project')||''}
  }catch{return null}
})()
