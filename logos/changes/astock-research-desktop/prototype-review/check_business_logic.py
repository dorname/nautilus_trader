"""统一离线原型的浏览器编排检查；不触发生产验收。"""
from pathlib import Path
from datetime import datetime, timezone
import json
import time
import os
import traceback
from playwright.sync_api import sync_playwright

CHANGE = Path(__file__).resolve().parent.parent
OUT = Path(__file__).resolve().parent
HTML = CHANGE / 'deltas/prd/2-product-design/2-page-design/core-05-ai-workspace-prototype.html'
OLD = HTML.with_name('core-03-research-prototype.html')
REPORT = OUT / 'business-test-results.jsonl'
REPORT.write_text('')
failures = []


def check(ident, function):
    """OpenLogos reporter：仅报告真实断言结果，原型证据单独存放。"""
    start = time.monotonic()
    record = {'id': ident, 'status': 'pass', 'timestamp': datetime.now(timezone.utc).isoformat(),
              'scenario': ident.split('-')[1], 'source': '统一离线HTML原型编排检查，非生产API或引擎验收'}
    try:
        reset()
        function()
        assert not errors, '存在页面脚本异常：' + repr(errors)
    except Exception as exc:
        record.update(status='fail', error=traceback.format_exc())
        failures.append(ident)
        page.screenshot(path=str(OUT / (ident + '-failure.png')))
    record['duration_ms'] = round((time.monotonic() - start) * 1000)
    with REPORT.open('a') as f:
        f.write(json.dumps(record, ensure_ascii=False) + '\n')
    print(json.dumps(record, ensure_ascii=False), flush=True)


with sync_playwright() as pw:
    browser = pw.chromium.launch(headless=True, executable_path=os.environ.get(
        'PROTOTYPE_CHROMIUM', '/root/.cache/ms-playwright/chromium-1243/chrome-linux64/chrome'), timeout=20000)
    page = browser.new_page(viewport={'width': 1440, 'height': 1000})
    page.set_default_timeout(5000)
    errors = []
    page.on('pageerror', lambda exc: errors.append(str(exc)))

    def reset():
        errors.clear()
        page.set_viewport_size({'width': 1440, 'height': 1000})
        page.goto(HTML.as_uri())

    def state(expr):
        return page.evaluate(expr)

    def nav(route):
        page.locator('#nav [data-go="' + route + '"], #resourceNav [data-go="' + route + '"]').click()

    def act(name):
        page.locator('[data-act="' + name + '"]:visible').click()

    def task_done():
        page.wait_for_function('!p().task')

    def chat(text):
        page.locator('#chatInput').fill(text)
        page.locator('#chatInput').press('Enter')

    def base():
        nav('requirements')
        act('confirmReq')
        nav('design')
        act('generateDesign')
        task_done()
        act('saveDesign')
        nav('develop')
        act('generateCode')
        task_done()
        act('checkCode')
        act('saveVersion')

    def run():
        act('runExperiment')
        task_done()

    def fixed():
        base()
        run()
        nav('develop')
        act('showDiff')
        act('applyFix')
        act('checkCode')
        act('saveVersion')
        run()

    def ready():
        fixed()
        act('makeReport')
        nav('plan')

    def fill_plan(selector, value):
        page.locator(selector).fill(str(value))
        page.locator(selector).press('Tab')

    def journey():
        assert state('nextStep().route') == 'requirements'
        base()
        assert state('nextStep().label') == '运行合成实验'
        run()
        assert state('nextStep().label') == '生成验证报告'
        act('makeReport')
        assert state('nextStep().label') == '排查失败证据'
        nav('develop')
        act('showDiff')
        act('applyFix')
        act('checkCode')
        act('saveVersion')
        assert state('p().runs.length') == 1
        assert state('nextStep().label') == '运行合成实验'
        run()
        act('makeReport')
        assert state('nextStep().label') == '生成演示计划'
        nav('plan')
        act('makePlan')
        assert state('nextStep().label') == '核对交易计划'
        act('checkPlan')
        assert state('nextStep().label') == '确认导出演示清单'
        nav('overview')
        assert '确认导出演示清单' in page.locator('#canvas').inner_text()
        page.screenshot(path=str(OUT / 'business-overview.png'))
        nav('data')
        act('updateData')
        task_done()
        assert state('nextStep().label') == '保存当前策略版本'
    check('ST-S17-15', journey)

    def drafts():
        base()
        frozen = state('JSON.stringify(active())')
        nav('requirements')
        page.locator('#reqText').fill('新的研究草稿')
        nav('design')
        page.locator('#designNote').fill('新的设计草稿')
        nav('develop')
        page.locator('#code').fill('新的源码草稿')
        nav('overview')
        assert '需求、设计、源码草稿尚未确认或保存' in page.locator('#canvas').inner_text()
        assert state('JSON.stringify(active())') == frozen
        assert state('fresh(active())')
        nav('requirements')
        act('confirmReq')
        assert not state('fresh(active())')
        assert state('nextStep().label') == '保存当前设计'
    check('ST-S18-14', drafts)

    def compare_inputs():
        fixed()
        assert '控制输入一致' in page.locator('#comparisonStatus').inner_text()
        frozen = state('JSON.stringify(p().runs)')
        nav('data')
        act('updateData')
        task_done()
        nav('develop')
        act('checkCode')
        act('saveVersion')
        run()
        assert '数据快照' in page.locator('#comparisonStatus').inner_text()
        assert '不作代码效果归因' in page.locator('#comparisonStatus').inner_text()
        assert state('JSON.stringify(p().runs.slice(0,2))') == frozen
        nav('design')
        page.locator('#designAllocation').fill('50')
        act('saveDesign')
        nav('develop')
        act('checkCode')
        act('saveVersion')
        run()
        assert '设计' in page.locator('#comparisonStatus').inner_text()
        page.screenshot(path=str(OUT / 'business-comparison.png'))
        run()
        assert '重复实验' in page.locator('#comparisonStatus').inner_text()
    check('ST-S19-14', compare_inputs)
    browser.close()
print('通过数：', 3 - len(failures), '失败：', failures, flush=True)
raise SystemExit(bool(failures))
