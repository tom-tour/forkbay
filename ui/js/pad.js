// Controller and arrow keys.
function padArea(){if($('imgBox').style.display=='block')return[$('panel')];const w=$('wiz');if(w.style.display=='block')return[w];const p=document.querySelector('.page.on');return[document.querySelector('aside'),p]}
function padItems(){return padArea().flatMap(a=>[...a.querySelectorAll('button,select,input,textarea,summary,a[href]')]).filter(e=>!e.disabled&&e.offsetParent!==null)}
function padMove(dir){const items=padItems();if(!items.length)return;const cur=document.activeElement;if(!items.includes(cur)){items[0].focus();items[0].scrollIntoView({block:'nearest'});return}const a=cur.getBoundingClientRect(),ax=a.left+a.width/2,ay=a.top+a.height/2;let best=null,bs=1e9;for(const e of items){if(e==cur)continue;const b=e.getBoundingClientRect(),dx=b.left+b.width/2-ax,dy=b.top+b.height/2-ay;const along={up:-dy,down:dy,left:-dx,right:dx}[dir],across=(dir=='up'||dir=='down')?Math.abs(dx):Math.abs(dy);if(along<=4)continue;const s=along+across*2;if(s<bs){bs=s;best=e}}if(best){best.focus();best.scrollIntoView({block:'nearest'})}}
// A list is opened with A before its choice moves, so that going past it changes nothing.
// Its choice is kept with A and put back with B, or when the list loses the focus.
let PADSEL=null;
function padSelOpen(sel){PADSEL={sel,was:sel.selectedIndex};sel.classList.add('padedit');$('statusText').textContent='Gauche ou droite pour choisir, A pour garder, B pour annuler.'}
function padSelClose(keep){const p=PADSEL;if(!p)return;PADSEL=null;p.sel.classList.remove('padedit');$('statusText').textContent='Prêt.';if(!keep)p.sel.selectedIndex=p.was;else if(p.sel.selectedIndex!=p.was)p.sel.dispatchEvent(new Event('change',{bubbles:true}))}
function padCycle(sel,d){const n=sel.selectedIndex+d;if(n<0||n>=sel.options.length)return;sel.selectedIndex=n}
document.addEventListener('focusout',e=>{if(PADSEL&&e.target==PADSEL.sel)padSelClose(false)});
function padPage(d){if($('wiz').style.display=='block')return;const i=pages.findIndex(([p])=>$('p-'+p).classList.contains('on'));const n=(i+d+pages.length)%pages.length;go(pages[n][0]);const b=$('n-'+pages[n][0]);b.focus()}
window.padAction=name=>{document.documentElement.dataset.pad='1';const cur=document.activeElement;if(PADSEL&&PADSEL.sel!=cur)padSelClose(false);if(PADSEL){if(name=='a'||name=='b')return padSelClose(name=='a');if(name=='left'||name=='up')return padCycle(cur,-1);if(name=='right'||name=='down')return padCycle(cur,1);return}if(['up','down','left','right'].includes(name))return padMove(name);if(name=='a'){if(cur&&cur.tagName=='SELECT'&&padItems().includes(cur))return padSelOpen(cur);if(cur&&padItems().includes(cur))cur.click();else padMove('down');return}if(name=='y'){const g=cur&&cur.closest&&cur.closest('.game');if(g)g.querySelector('.gear').click();return}if(name=='b'){if($('imgBox').style.display=='block')return closeBox();if($('wiz').style.display=='block'){if(wi>0)wiz(-1);return}const n=document.querySelector('nav button.on');if(n)n.focus();return}if(name=='lb')padPage(-1);if(name=='rb')padPage(1)};
const PAD_MAP={12:'up',13:'down',14:'left',15:'right',0:'a',1:'b',3:'y',4:'lb',5:'rb'};let padHeld={},padSeen=false;
// The controller is read by Forkbay itself, with its own controls from the Controller page, which are
// player 1's until they are set apart: A in Forkbay is then the button that is A in the games. The browser's own reading stays for a controller those
// controls do not use. The controller is let go while a game runs and while Forkbay is not in front.
const NAV_CTL={up:'button_dup',down:'button_ddown',left:'button_dleft',right:'button_dright',a:'button_a',b:'button_b',y:'button_y',lb:'button_l',rb:'button_r'};
let NAVSTATES=null,navBusy=false,navAt=0,navCtlAt=0,navHeld=false;
function navFree(){return document.hasFocus()&&!SESSION&&!OTHER&&!(window.NAVPAUSE>Date.now())}
function navRead(now){if(navBusy||now-navAt<45)return;navBusy=true;navAt=now;
  if(!CTL&&now-navCtlAt>5000){navCtlAt=now;call('controls_get').then(c=>{if(!CTL)CTL=c}).catch(()=>{})}
  call('controls_states').then(s=>{NAVSTATES=navFree()?s:null;navHeld=true}).catch(()=>{NAVSTATES=null}).finally(()=>{navBusy=false})}
function navRelease(){NAVSTATES=null;if(!navHeld)return;navHeld=false;if(!$('p-pad').classList.contains('on')||!document.hasFocus())call('controls_release').catch(()=>{})}
// What the controller presses now, or null when it is not plugged in or its controls are keys.
function navFromSdl(){if(!CTL||!NAVSTATES||!NAVSTATES.length)return null;const c=navPlayer().controls;
  const sdl=t=>{const p=pParse(t);return p.engine=='sdl'&&NAVSTATES.some(x=>x.guid==p.guid&&x.port==Number(p.port||0))};
  const stick=pParse(c.sticks.lstick).engine=='sdl'&&sdl(c.sticks.lstick);
  if(!stick&&!Object.values(NAV_CTL).some(k=>sdl(c.buttons[k])))return null;
  const on={};for(const n in NAV_CTL)if(sdl(c.buttons[NAV_CTL[n]])&&ctlActive(c.buttons[NAV_CTL[n]],NAVSTATES))on[n]=1;
  const pos=stick?ctlStickPos(c.sticks.lstick,NAVSTATES):null;if(pos){if(pos[0]<-.6)on.left=1;if(pos[0]>.6)on.right=1;if(pos[1]>.6)on.up=1;if(pos[1]<-.6)on.down=1}return on}
function navFromBrowser(){const pads=navigator.getGamepads?[...navigator.getGamepads()].filter(Boolean):[];if(!pads.length)return null;const p=pads[0],on={};for(const k in PAD_MAP)if(p.buttons[k]&&p.buttons[k].pressed)on[PAD_MAP[k]]=1;const x=p.axes[0]||0,y=p.axes[1]||0;if(x<-.6)on.left=1;if(x>.6)on.right=1;if(y<-.6)on.up=1;if(y>.6)on.down=1;return on}
function padPoll(now){if(window.PADHOLD){padHeld={};requestAnimationFrame(padPoll);return}
  if(navFree())navRead(now);else navRelease();
  const on=document.hasFocus()?(navFromSdl()||navFromBrowser()):null;
  if(on){if(!padSeen){padSeen=true;$('padHint').textContent='A / B / Y / L / R';}for(const n in on){const h=padHeld[n];const move=['up','down','left','right'].includes(n);if(!h||(move&&now-h.t>(h.r?140:420))){padAction(n);padHeld[n]={t:now,r:!!h}}}for(const n in padHeld)if(!on[n])delete padHeld[n]}else padHeld={};
  requestAnimationFrame(padPoll)}
// Controls changed in an emulator's own window are read again when Forkbay comes back in front.
window.addEventListener('focus',()=>{if(!$('p-pad').classList.contains('on'))call('controls_get').then(c=>{CTL=c}).catch(()=>{})});
requestAnimationFrame(padPoll);
document.addEventListener('keydown',e=>{if(e.key=='F11'){e.preventDefault();return toggleFs()}if(e.key=='Escape'&&!PADSEL&&$('imgBox').style.display=='block')return closeBox();const t=e.target;if(t&&(t.tagName=='TEXTAREA'||(t.tagName=='INPUT'&&t.type!='checkbox'&&t.type!='radio')))return;if(t&&t.tagName=='SELECT'&&(e.key=='Enter'||(PADSEL&&e.key=='Escape'))){e.preventDefault();e.stopPropagation();return padAction(e.key=='Enter'?'a':'b')}const m={ArrowUp:'up',ArrowDown:'down',ArrowLeft:'left',ArrowRight:'right'}[e.key];if(!m)return;e.preventDefault();padAction(m)});
