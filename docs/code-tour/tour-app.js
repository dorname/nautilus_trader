
const TOUR = __TOUR__;
const VIEWS = [
  {id:"overview",label:"Overview"},
  {id:"architecture",label:"Architecture"},
  {id:"flow",label:"Execution Flow"},
  {id:"modules",label:"Module Explorer"},
  {id:"reading",label:"Reading Path"},
  {id:"library",label:"Flow Library"}
];
const state = {
  view:"overview", selected:null, selectedKind:null,
  flowId:"backtest-tick", moduleId:"nautilus-system", readingStep:0,
  history:[], archFocus:null, graphMode:"spine",
  graphZoom:{overview:1,architecture:1,flow:1}
};
const $ = id => document.getElementById(id);
const esc = s => String(s??"").replace(/[&<>"']/g, c => ({"&":"&amp;","<":"&lt;",">":"&gt;","\"":"&quot;","'":"&#39;"}[c]));
const layerOf = r => ({protocol:"protocol",runtime:"runtime",storage:"storage",extension:"extension",host:"host",external:"external"}[r]||"external");
const findOverview = id => TOUR.overviewNodes.find(n => n.id===id);
const findModule = id => TOUR.modules.find(m => m.id===id);
const currentFlow = () => TOUR.flows[state.flowId];

function pushHistory(){
  state.history.push({view:state.view,selected:state.selected,selectedKind:state.selectedKind,flowId:state.flowId,moduleId:state.moduleId,readingStep:state.readingStep,archFocus:state.archFocus});
  if(state.history.length>50) state.history.shift();
}
function goBack(){
  const p = state.history.pop();
  if(!p) return;
  Object.assign(state,p);
  render();
}
function select(kind,id,{push=true}={}){
  if(push) pushHistory();
  state.selectedKind = kind;
  state.selected = id;
  if(kind==="module") state.moduleId = id;
  renderDetail();
  highlight();
}

function renderTabs(){
  $("tabs").innerHTML = VIEWS.map(v=>`<button class="tab ${state.view===v.id?"active":""}" data-view="${v.id}">${v.label}</button>`).join("");
}
function render(){
  renderTabs();
  const main = $("main");
  if(state.view==="overview") main.innerHTML = viewOverview();
  else if(state.view==="architecture") main.innerHTML = viewArch();
  else if(state.view==="flow"||state.view==="library") main.innerHTML = viewFlow(state.view==="library");
  else if(state.view==="modules") main.innerHTML = viewModules();
  else if(state.view==="reading") main.innerHTML = viewReading();
  bind();
  renderDetail();
  highlight();
}

function legend(){
  return `<div class="legend"><span><i class="dot protocol"></i>Protocol</span><span><i class="dot runtime"></i>Runtime</span><span><i class="dot storage"></i>Storage</span><span><i class="dot extension"></i>Extension</span><span><i class="dot host"></i>Host</span></div>`;
}
function arrowDef(){
  return `<defs>
    <marker id="arrow" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse"><path d="M0 0 L10 5 L0 10 z" fill="context-stroke"/></marker>
    <filter id="nodeShadow" x="-20%" y="-30%" width="140%" height="160%"><feDropShadow dx="0" dy="3" stdDeviation="4" flood-color="#000" flood-opacity=".28"/></filter>
  </defs>`;
}
function graphToolbar(graph, {modes=false}={}){
  const modeButtons = modes ? `<div class="graph-modes" aria-label="依赖图显示范围">
    <button class="graph-control ${state.graphMode==="spine"?"active":""}" data-graph-mode="spine">核心主链</button>
    <button class="graph-control ${state.graphMode==="full"?"active":""}" data-graph-mode="full">完整依赖</button>
  </div>` : "";
  return `<div class="graph-toolbar">
    ${modeButtons}
    <div class="graph-controls" aria-label="图缩放">
      <button class="graph-control" data-graph="${graph}" data-zoom="out" title="缩小">−</button>
      <button class="graph-control zoom-value" data-graph="${graph}" data-zoom="reset" title="重置缩放">${Math.round((state.graphZoom[graph]||1)*100)}%</button>
      <button class="graph-control" data-graph="${graph}" data-zoom="in" title="放大">＋</button>
    </div>
  </div>`;
}
function graphFrame(graph, svg, options={}){
  const zoom = state.graphZoom[graph] || 1;
  return `${graphToolbar(graph, options)}
    <div class="canvas graph-viewport" data-graph-canvas="${graph}">
      <div class="graph-stage" style="width:${zoom*100}%">${svg}</div>
    </div>`;
}
function lane(y,h,label,kind){
  return `<g class="arch-lane arch-lane-${kind}">
    <rect x="10" y="${y}" width="1040" height="${h}" rx="12"/>
    <text x="28" y="${y+25}" class="lane-label">${label}</text>
  </g>`;
}
function nodeSvg(n,x,y,{width=164,subtitle="",planned=false}={}){
  const role = layerOf(n.layer || n.role);
  const isModule = String(n.id||"").startsWith("nautilus-");
  return `<g class="node-hit ${planned?"planned":""}" data-kind="${isModule?"module":"overview"}" data-id="${n.id}" transform="translate(${x},${y})">
    <rect class="node-box" x="${-width/2}" y="-25" width="${width}" height="50" rx="9" fill="var(--elev)" stroke="var(--line2)" filter="url(#nodeShadow)"/>
    <rect x="${-width/2}" y="-25" width="4" height="50" rx="2" fill="var(--${role})"/>
    <text x="${-width/2+15}" y="${subtitle?-4:4}" fill="var(--text)" font-size="11.5" font-family="${isModule?"var(--mono)":"var(--sans)"}">${esc(n.label||n.id)}</text>
    ${subtitle?`<text x="${-width/2+15}" y="13" fill="var(--dim)" font-size="9.5">${esc(subtitle)}</text>`:""}
  </g>`;
}

function viewOverview(){
  const layout = {
    venue:[220,52], catalog:[520,52],
    "host-python":[220,145], "host-research":[520,145], "research-worker":[820,145],
    kernel:[530,245],
    "data-engine":[160,360], trader:[360,360], "risk-engine":[560,360], "exec-engine":[760,360], portfolio:[960,360],
    msgbus:[300,470], cache:[530,470], "event-store":[760,470],
    backtest:[280,580], live:[530,580], adapters:[780,580], network:[960,580]
  };
  const subtitles = {
    venue:"REST / WebSocket venues",
    catalog:"Parquet history",
    "host-python":"PyO3 control plane",
    "host-research":"研序 Desktop / CLI",
    "research-worker":"isolated backtest job",
    kernel:"NautilusKernel 六组件",
    "data-engine":"subscribe · publish",
    trader:"Strategy / Actor",
    "risk-engine":"pre-trade checks",
    "exec-engine":"route · fill events",
    portfolio:"PnL · exposure",
    msgbus:"pub/sub · command",
    cache:"orders · positions",
    "event-store":"crash-only replay",
    backtest:"deterministic clock",
    live:"real-time node",
    adapters:"Data/Exec clients",
    network:"HTTP / WS base"
  };
  const nodes = TOUR.overviewNodes.filter(n=>layout[n.id]).map(n=>{
    const [x,y]=layout[n.id];
    const wide = ["kernel","host-research","research-worker"].includes(n.id);
    return nodeSvg(n,x,y,{width:wide?190:168,subtitle:subtitles[n.id]||""});
  }).join("");
  const main = new Set([
    "catalog>data-engine","data-engine>msgbus","msgbus>trader","trader>risk-engine",
    "risk-engine>exec-engine","exec-engine>adapters","kernel>data-engine","kernel>trader",
    "backtest>kernel","host-research>research-worker","research-worker>backtest"
  ]);
  const orchestrates = new Set([
    "host-python>kernel","host-python>backtest","host-python>live","live>kernel"
  ]);
  const edges = TOUR.overviewEdges.map(([a,b])=>{
    if(!layout[a]||!layout[b]) return "";
    const [x1,y1]=layout[a],[x2,y2]=layout[b];
    const key=`${a}>${b}`;
    const cls = main.has(key)?"edge edge-main":orchestrates.has(key)?"edge edge-orchestration":"edge edge-support";
    const fromY = y2>y1 ? y1+25 : (y2<y1 ? y1-25 : y1);
    const toY = y2>y1 ? y2-25 : (y2<y1 ? y2+25 : y2);
    const fromX = y1===y2 ? (x2>x1?x1+84:x1-84) : x1;
    const toX = y1===y2 ? (x2>x1?x2-84:x2+84) : x2;
    return `<path class="${cls}" data-from="${a}" data-to="${b}" d="M${fromX},${fromY} C${fromX},${(y1+y2)/2} ${toX},${(y1+y2)/2} ${toX},${toY}"/>`;
  }).join("");
  const lanes = [
    lane(14,70,"01 · EXTERNAL","external"),
    lane(100,80,"02 · HOSTS","host"),
    lane(200,80,"03 · KERNEL","runtime"),
    lane(300,100,"04 · ENGINES","runtime"),
    lane(420,90,"05 · BUS / STATE","storage"),
    lane(530,100,"06 · ENVIRONMENTS / ADAPTERS","extension")
  ].join("");
  const svg = `<svg class="graph-svg" viewBox="0 0 1100 660" width="100%" role="img" aria-label="NautilusTrader 分层系统架构图">${arrowDef()}${lanes}${edges}${nodes}</svg>`;
  return `<section><h2 class="view-title">NautilusTrader 整体架构</h2>
    <p class="view-lede">先沿金色主链理解「行情 → 策略 → 风控 → 执行」，再看蓝色支撑与橙色宿主编排。泳道是逻辑边界，不是微服务拆分。</p>
    ${legend()}
    <div class="edge-legend">
      <span><i class="line-sample line-main"></i>主调用链</span>
      <span><i class="line-sample line-support"></i>运行时支撑</span>
      <span><i class="line-sample line-orchestration"></i>宿主编排入口</span>
      <span><i class="line-sample line-planned"></i>计划中 / scaffold</span>
    </div>
    ${graphFrame("overview",svg)}
    <p class="view-lede" style="margin-top:12px">同一 <code>NautilusKernel</code> 支撑回测与实盘；本仓库另有 A股「研序」Desktop/CLI，经 worker 子进程复用 <code>BacktestEngine</code>，GUI 不直连交易执行通道。</p></section>`;
}

function viewArch(){
  const order = [
    "nautilus-core","nautilus-model","nautilus-serialization","nautilus-common",
    "nautilus-data","nautilus-execution","nautilus-portfolio","nautilus-analysis",
    "nautilus-risk","nautilus-trading","nautilus-system","nautilus-persistence",
    "nautilus-backtest","nautilus-network","nautilus-live","nautilus-event-store",
    "nautilus-pyo3","nautilus-research-domain","nautilus-research-worker",
    "nautilus-research-desktop","nautilus-research-cli"
  ];
  const list = order.map(findModule).filter(Boolean);
  const rows = [
    {label:"FOUNDATION",kind:"protocol",ids:["nautilus-core","nautilus-model","nautilus-serialization"]},
    {label:"RUNTIME CORE",kind:"runtime",ids:["nautilus-common","nautilus-data","nautilus-execution","nautilus-portfolio"]},
    {label:"ENGINES / APP",kind:"runtime",ids:["nautilus-risk","nautilus-trading","nautilus-system","nautilus-analysis"]},
    {label:"ENVIRONMENTS",kind:"extension",ids:["nautilus-backtest","nautilus-live","nautilus-network","nautilus-persistence","nautilus-event-store"]},
    {label:"HOSTS",kind:"host",ids:["nautilus-pyo3","nautilus-research-domain","nautilus-research-worker","nautilus-research-desktop","nautilus-research-cli"]}
  ];
  const pos = {};
  rows.forEach((row,ri)=>{
    const y = 57 + ri*106;
    row.ids.forEach((id,ci)=>{ pos[id] = [120 + (ci+0.5)*(900/Math.max(row.ids.length,1)), y]; });
  });
  const allEdges = [];
  list.forEach(m => (m.deps||[]).forEach(d => { if(pos[d]&&pos[m.id]) allEdges.push([d,m.id]); }));
  const spineKeys = new Set([
    "nautilus-core>nautilus-model","nautilus-model>nautilus-common","nautilus-common>nautilus-data",
    "nautilus-common>nautilus-execution","nautilus-common>nautilus-portfolio",
    "nautilus-execution>nautilus-risk","nautilus-portfolio>nautilus-risk",
    "nautilus-risk>nautilus-system","nautilus-trading>nautilus-system","nautilus-data>nautilus-system",
    "nautilus-system>nautilus-backtest","nautilus-network>nautilus-live",
    "nautilus-backtest>nautilus-research-worker","nautilus-research-domain>nautilus-research-desktop",
    "nautilus-research-worker>nautilus-research-desktop","nautilus-system>nautilus-pyo3",
    "nautilus-backtest>nautilus-pyo3"
  ]);
  const selectedModule = state.selectedKind==="module" ? state.selected : null;
  const visibleEdges = state.graphMode==="full"
    ? allEdges
    : allEdges.filter(([a,b])=>spineKeys.has(`${a}>${b}`) || a===selectedModule || b===selectedModule);
  const edgeSvg = visibleEdges.map(([a,b],i)=>{
    const [x1,y1]=pos[a],[x2,y2]=pos[b];
    const selected = state.selectedKind==="module" ? state.selected : null;
    const relation = b===selected?"dependency":a===selected?"dependent":"neutral";
    const bend = (i%5-2)*8;
    return `<path class="edge dep-edge edge-${relation}" data-from="${a}" data-to="${b}" data-relation="${relation}" d="M${x1},${y1+25} C${x1+bend},${(y1+y2)/2} ${x2+bend},${(y1+y2)/2} ${x2},${y2-25}"/>`;
  }).join("");
  const nodesSvg = list.map(m=>{
    const [x,y]=pos[m.id];
    const short = m.id.replace(/^nautilus-/,"");
    const subtitle = `${(m.deps||[]).length} deps · ${(m.dependents||[]).length} used by`;
    return nodeSvg({...m,label:short},x,y,{width:168,subtitle});
  }).join("");
  const laneSvg = rows.map((row,ri)=>lane(14+ri*106,86,row.label,row.kind)).join("");
  const svg = `<svg class="graph-svg" viewBox="0 0 1140 560" width="100%" role="img" aria-label="NautilusTrader 核心 crate Cargo 依赖图">${arrowDef()}${laneSvg}${edgeSvg}${nodesSvg}</svg>`;
  return `<section><h2 class="view-title">核心 Crate 依赖</h2>
    <p class="view-lede">箭头从依赖指向使用者：A → B 表示 B 的 Cargo.toml 直接依赖 A。默认仅显示理解内核所需的核心主链；完整依赖保留图中模块的全部直接边。</p>
    ${legend()}
    <div class="edge-legend">
      <span><i class="line-sample line-dependency"></i>选中节点的 dependencies</span>
      <span><i class="line-sample line-dependent"></i>选中节点的 dependents</span>
      <span><i class="line-sample line-neutral"></i>Cargo 直接依赖</span>
    </div>
    ${graphFrame("architecture",svg,{modes:true})}
    <p class="view-lede graph-hint">选择 crate 后：绿色边流入选中节点（它依赖谁），金色边流出选中节点（谁依赖它）。</p>
  </section>`;
}

function viewFlow(showLib){
  const flow = currentFlow();
  const chips = Object.values(TOUR.flows).map(f=>`<button class="flow-chip ${f.id===state.flowId?"active":""}" data-flow="${f.id}">${esc(f.title)} <span style="color:var(--dim)">${f.priority}</span></button>`).join("");
  const gap=56, x=240, startY=36;
  const pos={}; flow.nodes.forEach((n,i)=>pos[n.id]=[x, startY+i*gap]);
  const edgeSvg = flow.edges.map(([a,b])=>{
    const [x1,y1]=pos[a],[x2,y2]=pos[b];
    const ia=flow.nodes.findIndex(n=>n.id===a), ib=flow.nodes.findIndex(n=>n.id===b);
    if(ib<ia) return `<path class="edge" data-from="${a}" data-to="${b}" d="M${x1+78},${y1} C${x1+170},${y1} ${x2+170},${y2} ${x2+78},${y2}"/>`;
    return `<path class="edge" data-from="${a}" data-to="${b}" d="M${x1},${y1+16} L${x2},${y2-16}"/>`;
  }).join("");
  const nodeSvg = flow.nodes.map(n=>{
    const [px,py]=pos[n.id];
    return `<g class="node-hit" data-kind="flow-node" data-id="${n.id}" transform="translate(${px},${py})">
      <rect class="node-box" x="-170" y="-20" width="340" height="40" rx="8" fill="var(--elev)" stroke="var(--line2)"/>
      <text x="-158" y="-1" fill="var(--text)" font-size="12">${esc(n.label)}</text>
      <text x="-158" y="13" fill="var(--dim)" font-size="10" font-family="var(--mono)">${esc(n.crate)} · ${esc((n.symbol||"").slice(0,40))}</text></g>`;
  }).join("");
  const h = startY + flow.nodes.length*gap + 30;
  return `<section><h2 class="view-title">${showLib?"Flow Library":"Execution Flow"} — ${esc(flow.title)}</h2>
    <p class="view-lede">${esc(flow.summary)}</p>
    <div class="flow-bar">${chips}</div>
    ${graphFrame("flow",`<svg class="graph-svg" viewBox="0 0 560 ${h}" width="100%">${arrowDef()}${edgeSvg}${nodeSvg}</svg>`)}</section>`;
}

function relatedFlows(mid){
  const short = mid.replace(/^nautilus-/,"").replace(/-/g,"_");
  return Object.values(TOUR.flows).filter(f => f.nodes.some(n =>
    n.crate===mid ||
    (n.file||"").includes(mid.replace(/^nautilus-/,"")) ||
    (n.file||"").includes(short) ||
    (n.file||"").includes(mid)
  ));
}
function modulePanel(m){
  return `<h3>${esc(m.id)} <span class="badge">${esc(m.role)}</span></h3>
    <p class="role">${esc(m.responsibility)}</p>
    <div class="kv"><div class="label">Key symbols</div><div class="chip-row">${(m.keySymbols||[]).map(s=>`<span class="chip">${esc(s)}</span>`).join("")||"—"}</div></div>
    <div class="kv"><div class="label">Important files</div><div>${(m.keyFiles||[]).map(f=>`<button class="file-link" data-file="${esc(f)}">${esc(f)}</button>`).join("")}</div></div>
    <div class="kv"><div class="label">Depends on</div><div class="chip-row">${(m.deps||[]).map(d=>`<button class="chip" data-kind="module" data-id="${d}">${esc(d)}</button>`).join("")||"—"}</div></div>
    <div class="kv"><div class="label">Used by</div><div class="chip-row">${(m.dependents||[]).map(d=>`<button class="chip" data-kind="module" data-id="${d}">${esc(d)}</button>`).join("")||"—"}</div></div>
    <div class="kv"><div class="label">Related flows</div><div class="chip-row">${relatedFlows(m.id).map(f=>`<button class="chip" data-go-flow="${f.id}">${esc(f.title)}</button>`).join("")||"—"}</div></div>
    <div class="kv"><div class="label">Evidence</div><div class="evidence">${esc(m.evidence)}</div></div>`;
}
function viewModules(){
  const m = findModule(state.moduleId)||TOUR.modules[0];
  const tree = TOUR.modules.map(mod=>{
    const subs=(mod.modules||[]).map(s=>`<button class="tree-item mod" data-kind="module" data-id="${mod.id}">${esc(s)}</button>`).join("");
    return `<button class="tree-item crate ${mod.id===m.id?"active":""}" data-kind="module" data-id="${mod.id}">${esc(mod.id)}</button>${subs}`;
  }).join("");
  return `<section><h2 class="view-title">Module Explorer</h2>
    <p class="view-lede">左侧为 crate / 子模块（来自源码目录与 lib 导出）。</p>
    <div class="explorer"><div class="tree">${tree}</div><div class="mod-panel" id="modPanel">${modulePanel(m)}</div></div></section>`;
}
function viewReading(){
  const html = TOUR.readingPath.map((s,i)=>{
    const active = i===state.readingStep?"active":"";
    const line = i<TOUR.readingPath.length-1?`<div class="path-line"></div>`:"";
    return `<div class="path-step ${active}" data-rp="${i}">
      <div class="path-rail"><div class="path-num">${String(i+1).padStart(2,"0")}</div>${line}</div>
      <div class="path-card"><h3>${esc(s.crate)}</h3><p class="why">${esc(s.why)}</p>
        <div class="path-grid">
          <div class="path-box"><h4>What to read</h4><ul>${s.whatToRead.map(x=>`<li>${esc(x)}</li>`).join("")}</ul></div>
          <div class="path-box"><h4>What to understand</h4><ul>${s.whatToUnderstand.map(x=>`<li>${esc(x)}</li>`).join("")}</ul></div>
          <div class="path-box"><h4>Why now</h4><p style="margin:0;color:var(--muted);font-size:12.5px">${esc(s.why)}</p></div>
          <div class="path-box"><h4>Skip for now</h4><ul>${s.skip.map(x=>`<li>${esc(x)}</li>`).join("")}</ul></div>
        </div>
      </div></div>`;
  }).join("");
  return `<section><h2 class="view-title">Reading Path</h2>
    <p class="view-lede">按依赖与执行主链排序的学习路线。控制阅读范围，而不是堆文件列表。</p>
    <div class="path-steps">${html}</div></section>`;
}

function renderDetail(){
  const el = $("detail");
  if(!state.selected){
    el.innerHTML = `<div class="detail-empty">点击图中的节点，查看职责、文件、符号与证据。<br/><br/>建议路径：Overview → Execution Flow（回测主循环）→ Module Explorer → Reading Path。</div>`;
    return;
  }
  if(state.selectedKind==="overview"){
    const n = findOverview(state.selected); if(!n) return;
    el.innerHTML = detailOverview(n);
  } else if(state.selectedKind==="module"){
    const m = findModule(state.selected); if(!m) return;
    el.innerHTML = detailModule(m);
  } else if(state.selectedKind==="flow-node"){
    const n = currentFlow().nodes.find(x=>x.id===state.selected); if(!n) return;
    el.innerHTML = detailFlowNode(n);
  } else if(state.selectedKind==="file"){
    el.innerHTML = `<div class="detail"><h3 class="mono">${esc(state.selected)}</h3><p class="role">源码文件路径（相对仓库根）。在 IDE 中打开以继续阅读。</p>
      <div class="actions"><button class="btn" data-go-flow="backtest-tick">查看回测主循环</button></div></div>`;
  }
}
function detailOverview(n){
  return `<div class="detail"><h3>${esc(n.label)}</h3><p class="role">${esc(n.responsibility)}</p>
    <section><h4>Crates</h4><div class="chip-row">${(n.crates||[]).map(c=>`<button class="chip" data-kind="module" data-id="${c}">${esc(c)}</button>`).join("")||"—"}</div></section>
    <section><h4>Key files</h4><div>${(n.keyFiles||[]).map(f=>`<button class="file-link" data-file="${esc(f)}">${esc(f)}</button>`).join("")||"—"}</div></section>
    <section><h4>Key symbols</h4><div class="chip-row">${(n.keySymbols||[]).map(s=>`<span class="chip">${esc(s)}</span>`).join("")||"—"}</div></section>
    <section><h4>Depends on / Used by</h4><div class="chip-row">${[...(n.dependsOn||[]).map(id=>`<button class="chip" data-kind="overview" data-id="${id}">↓ ${esc(id)}</button>`),...(n.usedBy||[]).map(id=>`<button class="chip" data-kind="overview" data-id="${id}">↑ ${esc(id)}</button>`)].join("")||"—"}</div></section>
    <section><h4>Reading priority</h4><div>${n.readingPriority??"—"}</div></section>
    <section><h4>Evidence</h4><div class="evidence">${esc(n.evidence)}</div></section>
    <div class="actions"><button class="btn" data-go-flow="backtest-tick">打开回测主循环</button>
      ${(n.crates||[])[0]?`<button class="btn" data-kind="module" data-id="${n.crates[0]}">进入 Module</button>`:""}</div></div>`;
}
function detailModule(m){
  return `<div class="detail"><h3>${esc(m.id)}</h3><p class="role">${esc(m.responsibility)}</p>
    <section><h4>Entry points</h4><div class="chip-row">${(m.entryPoints||[]).map(s=>`<span class="chip">${esc(s)}</span>`).join("")||"—"}</div></section>
    <section><h4>Key symbols</h4><div class="chip-row">${(m.keySymbols||[]).map(s=>`<span class="chip">${esc(s)}</span>`).join("")||"—"}</div></section>
    <section><h4>Files</h4><div>${(m.keyFiles||[]).map(f=>`<button class="file-link" data-file="${esc(f)}">${esc(f)}</button>`).join("")}</div></section>
    <section><h4>Dependencies</h4><div class="chip-row">${(m.deps||[]).map(d=>`<button class="chip" data-kind="module" data-id="${d}">${esc(d)}</button>`).join("")||"—"}</div></section>
    <section><h4>Dependents</h4><div class="chip-row">${(m.dependents||[]).map(d=>`<button class="chip" data-kind="module" data-id="${d}">${esc(d)}</button>`).join("")||"—"}</div></section>
    <section><h4>Evidence</h4><div class="evidence">${esc(m.evidence)}</div></section>
    <div class="actions"><button class="btn" data-view="architecture">在依赖图中查看</button>
      ${relatedFlows(m.id)[0]?`<button class="btn" data-go-flow="${relatedFlows(m.id)[0].id}">相关 Flow</button>`:""}</div></div>`;
}
function detailFlowNode(n){
  const flow = currentFlow();
  const ups = flow.edges.filter(e=>e[1]===n.id).map(e=>e[0]);
  const downs = flow.edges.filter(e=>e[0]===n.id).map(e=>e[1]);
  const label = id => (flow.nodes.find(x=>x.id===id)||{}).label||id;
  return `<div class="detail"><h3>${esc(n.label)} <span class="badge ${esc(n.verification)}">${esc(n.verification)}</span></h3>
    <p class="role">${esc(n.responsibility)}</p>
    <section><h4>Crate / File / Symbol</h4>
      <div class="chip-row"><button class="chip" data-kind="module" data-id="${esc(n.crate)}">${esc(n.crate)}</button></div>
      <div style="margin-top:6px"><button class="file-link" data-file="${esc(n.file)}">${esc(n.file)}</button></div>
      <div class="evidence" style="margin-top:6px">${esc(n.symbol)}</div></section>
    <section><h4>Upstream</h4><div class="chip-row">${ups.map(u=>`<button class="chip" data-kind="flow-node" data-id="${u}">${esc(label(u))}</button>`).join("")||"—"}</div></section>
    <section><h4>Downstream</h4><div class="chip-row">${downs.map(d=>`<button class="chip" data-kind="flow-node" data-id="${d}">${esc(label(d))}</button>`).join("")||"—"}</div></section>
    <section><h4>Why this step matters</h4><p class="role">${esc(n.responsibility)}</p></section>
    <section><h4>Evidence</h4><div class="evidence">${esc(n.evidence)}</div></section></div>`;
}

function highlight(){
  document.querySelectorAll(".node-hit").forEach(g=>g.classList.remove("selected","dim","hl"));
  document.querySelectorAll(".edge").forEach(e=>e.classList.remove("dim","hl","edge-dependency","edge-dependent"));
  if(state.view==="architecture" && state.selectedKind==="module" && state.selected){
    const m = findModule(state.selected); if(!m) return;
    const keep = new Set([m.id, ...(m.deps||[]), ...(m.dependents||[])]);
    document.querySelectorAll(".node-hit").forEach(g=>{
      const id=g.dataset.id;
      if(id===m.id) g.classList.add("selected","hl");
      else if(keep.has(id)) g.classList.add("hl");
      else g.classList.add("dim");
    });
    document.querySelectorAll(".edge").forEach(e=>{
      const f=e.dataset.from, t=e.dataset.to;
      if(t===m.id && keep.has(f)){
        e.classList.add("hl","edge-dependency");
      }else if(f===m.id && keep.has(t)){
        e.classList.add("hl","edge-dependent");
      }else{
        e.classList.add("dim");
      }
    });
  } else if(state.selected){
    document.querySelectorAll(`.node-hit[data-id="${CSS.escape(state.selected)}"]`).forEach(g=>g.classList.add("selected"));
  }
}

function bind(){
  $("tabs").onclick = e=>{
    const b=e.target.closest("[data-view]"); if(!b) return;
    pushHistory(); state.view=b.dataset.view; render();
  };
  $("main").onclick = e=>{
    const modeBtn=e.target.closest("[data-graph-mode]");
    if(modeBtn){ state.graphMode=modeBtn.dataset.graphMode; render(); return; }
    const zoomBtn=e.target.closest("[data-zoom]");
    if(zoomBtn){
      const graph=zoomBtn.dataset.graph;
      const action=zoomBtn.dataset.zoom;
      const current=state.graphZoom[graph]||1;
      state.graphZoom[graph]=action==="reset"?1:Math.min(1.8,Math.max(.7,current+(action==="in"?.15:-.15)));
      render();
      return;
    }
    const flowBtn=e.target.closest("[data-flow]");
    if(flowBtn){ pushHistory(); state.flowId=flowBtn.dataset.flow; state.view = state.view==="library"?"library":"flow"; state.selected=null; render(); return; }
    const goFlow=e.target.closest("[data-go-flow]");
    if(goFlow){ pushHistory(); state.flowId=goFlow.dataset.goFlow; state.view="flow"; state.selected=null; render(); return; }
    const rp=e.target.closest("[data-rp]");
    if(rp){ pushHistory(); state.readingStep=+rp.dataset.rp; render(); return; }
    const file=e.target.closest("[data-file]");
    if(file){ select("file", file.dataset.file); return; }
    const hit=e.target.closest("[data-kind]");
    if(hit && hit.dataset.kind){
      if(hit.dataset.kind==="module"){ state.moduleId=hit.dataset.id; }
      select(hit.dataset.kind, hit.dataset.id);
      if(state.view==="modules" || state.view==="architecture") render();
      return;
    }
    const v=e.target.closest("[data-view]");
    if(v){ pushHistory(); state.view=v.dataset.view; render(); }
  };
  $("detail").onclick = e=>{
    const goFlow=e.target.closest("[data-go-flow]");
    if(goFlow){ pushHistory(); state.flowId=goFlow.dataset.goFlow; state.view="flow"; state.selected=null; render(); return; }
    const file=e.target.closest("[data-file]");
    if(file){ select("file", file.dataset.file); return; }
    const hit=e.target.closest("[data-kind]");
    if(hit){
      if(hit.dataset.kind==="module"){ pushHistory(); state.view="modules"; state.moduleId=hit.dataset.id; select("module", hit.dataset.id, {push:false}); render(); return; }
      select(hit.dataset.kind, hit.dataset.id);
      return;
    }
    const v=e.target.closest("[data-view]");
    if(v){ pushHistory(); state.view=v.dataset.view; render(); }
  };
}

function buildSearchIndex(){
  const items=[];
  TOUR.modules.forEach(m=>{
    items.push({k:m.id, kind:"crate", type:"module", id:m.id});
    (m.keyFiles||[]).forEach(f=>items.push({k:f, kind:"file", type:"file", id:f}));
    (m.keySymbols||[]).forEach(s=>items.push({k:s, kind:"symbol", type:"module", id:m.id, extra:m.id}));
  });
  Object.values(TOUR.flows).forEach(f=>f.nodes.forEach(n=>{
    items.push({k:`${n.label} ${n.symbol}`, kind:"flow", type:"flow-node", id:n.id, flow:f.id, extra:f.title});
  }));
  return items;
}
const INDEX = buildSearchIndex();
function setupSearch(){
  const input=$("search"), box=$("searchResults");
  input.addEventListener("input", ()=>{
    const q=input.value.trim().toLowerCase();
    if(!q){ box.classList.remove("open"); box.innerHTML=""; return; }
    const hits=INDEX.filter(i=>i.k.toLowerCase().includes(q)).slice(0,20);
    box.innerHTML = hits.map(h=>`<button data-si="${esc(h.type)}" data-id="${esc(h.id)}" data-flow="${esc(h.flow||"")}"><div>${esc(h.k)}</div><div class="k">${esc(h.kind)}${h.extra?" · "+esc(h.extra):""}</div></button>`).join("") || `<button disabled>无结果</button>`;
    box.classList.add("open");
  });
  box.addEventListener("click", e=>{
    const b=e.target.closest("button[data-si]"); if(!b) return;
    box.classList.remove("open");
    const t=b.dataset.si, id=b.dataset.id;
    if(t==="module"){ pushHistory(); state.view="modules"; state.moduleId=id; select("module", id, {push:false}); render(); }
    else if(t==="file"){ select("file", id); }
    else if(t==="flow-node"){ pushHistory(); state.view="flow"; state.flowId=b.dataset.flow||state.flowId; select("flow-node", id, {push:false}); render(); }
  });
  document.addEventListener("click", e=>{ if(!e.target.closest(".search-wrap")) box.classList.remove("open"); });
}

$("btnBack").onclick = goBack;
setupSearch();
render();
