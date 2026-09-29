from pathlib import Path
import json,time,datetime
from playwright.sync_api import sync_playwright
root=Path(__file__).resolve().parents[4];c=root/'logos/changes/astock-research-desktop';out=c/'prototype-review';out.mkdir(exist_ok=True)
html=c/'deltas/prd/2-product-design/2-page-design/core-03-research-prototype.html'
report=out/'test-results.jsonl';report.write_text('')
errors=[]
def check(ident,fn):
 start=time.time();status='pass';error=None
 try:fn()
 except Exception as exc:status='fail';error=str(exc)
 row={'id':ident,'status':status,'duration_ms':round((time.time()-start)*1000),'timestamp':datetime.datetime.now(datetime.timezone.utc).isoformat(),'source':'HTML原型交互检查，非业务验收'}
 if error:row['error']=error
 with report.open('a') as f:f.write(json.dumps(row,ensure_ascii=False)+'\n')
 print(json.dumps(row,ensure_ascii=False))
 if error:errors.append(error)
with sync_playwright() as pw:
 browser=pw.chromium.launch(headless=True, executable_path="/root/.cache/ms-playwright/chromium-1243/chrome-linux64/chrome")
 page=browser.new_page(viewport={'width':1440,'height':1200},device_scale_factor=1)
 js=[];page.on('pageerror',lambda e:js.append(str(e)))
 page.goto(html.as_uri());page.screenshot(path=str(out/'research.png'),full_page=True)
 def navigation():
  for key,title in [('data','数据中心'),('pool','股票池'),('develop','策略开发'),('research','策略研究'),('compare','回测对比'),('plan','交易计划')]:
   page.locator('nav [data-nav="'+key+'"]').click();assert page.locator('h1').inner_text()==title
  assert not js,js
 check('UI-P01',navigation)
 def filtering():
  page.locator('nav [data-nav="pool"]').click();page.locator('#stocksearch').fill('茅台');assert page.locator('#poolrows tr').count()==1
  page.locator('#stocksearch').fill('');page.locator('#liquidity').select_option('50');assert '暂无匹配' in page.locator('#poolrows').inner_text()
  page.locator('#liquidity').select_option('0');page.locator('[data-stock="300750"]').click();assert '宁德时代' in page.locator('#stockdetail h2').inner_text()
  page.screenshot(path=str(out/'pool.png'),full_page=True)
 check('UI-P02',filtering)
 def cancel():
  page.locator('nav [data-nav="research"]').click();page.locator('[data-config="top"]').select_option('5')
  page.locator('[data-action="run"]').first.click();page.locator('[data-action="cancel"]').wait_for();page.locator('[data-action="cancel"]').click()
  assert '已取消' in page.locator('#status').inner_text();assert page.evaluate('state.run')==0
 check('UI-P03',cancel)
 def complete():
  page.locator('[data-action="run"]').first.click();page.wait_for_function('state.run === 1 && state.task === null')
  assert '新演示 1' in page.locator('#app').inner_text();assert '演示数据' in page.locator('.topbar').inner_text()
 check('UI-P04',complete)
 def export():
  page.locator('nav [data-nav="plan"]').click();page.locator('[data-action="holdings"]').click();page.locator('#capital').fill('200000')
  page.locator('[data-holding="600036"]').fill('500');page.locator('[data-action="save-holdings"]').click()
  assert page.evaluate('state.capital')==200000;assert page.evaluate('state.holdings["600036"]')==500
  with page.expect_download() as item:page.locator('[data-action="export"]').click()
  dest=out/'demo-plan.csv';item.value.save_as(str(dest));csv=dest.read_text(encoding='utf-8-sig')
  assert '历史演示计划' in csv and '2026-09-28' in csv and '600036,招商银行,36.72,500,1000,500' in csv
  page.screenshot(path=str(out/'plan.png'),full_page=True)
 check('UI-P05',export)
 def layouts():
  for width in [1100,1440]:
   page.set_viewport_size({'width':width,'height':1200})
   for key in ['data','pool','develop','research','compare','plan']:
    page.locator('nav [data-nav="'+key+'"]').click();assert page.evaluate('document.documentElement.scrollWidth <= innerWidth'),(key,width)
  page.locator('[data-action="theme"]').click();assert page.locator('body').evaluate('(el)=>el.classList.contains("dark")')
  page.locator('nav [data-nav="research"]').click();page.screenshot(path=str(out/'research-dark.png'),full_page=True)
  assert not js,js
 check('UI-P06',layouts)

 def development():
  page.locator('nav [data-nav="develop"]').click()
  if page.locator('body').evaluate('(e)=>e.classList.contains("dark")'):page.locator('[data-action="theme"]').click()
  page.locator('[data-dev="create"]').click();page.locator('#newstrategyname').fill('测试动量');page.locator('[data-dev="confirm-create"]').click()
  page.locator('#deveditor').fill(page.locator('#deveditor').input_value()+'\n# 编辑保留检查\n')
  text=page.locator('#deveditor').input_value()
  page.locator('nav [data-nav="pool"]').click();page.locator('nav [data-nav="develop"]').click();assert page.locator('#deveditor').input_value()==text
  page.locator('[data-dev="duplicate"]').click();page.locator('#deveditor').fill(text+'# 独立副本\n')
  assert page.evaluate('dev.strategies.find(x=>x.name==="测试动量").source')==text
  page.locator('[data-dev-select="d3"]').click();assert page.locator('#deveditor').input_value()==text
  page.screenshot(path=str(out/'development.png'),full_page=True)
 check('UI-P07',development)
 def validation():
  source=page.locator('#deveditor').input_value();page.locator('#deveditor').fill('没有入口')
  page.locator('[data-dev="check"]').click();assert '缺少入口' in page.locator('#devlogs').inner_text();assert page.locator('[data-dev="save"]').is_disabled()
  page.locator('#deveditor').fill(source);page.locator('[data-dev-tab="schema"]').click();schema=page.locator('#deveditor').input_value();page.locator('#deveditor').fill('{')
  page.locator('[data-dev="check"]').click();assert '参数定义' in page.locator('#devlogs').inner_text()
  page.locator('#deveditor').fill(schema);page.locator('[data-dev="check"]').click();assert '未编译、未执行 Python' in page.locator('#devlogs').inner_text();assert page.locator('[data-dev="save"]').is_enabled()
 check('UI-P08',validation)
 def versions():
  page.locator('[data-dev-tab="source"]').click();page.locator('#deveditor').fill(page.locator('#deveditor').input_value()+'# 新改动\n');assert page.locator('[data-dev="save"]').is_disabled()
  page.locator('[data-dev="check"]').click();page.locator('[data-dev="save"]').click();assert page.evaluate('currentDraft().versions.length')==1
  frozen=page.evaluate('currentDraft().versions[0].source');page.locator('#deveditor').fill(frozen+'# 尚未保存的新草稿\n')
  page.locator('[data-version="1"]').click();assert page.locator('#modal .codepreview').first.inner_text()==frozen.rstrip('\n') or page.locator('#modal .codepreview').first.inner_text()==frozen
  assert page.locator('#modal textarea').count()==0;page.locator('[data-action="close-modal"]').click();assert page.evaluate('currentDraft().versions[0].source')==frozen
 check('UI-P09',versions)
 def binding():
  page.locator('[data-dev="send"]').click();assert page.locator('h1').inner_text()=='策略研究';assert '测试动量' in page.locator('.devbinding').inner_text();assert 'v1' in page.locator('.devbinding').inner_text()
  frozen=page.evaluate('dev.binding.source');assert page.locator('[data-config="window"]').count()==0
  page.locator('.devbinding [data-dev="return"]').click();page.locator('#deveditor').fill(page.locator('#deveditor').input_value()+'# 再次修改\n')
  page.locator('nav [data-nav="research"]').click();assert page.evaluate('dev.binding.source')==frozen
  page.locator('[data-action="run"]').first.click();page.wait_for_function('state.task === null');assert page.evaluate('dev.lastRunBinding.source')==frozen
  page.screenshot(path=str(out/'development-binding.png'),full_page=True)
 check('UI-P10',binding)
 def source_export():
  page.locator('nav [data-nav="develop"]').click();expected=page.locator('#deveditor').input_value()
  with page.expect_download() as item:page.locator('[data-dev="download"]').click()
  file=out/'demo-strategy.py';item.value.save_as(str(file));assert file.read_text()==expected
  for width in [1100,1440]:
   page.set_viewport_size({'width':width,'height':1200});assert page.evaluate('document.documentElement.scrollWidth <= innerWidth')
  page.locator('[data-action="theme"]').click();assert page.locator('#deveditor').is_visible();page.screenshot(path=str(out/'development-dark.png'),full_page=True)
  page.locator('[data-action="theme"]').click();page.locator('[data-dev="check"]').click();page.screenshot(path=str(out/'development.png'),full_page=True)
  assert not js,js
 check('UI-P11',source_export)

 page.goto(html.as_uri()+'#develop')
 page.locator('[data-dev="check"]').click();page.locator('[data-dev="save"]').click()
 page.wait_for_timeout(3400)
 page.screenshot(path=str(out/'development.png'),full_page=True)
 browser.close()
if errors:raise SystemExit(1)
