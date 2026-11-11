// Controller page: the controls every emulator shares, set the way their own Controls window sets them.
let CTL=null,CTLDEV=[],CTLTAB=0,CTLLIVE=null,CTLSTATES=[],CTLCAP=null,CTLKEYS=new Set(),CTLMENU=null;

// --- The emulators' parameter strings, as src/params.rs reads them ---
function pParse(t){const p={};if(!t||t=='[empty]')return p;for(const pair of String(t).replace(/^"|"$/g,'').split(',')){const kv=pair.split(':');if(kv.length!=2)continue;const un=s=>s.replace(/\$0/g,':').replace(/\$1/g,',').replace(/\$2/g,'$');p[un(kv[0])]=un(kv[1])}return p}
function pText(p){const ks=Object.keys(p);if(!ks.length)return'[empty]';const es=s=>String(s).replace(/\$/g,'$2').replace(/,/g,'$1').replace(/:/g,'$0');return ks.map(k=>es(k)+':'+es(p[k])).join(',')}
const fl=v=>Number(v).toFixed(6);

// --- Names of keys, as Qt key codes, and of controls ---
const QT_SPECIAL={0x01000000:'Échap',0x01000001:'Tab',0x01000003:'Retour arrière',0x01000004:'Entrée',0x01000005:'Entrée (pavé)',0x01000006:'Inser',0x01000007:'Suppr',0x01000010:'Début',0x01000011:'Fin',0x01000012:'Gauche',0x01000013:'Haut',0x01000014:'Droite',0x01000015:'Bas',0x01000016:'Page préc.',0x01000017:'Page suiv.',0x01000020:'Maj',0x01000021:'Ctrl',0x01000023:'Alt',0x01000024:'Verr. maj',0x20:'Espace'};
function keyName(code){code=Number(code);if(!code)return'non réglée';if(QT_SPECIAL[code])return QT_SPECIAL[code];if(code>=0x01000030&&code<=0x01000052)return'F'+(code-0x0100002f);if(code<0x01000000)return String.fromCharCode(code);return'Touche '+code}
function qtCode(e){const m={Escape:0x01000000,Tab:0x01000001,Backspace:0x01000003,Enter:e.location==3?0x01000005:0x01000004,Insert:0x01000006,Delete:0x01000007,Home:0x01000010,End:0x01000011,ArrowLeft:0x01000012,ArrowUp:0x01000013,ArrowRight:0x01000014,ArrowDown:0x01000015,PageUp:0x01000016,PageDown:0x01000017,Shift:0x01000020,Control:0x01000021,Alt:0x01000023,AltGraph:0x01000023,CapsLock:0x01000024,' ':0x20};if(m[e.key]!==undefined)return m[e.key];const f=/^F(\d{1,2})$/.exec(e.key);if(f)return 0x0100002f+Number(f[1]);if(e.key.length==1)return e.key.toUpperCase().charCodeAt(0);return 0}
const DIRS={up:'haut',down:'bas',left:'gauche',right:'droite'};
// The name of a control, with its options apart, each in its own element for the translation.
function bindParts(t){const p=pParse(t);if(!p.engine)return['non réglée',[]];const on=k=>p[k]=='1'||p[k]=='true';const tags=[on('turbo')?'turbo':'',on('toggle')?'bascule':'',on('inverted')?'inversé':'',p.axis!==undefined&&p.invert=='-'?'axe inversé':''].filter(Boolean);
  if(p.engine=='keyboard')return[keyName(p.code),tags];
  if(p.engine=='mouse')return['Souris '+(p.button||''),tags];
  if(p.hat!==undefined)return['Croix '+(DIRS[p.direction]||p.direction),tags];
  if(p.axis!==undefined)return['Axe '+p.axis,tags];
  if(p.axis_z!==undefined)return['Axes '+p.axis_x+', '+p.axis_y+', '+p.axis_z,tags];
  if(p.motion!==undefined)return['Gyroscope',tags];
  if(p.button!==undefined)return['Bouton '+p.button,tags];
  return[p.engine,tags]}
function bindHtml(t){const[n,tags]=bindParts(t);return`<span>${esc(n)}</span>${tags.length?' <span class="muted small">'+tags.map(x=>`<span>${x}</span>`).join(' · ')+'</span>':''}`}
function stickHtml(t,dir){const p=pParse(t);if(!p.engine)return'<span>non réglée</span>';if(p.engine=='analog_from_button')return bindHtml(p[dir]);if(p.axis_x===undefined||p.axis_y===undefined)return'<span>inconnue</span>';const ix=p.invert_x=='-',iy=p.invert_y=='-';return'<span>'+{left:'Axe '+p.axis_x+(ix?'+':'−'),right:'Axe '+p.axis_x+(ix?'−':'+'),up:'Axe '+p.axis_y+(iy?'−':'+'),down:'Axe '+p.axis_y+(iy?'+':'−')}[dir]+'</span>'}
const isPadStick=p=>p.engine&&!['keyboard','mouse','analog_from_button'].includes(p.engine);

// --- Kinds of controller, and what each one has (UpdateControllerAvailableButtons) ---
const TYPES=[[0,'Pro Controller'],[1,'Joy-Con doubles'],[2,'Joy-Con gauche'],[3,'Joy-Con droit'],[4,'Portable'],[5,'Manette GameCube']];
function layout(type){const all={lstick:1,dpad:1,left:1,minus:1,right:1,plus:1,home:1,face:1,rstick:1,slsrl:0,slsrr:0,capture:0,motl:1,motr:0};
  if(type==1)Object.assign(all,{motr:1});
  if(type==2)Object.assign(all,{right:0,plus:0,home:0,face:0,rstick:0,slsrl:1});
  if(type==3)Object.assign(all,{left:0,minus:0,lstick:0,dpad:0,slsrr:1,motl:0,motr:1});
  if(type==5)Object.assign(all,{minus:0,motl:0});
  return all}
const GC=t=>t==5;

function ctlPlayer(){if(!CTL)return null;const p=CTL.players[CTLTAB];return CTLTAB==0&&p.type==4?CTL.players[8]:p}

async function loadControls(){const el=$('ctl');if(!el)return;HK=null;try{[CTL,CTLDEV]=await Promise.all([call('controls_get'),call('controls_devices').catch(()=>[])])}catch(e){el.innerHTML=`<div class="card">${esc(e.message)}</div>`;return}renderControls();ctlLiveStart()}
function stopControls(){ctlLiveStop();ctlCancel();ctlCloseMenu();if(HKCAP){HKCAP=null;window.PADHOLD=false}}

function ctlTabs(){return`<div class="tabs ctltabs" id="ctlTabs">${[0,1,2,3,4,5,6,7].map(i=>`<button class="${CTLTAB===i?'on':''}${CTL.players[i==0&&CTL.players[0].type==4?8:i].connected?'':' off'}" onclick="ctlTab(${i})"><span>Joueur</span> ${i+1}</button>`).join('')}<button class="${CTLTAB=='adv'?'on':''}" onclick="ctlTab('adv')">Avancé</button><button class="${CTLTAB=='keys'?'on':''}" onclick="ctlTab('keys')">Raccourcis</button></div>`}
function ctlTab(t){ctlCancel();if(HKCAP)HKCAP=null,window.PADHOLD=false;CTLTAB=t;renderControls()}

function renderControls(){const el=$('ctl');if(!el||!CTL)return;ctlCloseMenu();
  const warn=CTL.running?'<div class="card" style="margin-bottom:14px"><b>Un émulateur est ouvert.</b><p class="muted small">Ferme-le avant de changer tes contrôles : il les remplacerait par les siens en se fermant.</p></div>':'';
  el.innerHTML=warn+ctlTabs()+(CTLTAB=='adv'?ctlAdvanced():CTLTAB=='keys'?ctlHotkeys():ctlPlayerHtml());ctlRefreshLive()}

function deviceOf(c){const seen=new Map();const add=t=>{const p=pParse(t);if(!p.engine||p.engine=='analog_from_button'){for(const d of['up','down','left','right','modifier'])if(p[d])add(p[d]);return}if(p.engine=='keyboard'||p.engine=='mouse'){seen.set('kb','kb');return}if(p.guid)seen.set(p.engine+p.guid+':'+(p.port||0),{guid:p.guid,port:Number(p.port||0)})};
  for(const g of['buttons','sticks','motions'])for(const k in c[g])if(!(g=='motions'&&pParse(c[g][k]).engine=='keyboard'))add(c[g][k]);
  const v=[...seen.values()];if(v.length==1)return v[0]=='kb'?'kb':'pad:'+v[0].guid+':'+v[0].port;
  // A Joy-Con pair is one choice.
  if(v.length==2&&v.every(x=>x!='kb')){const d=CTLDEV.find(d=>d.guid2&&d.port==v[0].port&&((d.guid==v[0].guid&&d.guid2==v[1].guid)||(d.guid==v[1].guid&&d.guid2==v[0].guid)));if(d)return'pad:'+d.guid+':'+d.port+':'+d.guid2}
  return v.length&&!v.includes('kb')?'pad:'+v[0].guid+':'+v[0].port:''}

function ctlPlayerHtml(){const P=CTL.players[CTLTAB],T=ctlPlayer(),c=T.controls,type=P.type,L=layout(type),gc=GC(type);
  const dev=deviceOf(c);const devs=CTLDEV.map(d=>['pad:'+d.guid+':'+d.port+(d.guid2?':'+d.guid2:''),d.name]);
  if(dev.startsWith('pad:')&&!devs.some(d=>d[0]==dev))devs.push([dev,'Manette débranchée']);
  const types=TYPES.filter(([v])=>v!=4||CTLTAB===0);if(!types.some(([v])=>v==type))types.push([type,'Autre manette']);
  const profs=CTL.profiles;
  const top=`<div class="card"><div class="row wrap" style="gap:14px;align-items:flex-end">
   <label class="check"><input type="checkbox" ${T.connected?'checked':''} onchange="ctlConnected(this.checked)"> <span>Connecté</span></label>
   <div><p class="muted small">Type de manette</p><select onchange="ctlType(this.value)">${types.map(([v,n])=>`<option value="${v}" ${v==type?'selected':''}>${esc(n)}</option>`).join('')}</select></div>
   <div style="flex:1 1 220px"><p class="muted small">Manette utilisée</p><select id="ctlDev" onchange="ctlDevice(this.value)"><option value="" ${dev==''?'selected':''}>Non choisie</option><option value="kb" ${dev=='kb'?'selected':''}>Clavier</option>${devs.map(([v,n])=>`<option value="${attr(v)}" ${v==dev?'selected':''} translate="no">${esc(n)}</option>`).join('')}</select></div>
   <div><p class="muted small">Profil</p><select id="ctlProf" onchange="ctlProfileLoad(this.value)"><option value="">—</option>${profs.map(n=>`<option translate="no">${esc(n)}</option>`).join('')}</select></div>
   <div class="row" style="gap:6px"><button class="btn" onclick="ctlProfileSave()">Enregistrer</button><button class="btn" onclick="ctlProfileNew()">Nouveau</button><button class="btn" onclick="ctlProfileDelete()">Supprimer</button></div>
  </div><div id="ctlProfMore"></div>${CTLDEV.length?'':'<p class="muted small" style="margin-top:10px">Aucune manette détectée : branche-la, elle apparaîtra ici.</p>'}<p class="muted small" style="margin-top:10px">Choisir une manette remplit toutes les touches, comme dans les émulateurs. Clique ensuite sur une touche pour la changer, puis appuie sur le bouton voulu. Les touches s'allument quand tu appuies.</p></div>`;
  const b=(label,name,on=1,dis=0)=>on?`<div class="bind${dis?' off':''}"><span class="bl">${label}</span><button class="bv" data-k="${name}" ${dis?'disabled':''} onclick="ctlCapture(this)">${bindHtml(c.buttons[name])}</button><button class="bm" data-k="${name}" ${dis?'disabled':''} onclick="ctlMenu(this)" aria-label="Options">⋯</button></div>`:'';
  const thr=n=>{const p=pParse(c.buttons[n]);return p.threshold!==undefined&&p.axis!==undefined?`<div class="bind slim"><span class="bl muted small">Seuil</span><input type="range" min="0" max="100" value="${Math.round(Number(p.threshold)*100)}" data-k="${n}" onchange="ctlThreshold(this)"><span class="muted small">${Math.round(Number(p.threshold)*100)} %</span></div>`:''};
  const stick=(id,title,pressName,pressOn=1)=>{const s=pParse(c.sticks[id]);const pad=isPadStick(s);
   return`<div class="grp ctlg"><h3>${title}</h3>${['up','down','left','right'].map(d=>`<div class="bind"><span class="bl">${{up:'Haut',down:'Bas',left:'Gauche',right:'Droite'}[d]}</span><button class="bv" data-s="${id}" data-d="${d}" onclick="ctlCapture(this)">${stickHtml(c.sticks[id],d)}</button><button class="bm" data-s="${id}" data-d="${d}" onclick="ctlMenu(this)" aria-label="Options">⋯</button></div>`).join('')}${b('Clic du stick',pressName,1,!pressOn)}
    ${pad?`<div class="bind slim"><span class="bl muted small">Zone morte</span><input type="range" min="0" max="100" value="${Math.round(Number(s.deadzone??0.15)*100)}" data-s="${id}" data-f="deadzone" onchange="ctlStickValue(this)"><span class="muted small">${Math.round(Number(s.deadzone??0.15)*100)} %</span></div><div class="bind slim"><span class="bl muted small">Portée</span><input type="range" min="25" max="150" value="${Math.round(Number(s.range??0.95)*100)}" data-s="${id}" data-f="range" onchange="ctlStickValue(this)"><span class="muted small">${Math.round(Number(s.range??0.95)*100)} %</span></div>`
     :`<div class="bind"><span class="bl">Modificateur</span><button class="bv" data-s="${id}" data-d="modifier" onclick="ctlCapture(this)">${bindHtml(s.modifier)}</button><button class="bm" data-s="${id}" data-d="modifier" onclick="ctlMenu(this)" aria-label="Options">⋯</button></div><div class="bind slim"><span class="bl muted small">Portée du modificateur</span><input type="range" min="0" max="100" value="${Math.round(Number(s.modifier_scale??0.5)*100)}" data-s="${id}" data-f="modifier_scale" onchange="ctlStickValue(this)"><span class="muted small">${Math.round(Number(s.modifier_scale??0.5)*100)} %</span></div>`}</div>`};
  const mot=(name,title)=>`<div class="bind"><span class="bl">${title}</span><button class="bv" data-m="${name}" onclick="ctlCapture(this)">${bindHtml(c.motions[name])}</button><button class="bm" data-m="${name}" onclick="ctlMenu(this)" aria-label="Options">⋯</button></div>`;
  const mid=(keys)=>{const rows=keys.filter(k=>k[2]).map(k=>b(k[0],k[1],1,k[3]));return rows.length?`<div class="grp ctlg"><h3>Boutons du milieu</h3>${rows.join('')}</div>`:''};
  const colL=`${L.left?`<div class="grp ctlg"><h3>${gc?'L':'L et ZL'}</h3>${b(gc?'L':'ZL','button_zl')}${thr('button_zl')}${b('L','button_l',1,gc)}</div>`:''}
   ${L.lstick?stick('lstick',gc?'Stick de contrôle':'Stick gauche','button_lstick',!gc):''}
   ${L.dpad?`<div class="grp ctlg"><h3>Croix</h3>${b('Haut','button_dup')}${b('Bas','button_ddown')}${b('Gauche','button_dleft')}${b('Droite','button_dright')}</div>`:''}
   ${L.slsrl?`<div class="grp ctlg"><h3>SL / SR</h3>${b('SL','button_slleft')}${b('SR','button_srleft')}</div>`:''}`;
  const colR=`${L.right?`<div class="grp ctlg"><h3>${gc?'R et Z':'R et ZR'}</h3>${b(gc?'R':'ZR','button_zr')}${thr('button_zr')}${b(gc?'Z':'R','button_r')}</div>`:''}
   ${L.face?`<div class="grp ctlg"><h3>Boutons</h3>${b('A','button_a')}${b('B','button_b')}${b('X','button_x')}${b('Y','button_y')}</div>`:''}
   ${L.rstick?stick('rstick',gc?'Stick C':'Stick droit','button_rstick',!gc):''}
   ${L.slsrr?`<div class="grp ctlg"><h3>SL / SR</h3>${b('SL','button_slright')}${b('SR','button_srright')}</div>`:''}`;
  const under=`${mid([['Moins','button_minus',L.minus],['Capture','button_screenshot',L.capture],[gc?'Start / Pause':'Plus','button_plus',L.plus],['Home','button_home',L.home,gc]])}${L.motl||L.motr?`<div class="grp ctlg"><h3>Gyroscope</h3>${L.motl?mot('motionleft',L.motr?'Gauche':'Gyroscope'):''}${L.motr?mot('motionright',L.motl?'Droite':'Gyroscope'):''}</div>`:''}`;
  const center=`<div class="ctlcenter"><div class="padstage"><div class="padart">${padSvg(type)}</div><p class="muted small" style="text-align:center">Clique sur une touche du dessin ou de la liste pour la régler.</p></div>${under}</div>`;
  const bottom=`<div class="card" style="margin-top:14px"><div class="row wrap between" style="gap:14px"><div class="row wrap" style="gap:14px;align-items:center"><label class="check"><input type="checkbox" ${T.vibration?'checked':''} onchange="ctlVibration(this.checked)"> <span>Vibrations</span></label><span class="muted small">Force</span><input type="range" min="0" max="100" value="${T.vibrationStrength}" onchange="ctlVibrationStrength(this.value)"><span class="muted small">${T.vibrationStrength} %</span></div><div class="row" style="gap:8px"><button class="btn" onclick="ctlDefaults()">Valeurs par défaut</button><button class="btn" onclick="ctlClear()">Tout effacer</button></div></div></div>`;
  // A single Joy-Con has controls on one side only: no empty column.
  const hasL=colL.trim()!=='',hasR=colR.trim()!=='';
  return top+`<div class="ctlpad${hasL&&hasR?'':' two'}">${hasL?`<div class="ctlside">${colL}</div>`:''}${center}${hasR?`<div class="ctlside">${colR}</div>`:''}</div>`+bottom}

function ctlAdvanced(){const a=CTL.advanced;const row=(k,t,d)=>`<div class="set"><div class="l"><b>${t}</b>${d?`<p class="muted small">${d}</p>`:''}</div><div class="v"><input type="checkbox" ${a[k]?'checked':''} onchange="ctlAdv('${k}',this.checked)"></div></div>`;
  return`<div class="card"><h2>Console</h2><div class="set"><div class="l"><b>Mode de la console</b><p class="muted small">Le mode portable va avec le type de manette « Portable » du joueur 1.</p></div><div class="v"><div class="seg small"><button class="${CTL.docked?'on':''}" onclick="ctlDocked(true)">TV</button><button class="${CTL.docked?'':'on'}" onclick="ctlDocked(false)">Portable</button></div></div></div></div>
  <div class="card" style="margin-top:14px"><h2>Pour tous les joueurs</h2>${row('vibration_enabled','Vibrations')}${row('enable_accurate_vibrations','Vibrations précises','Plus fidèles, mais certaines manettes vibrent moins bien.')}${row('motion_enabled','Gyroscope')}${row('controller_navigation','Naviguer dans les menus de l\'émulateur à la manette')}</div>
  <div class="card" style="margin-top:14px"><h2>Pilotes</h2><p class="muted small">À changer seulement si une manette n'est pas reconnue. Forkbay les applique aussi à sa propre lecture des manettes.</p>${row('enable_joycon_driver','Pilote Joy-Con de l\'émulateur','Gère lui-même les Joy-Con, avec le gyroscope.')}${row('enable_procon_driver','Pilote Pro Controller de l\'émulateur')}${row('enable_raw_input','Entrée brute (Raw Input)','Pour plus de 4 manettes Xbox sous Windows.')}</div>
  <div class="card" style="margin-top:14px"><h2>D'une manette à l'autre</h2><p class="muted small">Tes contrôles sont partagés par tous les émulateurs. Avant chaque partie, si la manette réglée n'est pas branchée mais qu'une du même modèle l'est, Forkbay lui passe tes contrôles : tu peux jouer avec l'une ou l'autre sans rien refaire.</p><p class="muted small">Un jeu peut avoir son propre profil par joueur, depuis l'engrenage sur sa jaquette.</p></div>`}

// --- Changes ---
async function ctlDo(cmd,args){const r=await act(cmd,args);if(r)await ctlReload();return r}
async function ctlReload(){try{CTL=await call('controls_get')}catch(e){}renderControls()}
const ctlIndex=()=>ctlPlayer().index;
function ctlSet(name,value){return ctlDo('controls_set',{player:ctlIndex(),name,value})}
function ctlConnected(on){ctlDo('controls_connected',{player:CTLTAB,on})}
function ctlType(v){ctlDo('controls_type',{player:CTLTAB,type:Number(v)})}
function ctlVibration(on){ctlDo('controls_vibration',{player:ctlIndex(),on})}
function ctlVibrationStrength(v){ctlDo('controls_vibration',{player:ctlIndex(),strength:Number(v)})}
function ctlAdv(key,on){ctlDo('controls_advanced',{key,on})}
function ctlDocked(on){ctlDo('controls_docked',{on})}
async function ctlDevice(v){if(!v)return;const [kind,guid,port,guid2]=v.split(':');const device=kind=='kb'?{kind:'keyboard'}:{kind:'pad',guid,port:Number(port),guid2};if(await ctlDo('controls_device',{player:ctlIndex(),device}))toast('Touches remplies.')}
function ctlDefaults(){const v=$('ctlDev').value;if(!v)return toast('Choisis d\'abord la manette utilisée.',true);ctlDevice(v)}
function ctlClear(){ctlDo('controls_clear',{player:ctlIndex()})}
function ctlThreshold(el){const p=pParse(ctlPlayer().controls.buttons[el.dataset.k]);p.threshold=fl(el.value/100);ctlSet(el.dataset.k,pText(p))}
function ctlStickValue(el){const s=pParse(ctlPlayer().controls.sticks[el.dataset.s]);if(!s.engine)return;s[el.dataset.f]=fl(el.value/100);ctlSet(el.dataset.s,pText(s))}

// --- Profiles ---
async function ctlProfileLoad(name){if(!name)return;if(await act('profile_load',{name,player:CTLTAB})){await ctlReload();const s=$('ctlProf');if(s)s.value=name;toast('Profil chargé.')}}
async function ctlProfileSave(){const n=$('ctlProf').value;if(!n)return ctlProfileNew();if(await act('profile_save',{name:n,player:CTLTAB,new:false}))toast('Profil enregistré.')}
function ctlProfileNew(){$('ctlProfMore').innerHTML=`<div class="row wrap" style="gap:8px;margin-top:12px"><input id="ctlProfName" maxlength="30" placeholder="Nom du profil" style="flex:1 1 200px" onkeydown="if(event.key=='Enter')ctlProfileCreate()"><button class="btn primary" onclick="ctlProfileCreate()">Créer</button><button class="btn" onclick="$('ctlProfMore').innerHTML=''">Annuler</button></div>`;$('ctlProfName').focus()}
async function ctlProfileCreate(){const name=$('ctlProfName').value.trim();if(!name)return;if(await act('profile_save',{name,player:CTLTAB,new:true})){await ctlReload();const s=$('ctlProf');if(s)s.value=name;toast('Profil créé.')}}
function ctlProfileDelete(){const n=$('ctlProf').value;if(!n)return toast('Choisis d\'abord un profil.',true);$('ctlProfMore').innerHTML=`<div class="row wrap" style="gap:8px;margin-top:12px;align-items:center"><span><span>Supprimer le profil</span> <b translate="no">${esc(n)}</b> ?</span><button class="btn" data-n="${attr(n)}" onclick="ctlProfileDrop(this.dataset.n)">Supprimer</button><button class="btn" onclick="$('ctlProfMore').innerHTML=''">Annuler</button></div>`}
async function ctlProfileDrop(name){if(await act('profile_delete',{name})){await ctlReload();toast('Profil supprimé.')}}

// --- Waiting for a control (HandleClick and SetPollingResult) ---
function ctlTarget(el){return el.dataset.s?{stick:el.dataset.s,dir:el.dataset.d}:el.dataset.m?{motion:el.dataset.m}:{button:el.dataset.k}}
async function ctlCapture(el){if(CTLCAP)return;if(CTL.running)return toast('Ferme d\'abord l\'émulateur.',true);ctlCloseMenu();const t=ctlTarget(el);
  const kind=t.motion?'motion':t.stick&&t.dir!='modifier'?'stick':'button';const dev=$('ctlDev').value;
  const pads=dev!='kb',keys=dev==''||dev=='kb';const parts=dev.split(':');const args={kind};if(parts[0]=='pad'&&!parts[3]){args.guid=parts[1];args.port=Number(parts[2])}
  el.innerHTML='<span>'+(kind=='motion'?'Secoue la manette…':kind=='stick'?'Bouge le stick…':'Appuie…')+'</span>';el.classList.add('wait');document.querySelectorAll(t.stick?`#ctl .padart [data-s="${t.stick}"]`:t.button?`#ctl .padart [data-k="${t.button}"]`:'#ctl .none').forEach(x=>x.classList.add('wait'));
  if(kind=='stick')$('statusText').textContent='Pousse le stick vers la droite, puis vers le haut. Pour inverser les axes, commence par le haut.';
  CTLCAP={el,t,kind,keys,until:Date.now()+(kind=='button'?4000:6000)};window.PADHOLD=true;ctlLiveStop();
  if(pads){try{await call('controls_capture',args)}catch(e){return ctlEnd(null,e.message)}}
  ctlCapturePoll(pads)}
async function ctlCapturePoll(pads){while(CTLCAP){if(Date.now()>CTLCAP.until)return ctlEnd(null);if(pads){let r=null;try{r=await call('controls_poll')}catch(e){}if(r&&CTLCAP)return ctlEnd(r)}await new Promise(f=>setTimeout(f,30))}}
async function ctlEnd(result,err){const cap=CTLCAP;CTLCAP=null;window.PADHOLD=false;call('controls_stop').catch(()=>{});$('statusText').textContent=err||'Prêt.';if(!cap)return;
  if(!result){renderControls();ctlLiveStart();return}
  const c=ctlPlayer().controls,t=cap.t;
  if(t.button)await ctlSet(t.button,result);
  else if(t.motion)await ctlSet(t.motion,result);
  else{const s=pParse(c.sticks[t.stick]);const r=pParse(result);let out;
    if(t.dir=='modifier'){s.modifier=result;out=s}
    else if(r.axis_x!==undefined&&r.axis_y!==undefined)out=r;
    else{out=(!s.engine||s.axis_x!==undefined||s.axis_y!==undefined)?{engine:'analog_from_button'}:s;out[t.dir]=result}
    await ctlSet(t.stick,pText(out))}
  ctlLiveStart()}
function ctlCancel(){if(CTLCAP){CTLCAP=null;window.PADHOLD=false;call('controls_stop').catch(()=>{})}}
document.addEventListener('keydown',e=>{if(!CTLCAP)return;e.preventDefault();e.stopImmediatePropagation();if(e.key=='Escape')return ctlEnd(null);if(!CTLCAP.keys)return;const code=qtCode(e);if(!code)return;ctlEnd(pText({engine:'keyboard',code:String(code),toggle:'0'}))},true);
document.addEventListener('keydown',e=>{const c=qtCode(e);if(c)CTLKEYS.add(c)});
document.addEventListener('keyup',e=>{const c=qtCode(e);if(c)CTLKEYS.delete(c)});

// --- Options of a control (its context menu in the emulators) ---
function ctlCloseMenu(){if(CTLMENU){CTLMENU.remove();CTLMENU=null}}
function ctlMenu(el){if(CTLCAP)return;if(CTLMENU&&CTLMENU.dataset.for==ctlKeyOf(el))return ctlCloseMenu();ctlCloseMenu();const t=ctlTarget(el),c=ctlPlayer().controls;const items=[];
  const flag=(p,k)=>p[k]=='1'||p[k]=='true';
  if(t.button||t.motion){const name=t.button||t.motion,group=t.button?'buttons':'motions';const p=pParse(c[group][name]);items.push(['Effacer',()=>ctlSet(name,'[empty]')]);
    if(p.code!==undefined||p.button!==undefined||p.hat!==undefined){items.push([flag(p,'inverted')?'Ne plus inverser':'Inverser le bouton',()=>{p.inverted=flag(p,'inverted')?'0':'1';ctlSet(name,pText(p))}]);items.push([flag(p,'toggle')?'Ne plus basculer':'Bouton bascule',()=>{p.toggle=flag(p,'toggle')?'0':'1';ctlSet(name,pText(p))}]);items.push([flag(p,'turbo')?'Arrêter le turbo':'Turbo',()=>{p.turbo=flag(p,'turbo')?'0':'1';ctlSet(name,pText(p))}])}
    if(p.axis!==undefined){items.push(['Inverser l\'axe',()=>{p.invert=p.invert=='-'?'+':'-';ctlSet(name,pText(p))}]);items.push([flag(p,'inverted')?'Ne plus inverser':'Inverser le bouton',()=>{p.inverted=flag(p,'inverted')?'0':'1';ctlSet(name,pText(p))}]);items.push([flag(p,'toggle')?'Ne plus basculer':'Axe bascule',()=>{p.toggle=flag(p,'toggle')?'0':'1';ctlSet(name,pText(p))}])}
    if(p.motion!==undefined)items.push(['Seuil du gyroscope…',()=>ctlAskNumber(el,'Seuil (0 à 100)',Math.round(Number(p.threshold??0.007)*1000),v=>{p.threshold=fl(v/1000);ctlSet(name,pText(p))})])}
  else{const s=pParse(c.sticks[t.stick]);
    if(t.dir=='modifier'){const m=pParse(s.modifier);items.push(['Effacer',()=>{s.modifier='';ctlSet(t.stick,pText(s))}]);items.push([flag(m,'toggle')?'Ne plus basculer':'Bouton bascule',()=>{m.toggle=flag(m,'toggle')?'0':'1';s.modifier=pText(m);ctlSet(t.stick,pText(s))}]);items.push([flag(m,'inverted')?'Ne plus inverser':'Inverser le bouton',()=>{m.inverted=flag(m,'inverted')?'0':'1';s.modifier=pText(m);ctlSet(t.stick,pText(s))}])}
    else{items.push(['Effacer',()=>{if(s.engine!='analog_from_button')return ctlSet(t.stick,'[empty]');delete s[t.dir];ctlSet(t.stick,pText(s))}]);
      if(isPadStick(s))items.push(['Centrer le stick',()=>ctlCenter(t.stick)]);
      items.push(['Inverser l\'axe',()=>{const k=t.dir=='left'||t.dir=='right'?'invert_x':'invert_y';s[k]=s[k]=='-'?'+':'-';ctlSet(t.stick,pText(s))}])}}
  const m=document.createElement('div');CTLMENU=m;m.className='ctlmenu card';m.dataset.for=ctlKeyOf(el);m.innerHTML=items.map((it,i)=>`<button class="btn" data-i="${i}">${esc(it[0])}</button>`).join('');m.querySelectorAll('button').forEach(b=>b.onclick=()=>{const f=items[b.dataset.i][1];if(!/…$/.test(items[b.dataset.i][0]))ctlCloseMenu();f()});
  el.parentElement.after(m);const first=m.querySelector('button');if(first)first.focus()}
function ctlKeyOf(el){return[el.dataset.k,el.dataset.s,el.dataset.d,el.dataset.m].join('|')}
function ctlAskNumber(el,label,value,done){const m=CTLMENU;if(!m)return;m.innerHTML=`<span class="muted small">${label}</span><input type="number" min="0" max="100" value="${value}" style="width:90px"><button class="btn">OK</button>`;const i=m.querySelector('input');i.focus();m.querySelector('button').onclick=()=>{const v=Math.max(0,Math.min(100,Number(i.value)||0));ctlCloseMenu();done(v)}}
async function ctlCenter(id){const s=pParse(ctlPlayer().controls.sticks[id]);const st=CTLSTATES.find(x=>x.guid==s.guid&&x.port==Number(s.port||0));if(!st)return toast('Cette manette n\'est pas branchée.',true);
  // Core::HID::SanitizeStick undone, as "Center axis" does.
  const ox=Number(s.offset_x||0),oy=Number(s.offset_y||0);let x=st.axes[s.axis_x]||0,y=st.axes[s.axis_y]||0;if(Math.abs(ox)<0.5){x*=x>0?1+ox:1-ox;y*=y>0?1+oy:1-oy}s.offset_x=fl(-x+ox);s.offset_y=fl(-y+oy);ctlSet(id,pText(s))}

// --- Controls lighting up while pressed ---
function ctlLiveStart(){ctlLiveStop();CTLLIVE=setInterval(async()=>{if(!$('p-pad').classList.contains('on')||CTLCAP)return;try{CTLSTATES=await call('controls_states')}catch(e){CTLSTATES=[]}ctlRefreshLive()},70)}
function ctlLiveStop(){if(CTLLIVE){clearInterval(CTLLIVE);CTLLIVE=null}}
function ctlActive(t){const p=pParse(t);if(!p.engine)return false;let on=false;
  if(p.engine=='keyboard')on=CTLKEYS.has(Number(p.code));
  else if(p.engine=='sdl'){const st=CTLSTATES.find(x=>x.guid==p.guid&&x.port==Number(p.port||0));if(!st)return false;
    if(p.button!==undefined)on=!!st.buttons[p.button];
    else if(p.hat!==undefined)on=!!((st.hats[p.hat]||0)&({up:1,right:2,down:4,left:8}[p.direction]||0));
    else if(p.axis!==undefined){const v=(st.axes[p.axis]||0)*(p.invert=='-'?-1:1);on=v>Number(p.threshold??0.5)}}
  if(p.inverted=='1'||p.inverted=='true')on=!on;return on}
function ctlStickPos(t){const p=pParse(t);if(p.engine=='analog_from_button'){const d=k=>ctlActive(p[k])?1:0;return[d('right')-d('left'),d('up')-d('down')]}if(p.engine!='sdl')return null;const st=CTLSTATES.find(x=>x.guid==p.guid&&x.port==Number(p.port||0));if(!st)return null;let x=(st.axes[p.axis_x]||0)+Number(p.offset_x||0),y=-((st.axes[p.axis_y]||0))+Number(p.offset_y||0);if(p.invert_x=='-')x=-x;if(p.invert_y=='-')y=-y;return[x,y]}
function ctlRefreshLive(){const T=ctlPlayer();if(!T||CTLTAB=='adv')return;const c=T.controls;
  document.querySelectorAll('#ctl .bv').forEach(b=>{let on=false;if(b.dataset.k)on=ctlActive(c.buttons[b.dataset.k]);else if(b.dataset.s){const s=pParse(c.sticks[b.dataset.s]);if(b.dataset.d=='modifier')on=ctlActive(s.modifier);else{const pos=ctlStickPos(c.sticks[b.dataset.s]);if(pos){const[x,y]=pos;on={left:x<-0.5,right:x>0.5,up:y>0.5,down:y<-0.5}[b.dataset.d]}}}b.classList.toggle('live',!!on)});
  // The drawing: pressed parts light up, and the sticks move.
  document.querySelectorAll('#ctl .padart [data-k]').forEach(e=>e.classList.toggle('live',ctlActive(c.buttons[e.dataset.k])));
  document.querySelectorAll('#ctl .padart [data-s]').forEach(e=>{const pos=ctlStickPos(c.sticks[e.dataset.s])||[0,0];const cap=e.querySelector('.cap');if(cap)cap.setAttribute('transform',`translate(${Math.max(-1,Math.min(1,pos[0]))*9} ${-Math.max(-1,Math.min(1,pos[1]))*9})`);e.classList.toggle('live',Math.hypot(pos[0],pos[1])>0.5)})}

// --- Shortcuts, the same in every emulator (ConfigureHotkeys, ControllerShortcut) ---
let HK=null,HKCAP=null;
const HK_GROUPS=[['Pendant la partie',[['Continue/Pause Emulation','Pause et reprise'],['Change Docked Mode','Mode TV ou portable'],['Audio Mute/Unmute','Couper ou remettre le son'],['Audio Volume Down','Baisser le volume'],['Audio Volume Up','Monter le volume'],['Capture Screenshot','Capture d\'écran'],['Toggle Performance Overlay','Infos de performance']]],
  ['Vitesse',[['Toggle Framerate Limit','Limite de vitesse','Coupée, le jeu tourne aussi vite que le PC le permet.'],['Toggle Turbo Speed','Vitesse turbo'],['Toggle Slow Speed','Vitesse lente']]],
  ['Image',[['Fullscreen','Plein écran'],['Exit Fullscreen','Quitter le plein écran'],['Change Adapting Filter','Changer le filtre d\'image'],['Change GPU Mode','Changer la précision GPU']]],
  ['Jeu',[['Load/Remove Amiibo','Amiibo','Pas dans tous les émulateurs.'],['Restart Emulation','Redémarrer le jeu'],['Stop Emulation','Arrêter le jeu'],['Exit Eglantine','Quitter l\'émulateur']]]];
// The order the emulators write a combination in, and the control of player 1 behind each button.
const HK_ORDER=['Home','Screenshot','L','R','ZL','ZR','Plus','Minus','A','B','X','Y','Dpad_Up','Dpad_Down','Dpad_Left','Dpad_Right','Left_Stick','Right_Stick'];
const HK_CTL={A:'button_a',B:'button_b',X:'button_x',Y:'button_y',L:'button_l',R:'button_r',ZL:'button_zl',ZR:'button_zr',Dpad_Left:'button_dleft',Dpad_Right:'button_dright',Dpad_Up:'button_dup',Dpad_Down:'button_ddown',Left_Stick:'button_lstick',Right_Stick:'button_rstick',Minus:'button_minus',Plus:'button_plus',Home:'button_home',Screenshot:'button_screenshot'};
const HK_KEYS={Esc:'Échap',Del:'Suppr',Ins:'Inser',Space:'Espace',Return:'Entrée',Enter:'Entrée (pavé)',PgUp:'Page préc.',PgDown:'Page suiv.',Shift:'Maj',Left:'Gauche',Right:'Droite',Up:'Haut',Down:'Bas',Home:'Début',End:'Fin',Backspace:'Retour arrière'};
function hkParts(s){if(!s)return[];if(s=='+')return['+'];return s.endsWith('++')?[...s.slice(0,-2).split('+'),'+']:s.split('+')}
function hkPadName(b){const d={Dpad_Up:'↑',Dpad_Down:'↓',Dpad_Left:'←',Dpad_Right:'→'}[b];if(d)return`<span>Croix</span> <span translate="no">${d}</span>`;
  const t={Left_Stick:'Clic du stick gauche',Right_Stick:'Clic du stick droit',Screenshot:'Capture'}[b];if(t)return`<span>${t}</span>`;return`<span translate="no">${esc({Plus:'+',Minus:'−',Home:'HOME'}[b]||b)}</span>`}
function hkHtml(v,f){const ps=hkParts(v);if(!ps.length)return'<span>non réglé</span>';return ps.map(p=>f=='pad'?hkPadName(p):HK_KEYS[p]?`<span>${HK_KEYS[p]}</span>`:`<span translate="no">${esc(p)}</span>`).join(' <span class="muted" translate="no">+</span> ')}
async function hkLoad(){try{HK=await call('hotkeys_get')}catch(e){HK={error:e.message}}if(CTLTAB=='keys')renderControls()}
function ctlHotkeys(){if(!HK){hkLoad();return'<div class="card muted">Lecture des raccourcis…</div>'}if(HK.error)return`<div class="card">${esc(HK.error)}</div>`;
  const by=Object.fromEntries(HK.list.map(h=>[h.id,h]));
  const btn=(h,f)=>`<button class="hkb${HKCAP&&HKCAP.id==h.id&&HKCAP.f==f?' wait':''}${h[f]!=h[f+'Default']?' own':''}" data-id="${attr(h.id)}" data-f="${f}" onclick="hkCapture(this)">${HKCAP&&HKCAP.id==h.id&&HKCAP.f==f?`<span>${f=='pad'?'Appuie sur la combinaison…':'Appuie sur une touche…'}</span>`:hkHtml(h[f],f)}</button><button class="bm" title="Effacer" data-id="${attr(h.id)}" data-f="${f}" onclick="hkSet(this.dataset.id,this.dataset.f,'')" ${h[f]?'':'disabled'}>×</button>`;
  const row=([id,t,d])=>{const h=by[id];if(!h)return'';return`<div class="set hk"><div class="l"><b>${t}</b>${d?`<p class="muted small">${d}</p>`:''}</div><div class="v"><div class="hkc">${btn(h,'key')}</div><div class="hkc">${btn(h,'pad')}</div></div></div>`};
  const head='<div class="set hk hkhead"><div class="l"></div><div class="v"><div class="hkc muted small">Clavier</div><div class="hkc muted small">Manette</div></div></div>';
  return`<div class="card"><div class="row wrap" style="justify-content:space-between;gap:12px;align-items:flex-start"><div style="flex:1 1 320px"><h2 style="margin:0 0 6px">Raccourcis</h2><p class="muted small">Les mêmes dans tous les émulateurs. Les combinaisons de boutons marchent pendant une partie, avec la manette du joueur 1.</p><p class="muted small">Sur une manette Xbox, HOME est le bouton Xbox. Si la Game Bar de Windows s'ouvre, coupe son ouverture par ce bouton dans les paramètres de Windows.</p></div><button class="btn" onclick="hkReset()">Tout remettre par défaut</button></div></div>
  <div class="cfgCards hkcards">${HK_GROUPS.map(([g,rows],i)=>`<div class="card" style="margin-top:14px"><h2>${g}</h2>${head}${rows.map(row).join('')}</div>`).join('')}</div>`}
async function hkSet(id,field,value){if(await act('hotkeys_set',{id,field,value}))await hkLoad()}
async function hkReset(){if(await act('hotkeys_reset')){toast('Raccourcis remis par défaut.');await hkLoad()}}
async function hkCapture(el){if(CTL.running)return toast('Ferme d\'abord l\'émulateur.',true);if(HKCAP){const same=HKCAP.id==el.dataset.id&&HKCAP.f==el.dataset.f;hkStop();if(same)return}
  HKCAP={id:el.dataset.id,f:el.dataset.f,until:Date.now()+6000,held:[],seen:false};window.PADHOLD=true;ctlLiveStop();renderControls();
  if(HKCAP.f=='pad')hkPoll();else setTimeout(()=>{if(HKCAP&&Date.now()>HKCAP.until-100)hkStop()},6000)}
function hkStop(){HKCAP=null;window.PADHOLD=false;renderControls();ctlLiveStart()}
async function hkPoll(){const cap=HKCAP,c=CTL.players[0].controls.buttons;
  while(HKCAP===cap){if(Date.now()>cap.until)return hkStop();try{CTLSTATES=await call('controls_states')}catch(e){CTLSTATES=[]}if(HKCAP!==cap)return;
    const on=HK_ORDER.filter(b=>ctlActive(c[HK_CTL[b]]));
    if(on.length){cap.seen=true;for(const b of on)if(!cap.held.includes(b)&&cap.held.length<4)cap.held.push(b)}
    else if(cap.seen){const v=HK_ORDER.filter(b=>cap.held.includes(b)).join('+');HKCAP=null;window.PADHOLD=false;ctlLiveStart();return hkSet(cap.id,'pad',v)}
    await new Promise(f=>setTimeout(f,40))}}
function hkKeyName(e){const m={Escape:'Esc',Tab:'Tab',Backspace:'Backspace',Enter:e.location==3?'Enter':'Return',Insert:'Ins',Delete:'Del',Home:'Home',End:'End',PageUp:'PgUp',PageDown:'PgDown',ArrowLeft:'Left',ArrowUp:'Up',ArrowRight:'Right',ArrowDown:'Down',' ':'Space'};if(m[e.key])return m[e.key];if(/^F\d{1,2}$/.test(e.key))return e.key;if(e.key.length==1)return e.key.toUpperCase();return''}
document.addEventListener('keydown',e=>{if(!HKCAP||HKCAP.f!='key')return;e.preventDefault();e.stopImmediatePropagation();const k=hkKeyName(e);if(!k)return;
  const v=[e.ctrlKey&&'Ctrl',e.altKey&&'Alt',e.shiftKey&&'Shift',k].filter(Boolean).join('+');const cap=HKCAP;HKCAP=null;window.PADHOLD=false;ctlLiveStart();hkSet(cap.id,'key',v)},true);

// --- A game's own profiles (ConfigureInputPerGame) ---
async function loadGameCtl(){const el=$('gameCtl');if(!el)return;let g;try{g=await call('game_profiles',{file:IMGFILE})}catch(e){el.innerHTML='';return}
  const opts=cur=>`<option value="">Contrôles communs</option>${g.profiles.map(n=>`<option ${n==cur?'selected':''} translate="no">${esc(n)}</option>`).join('')}${cur&&!g.profiles.includes(cur)?`<option selected translate="no">${esc(cur)}</option>`:''}`;
  const row=i=>`<div class="set"><div class="l"><b><span>Joueur</span> ${i+1}</b></div><div class="v"><select data-p="${i}" onchange="setGameCtl(this)">${opts(g.players[i])}</select></div></div>`;
  el.innerHTML=`<div class="card" style="margin-top:14px"><h2>Manette pour ce jeu</h2><p class="muted small">Un profil donné à un joueur ne sert que pour ce jeu. Les profils se créent sur la page Manette.</p>${g.profiles.length||g.players.some(Boolean)?row(0)+`<details style="margin-top:6px"><summary class="muted small">Autres joueurs</summary>${[1,2,3,4,5,6,7].map(row).join('')}</details>`:'<p class="muted small">Aucun profil pour l\'instant.</p>'}</div>`}
async function setGameCtl(sel){if(await act('game_profile_set',{file:IMGFILE,player:Number(sel.dataset.p),name:sel.value})){toast(sel.value?'Profil donné à ce joueur pour ce jeu.':'Ce joueur suit de nouveau les contrôles communs.');refresh()}loadGameCtl()}

// --- The drawing of the controller ---
// Drawn here, one shape per kind of controller; each part is tied to the control it stands for.
function padSvg(type){const L=layout(type),gc=GC(type);let body='',parts='';
  const btn=(k,x,y,r,label,cls='')=>`<g class="part ${cls}" data-k="${k}"><circle cx="${x}" cy="${y}" r="${r}"/>${label?`<text x="${x}" y="${y}">${label}</text>`:''}</g>`;
  const pill=(k,x,y,w,h,label)=>`<g class="part" data-k="${k}"><rect x="${x}" y="${y}" width="${w}" height="${h}" rx="${Math.min(w,h)/2}"/>${label?`<text x="${x+w/2}" y="${y+h/2}">${label}</text>`:''}</g>`;
  const stk=(id,press,x,y)=>`<g class="part stick" data-s="${id}"><circle cx="${x}" cy="${y}" r="24" class="well"/><g class="cap" data-k="${press}"><circle cx="${x}" cy="${y}" r="15"/><circle cx="${x}" cy="${y}" r="9" class="dimple"/></g></g>`;
  const dpad=(x,y)=>[['button_dup',0,-15],['button_ddown',0,15],['button_dleft',-15,0],['button_dright',15,0]].map(([k,dx,dy])=>`<g class="part" data-k="${k}"><rect x="${x+dx-8}" y="${y+dy-8}" width="16" height="16" rx="4"/></g>`).join('')+`<rect x="${x-8}" y="${y-8}" width="16" height="16" class="hub"/>`;
  const face=(x,y)=>gc?btn('button_a',x,y+4,15,'A','big')+btn('button_b',x-25,y+20,9,'B')+btn('button_x',x+25,y-10,9,'X')+btn('button_y',x-6,y-23,9,'Y')
    :btn('button_x',x,y-22,11,'X')+btn('button_a',x+22,y,11,'A')+btn('button_b',x,y+22,11,'B')+btn('button_y',x-22,y,11,'Y');
  const joyL=x=>`<path class="shell left" d="M${x+70} 32 H${x+34} C${x+12} 32 ${x} 48 ${x} 72 V180 C${x} 204 ${x+12} 220 ${x+34} 220 H${x+70} Z"/>`;
  const joyR=x=>`<path class="shell right" d="M${x} 32 H${x+36} C${x+58} 32 ${x+70} 48 ${x+70} 72 V180 C${x+70} 204 ${x+58} 220 ${x+36} 220 H${x} Z"/>`;
  const leftParts=x=>pill('button_zl',x+4,8,52,12,gc?'L':'ZL')+pill('button_l',x+10,21,50,10,'L')+stk('lstick','button_lstick',x+38,78)+btn('button_minus',x+59,46,7,'−','small')+dpad(x+38,150)+(L.capture?pill('button_screenshot',x+44,190,14,14,''):'');
  const rightParts=x=>pill('button_zr',x+14,8,52,12,gc?'R':'ZR')+pill('button_r',x+10,21,50,10,'R')+btn('button_plus',x+11,46,7,'+','small')+face(x+32,82)+stk('rstick','button_rstick',x+32,150)+btn('button_home',x+24,196,9,'⌂','home');
  const rail=(k1,k2,x)=>pill(k1,x,62,9,40,'')+pill(k2,x,150,9,40,'');
  if(type==1||type==4){const hh=type==4;const lx=hh?40:135,rx=hh?310:215;
    body=(hh?`<rect class="screen" x="112" y="34" width="196" height="184" rx="8"/>`:'')+joyL(lx)+joyR(rx);parts=leftParts(lx)+rightParts(rx)}
  else if(type==2){body=joyL(175);parts=leftParts(175)+rail('button_slleft','button_srleft',247)}
  else if(type==3){body=joyR(175);parts=rightParts(175)+rail('button_slright','button_srright',164)}
  else{body=`<path class="shell" d="M92 40 C62 40 42 56 36 92 L20 178 C14 214 44 232 70 216 L112 188 H308 L350 216 C376 232 406 214 400 178 L384 92 C378 56 358 40 328 40 Z"/>`;
    parts=pill('button_zl',70,10,78,14,gc?'L':'ZL')+(gc?'':pill('button_l',78,27,72,12,'L'))+pill('button_zr',272,10,78,14,gc?'R':'ZR')+pill('button_r',270,27,72,12,gc?'Z':'R')
      +stk('lstick','button_lstick',120,94)+dpad(165,150)+face(298,94)+stk('rstick','button_rstick',255,150)
      +(L.minus?btn('button_minus',180,72,8,'−','small'):'')+btn('button_plus',gc?210:240,72,8,gc?'':'+','small')+(gc?'':btn('button_home',238,104,9,'⌂','home'))+(L.capture?pill('button_screenshot',176,96,14,14,''):'')}
  // Framed on the controller itself, so that a single Joy-Con is as large as a whole controller.
  const box={1:'118 0 184 236',4:'30 0 360 236',2:'160 0 110 236',3:'150 0 110 236'}[type]||'10 0 400 236';
  return`<svg viewBox="${box}" role="img" aria-label="Manette">${body}${parts}</svg>`}
function ctlPartClick(e){const part=e.target.closest('.padart .part');if(!part||CTLCAP)return;const k=part.dataset.k||(e.target.closest('.cap')||{dataset:{}}).dataset.k;
  // A stick: its whole axes are set from the "Right" control, as moving it fills both axes.
  const sel=part.dataset.s?`#ctl .bv[data-s="${part.dataset.s}"][data-d="right"]`:`#ctl .bv[data-k="${part.dataset.k}"]`;const target=document.querySelector(sel);if(target)ctlCapture(target)}
function ctlHover(e,on){const t=e.target.closest('#ctl .bind,#ctl .padart .part');if(!t)return;let key='';
  if(t.classList.contains('part'))key=t.dataset.s?'s:'+t.dataset.s:'k:'+t.dataset.k;else{const v=t.querySelector('.bv');if(!v)return;key=v.dataset.s?'s:'+v.dataset.s:v.dataset.k?'k:'+v.dataset.k:''}
  document.querySelectorAll('#ctl .hl').forEach(x=>x.classList.remove('hl'));if(!on||!key)return;const[kind,name]=key.split(':');
  document.querySelectorAll(kind=='s'?`#ctl [data-s="${name}"]`:`#ctl [data-k="${name}"]`).forEach(x=>(x.closest('.bind')||x).classList.add('hl'))}
document.addEventListener('click',e=>{if(e.target.closest&&e.target.closest('#ctl .padart'))ctlPartClick(e)});
document.addEventListener('mouseover',e=>{if(e.target.closest&&e.target.closest('#ctl'))ctlHover(e,true)});
document.addEventListener('focusin',e=>{if(e.target.closest&&e.target.closest('#ctl .bind'))ctlHover(e,true)});
