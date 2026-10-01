from pathlib import Path
import datetime,json,time,os
from playwright.sync_api import sync_playwright
root=Path(__file__).resolve().parents[4]
change=root/'logos/changes/astock-research-desktop'
out=change/'prototype-review'
html=change/'deltas/prd/2-product-design/2-page-design/core-05-ai-workspace-prototype.html'
report=out/'ai-test-results.jsonl'
report.write_text('')
failures=[]
def check(ident,fn):
    start=time.monotonic()
    row={'id':ident,'status':'pass','timestamp':datetime.datetime.now(datetime.timezone.utc).isoformat(),'source':'AI工作台HTML原型检查，非生产业务验收'}
    try: fn()
    except Exception as exc:
        row.update(status='fail',error=str(exc));failures.append(str(exc))
    row['duration_ms']=round((time.monotonic()-start)*1000)
    with report.open('a') as f:f.write(json.dumps(row,ensure_ascii=False)+'\n')
    print(json.dumps(row,ensure_ascii=False),flush=True)
with sync_playwright() as pw:
    browser=pw.chromium.launch(headless=True,executable_path=os.environ.get('PROTOTYPE_CHROMIUM','/root/.cache/ms-playwright/chromium-1243/chrome-linux64/chrome'))
    page=browser.new_page(viewport={'width':1440,'height':1080})
    js=[];page.on('pageerror',lambda e:js.append(str(e)))
    page.goto(html.as_uri())
    def nav(p):page.locator('[data-nav="'+p+'"]').click()
    def run_agents():
        page.locator('#startAgents').click();page.wait_for_function('current() === true')
    def confirm():page.locator('#confirmReq').click()
    def save_code():
        page.locator('#checkCode').click();page.locator('#saveVersion').click()
    def agents():
        page.screenshot(path=str(out/'ai-requirements.png'),full_page=True)
        confirm();page.locator('#startAgents').click()
        page.wait_for_function('state.agentStatus[1] === "执行中" && state.agentStatus[2] === "执行中"')
        page.locator('#cancelAgents').click();count=page.evaluate('state.artifacts.length')
        page.wait_for_timeout(900);assert page.evaluate('state.artifacts.length')==count
        assert not page.evaluate('state.busy')
        page.locator('#budget').fill('2');page.locator('#startAgents').click()
        page.wait_for_function('!state.busy');assert '预算不足' in page.locator('#app').inner_text()
        page.locator('#budget').fill('6');run_agents()
        assert page.evaluate('state.used')==6
        page.locator('[data-agent="2"]').click();assert '反例' in page.locator('#app').inner_text()
        page.screenshot(path=str(out/'ai-agents.png'),full_page=True)
    check('ST-S17-01',agents)
    def requirements():
        nav('requirements');page.locator('#reqText').fill('');confirm()
        assert '请填写需求' in page.locator('#formError').inner_text()
        assert page.evaluate('state.req.id')==1
        page.locator('#reqText').fill('验证成交阶段资金约束；保留修复前后实验。')
        page.locator('#allocation').fill('101');confirm();assert page.evaluate('state.req.id')==1
        page.locator('#allocation').fill('100');confirm()
        assert page.evaluate('state.req.id')==2
        assert not page.evaluate('current()')
        assert page.evaluate('state.artifacts.some(x=>x.req===1)')
        run_agents()
    check('ST-S17-02',requirements)
    def workflow():
        nav('workflow');page.locator('[data-node="size"]').click()
        assert '目标比例、现金、参考价格' in page.locator('#app').inner_text()
        page.locator('[data-source-line="10"]').click()
        assert page.evaluate('document.querySelector("#source").value.substring(document.querySelector("#source").selectionStart,document.querySelector("#source").selectionEnd)').strip().startswith('quantity')
        nav('workflow');page.locator('#flowAllocation').fill('50');page.locator('#saveFlow').click()
        assert page.evaluate('state.flow.allocation')==0.5
        nav('code');save_code();page.locator('#runExperiment').click()
        r=page.evaluate('state.runs.at(-1)');assert r['quantity']==500 and r['cash']==4945 and r['final']==10345
        nav('workflow');page.locator('#flowAmount').fill('2500');page.locator('#saveFlow').click()
        nav('code');assert page.locator('#runExperiment').is_disabled();save_code();page.locator('#runExperiment').click()
        r=page.evaluate('state.runs.at(-1)');assert r['held']==0 and r['final']==10000
        nav('workflow');page.locator('#flowAmount').fill('1000');page.locator('#flowAllocation').fill('100');page.locator('#saveFlow').click()
        page.screenshot(path=str(out/'ai-workflow.png'),full_page=True)
    check('ST-S18-01',workflow)
    def development():
        nav('code');original=page.locator('#source').input_value()
        page.locator('#source').fill(original+'# 自定义修改\n');page.locator('#checkCode').click()
        assert page.locator('#saveVersion').is_disabled();assert '无法运行' in page.locator('#codeError').inner_text()
        page.locator('#restoreCode').click();save_code()
        frozen=page.evaluate('JSON.stringify(state.activeVersion)')
        page.locator('#source').fill(original+'# 新草稿\n');assert page.locator('#saveVersion').is_disabled()
        assert page.evaluate('JSON.stringify(state.activeVersion)')==frozen
        page.locator('#runExperiment').click()
        nav('code');page.locator('#applyFix').click();save_code()
        assert page.evaluate('state.activeVersion.variant')==2
        assert page.evaluate('JSON.stringify(state.versions[state.versions.length-2])')==frozen
        page.screenshot(path=str(out/'ai-development.png'),full_page=True)
        page.locator('#runExperiment').click()
    check('ST-S18-02',development)
    def metrics():
        a,b=page.evaluate('state.runs.slice(-2)')
        assert a['quantity']==1000 and a['rejected']==1 and a['final']==10000
        ae=next(e for e in a['events'] if e['node']=='fill');assert ae['input']['cost']==10105
        assert b['quantity']==900 and b['held']==900 and b['cash']==905
        assert b['equity']==[10000,10265,10625]
        assert abs(b['returnPct']-6.25)<1e-9 and b['rejected']==0
        assert all(e['date']=='2026-01-05' for e in b['events'] if e['node']=='signal')
        assert all(e['date']=='2026-01-06' for e in b['events'] if e['node']=='fill')
        assert page.evaluate('divergence().event.node')=='size'
        assert all('visibleAt' in e and 'line' in e for e in b['events'])
    check('ST-S19-01',metrics)
    def replay():
        page.locator('#dateFilter').select_option('2026-01-06');page.locator('#symbolFilter').select_option('SYN-A');page.locator('#nodeFilter').select_option('fill')
        assert page.locator('[data-event]').count()==1
        assert '成交' in page.locator('[data-event]').inner_text()
        page.locator('#eventSource').click();assert '▶ 13' in page.locator('#dialogBody').inner_text()
        page.locator('dialog button').click()
        page.locator('#symbolFilter').select_option('SYN-C');assert page.locator('[data-event]').count()==0
        assert '当前条件没有事件' in page.locator('#app').inner_text()
        page.locator('#resetFilters').click();page.locator('#rewind').click();assert page.evaluate('state.eventIndex')==0
        page.locator('#step').click();assert page.evaluate('state.eventIndex')==1
        page.locator('#play').click();page.wait_for_function('state.eventIndex>=2');page.locator('#play').click()
        index=page.evaluate('state.eventIndex');page.wait_for_timeout(800);assert page.evaluate('state.eventIndex')==index
        nav('experiments');page.locator('#jumpDivergence').click()
        assert page.evaluate('state.runs[state.selectedRun].events[state.eventIndex].node')=='size'
        page.screenshot(path=str(out/'ai-debug.png'),full_page=True)
    check('ST-S19-02',replay)
    def experiments():
        nav('experiments');assert page.locator('.chart polyline').count()==2
        assert '6.25%' in page.locator('#app').inner_text()
        page.locator('[data-manifest]').last.click();assert 'SYN-202601' in page.locator('#dialogBody').inner_text();page.locator('dialog button').click()
        with page.expect_download() as dl:page.locator('#exportExperiments').click()
        dl.value.save_as(str(out/'ai-experiments.json'))
        exported=json.loads((out/'ai-experiments.json').read_text())
        assert exported['runs'][-1]['equity']==[10000,10265,10625]
        frozen=page.evaluate('JSON.stringify(state.runs)')
        nav('requirements');page.locator('#allocation').fill('75');confirm()
        nav('code');assert page.locator('#runExperiment').is_disabled()
        assert page.evaluate('JSON.stringify(state.runs)')==frozen
        nav('experiments');page.screenshot(path=str(out/'ai-experiments.png'),full_page=True)
    check('ST-S19-03',experiments)
    def layouts():
        for width in [1100,1440]:
            page.set_viewport_size({'width':width,'height':1080})
            for theme in [False,True]:
                if page.evaluate('document.body.classList.contains("dark")')!=theme:page.locator('#theme').click()
                for p in ['requirements','agents','workflow','code','debug','experiments']:
                    nav(p)
                    assert page.evaluate('document.documentElement.scrollWidth<=innerWidth'),(width,theme,p)
        assert page.locator('.shared a').count()==4
        for href in page.locator('.shared a').evaluate_all('(els)=>els.map(e=>e.getAttribute("href"))'):
            assert (html.parent/href.split('#')[0]).exists()
        assert not js,js
        page.screenshot(path=str(out/'ai-dark.png'),full_page=True)
    check('ST-S19-04',layouts)
    # 最后以默认需求重新演示，截图对应清晰的 v1/v2 和 E1/E2 旅程。
    page.goto(html.as_uri().split('#')[0]);page.set_viewport_size({'width':1440,'height':1080})
    def shot(name):
        page.wait_for_function('document.querySelector("#notice").textContent === ""')
        page.screenshot(path=str(out/name),full_page=True)
    shot('ai-requirements.png');confirm();run_agents();page.locator('[data-agent="2"]').click();shot('ai-agents.png')
    nav('workflow');shot('ai-workflow.png')
    nav('code');save_code();page.locator('#runExperiment').click();shot('ai-debug-rejected.png')
    nav('code');page.locator('#applyFix').click();save_code();shot('ai-development.png')
    page.locator('#runExperiment').click();nav('experiments');page.locator('#jumpDivergence').click();shot('ai-debug.png')
    nav('experiments');shot('ai-experiments.png');page.locator('#theme').click();shot('ai-dark.png')
    assert not js,js
    browser.close()
raise SystemExit(1 if failures else 0)
