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
  for key,title in [('data','数据中心'),('pool','股票池'),('research','策略研究'),('compare','回测对比'),('plan','交易计划')]:
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
   for key in ['data','pool','research','compare','plan']:
    page.locator('nav [data-nav="'+key+'"]').click();assert page.evaluate('document.documentElement.scrollWidth <= innerWidth'),(key,width)
  page.locator('[data-action="theme"]').click();assert page.locator('body').evaluate('(el)=>el.classList.contains("dark")')
  page.locator('nav [data-nav="research"]').click();page.screenshot(path=str(out/'research-dark.png'),full_page=True)
  assert not js,js
 check('UI-P06',layouts)
 browser.close()
if errors:raise SystemExit(1)
