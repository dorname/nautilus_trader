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
REPORT = OUT / 'unified-test-results.jsonl'
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

    def entry():
        mapping = {'data': 'data', 'pool': 'pool', 'develop': 'develop',
                   'research': 'experiments', 'compare': 'experiments', 'plan': 'plan', 'unknown': 'overview'}
        for old, new in mapping.items():
            page.goto(OLD.as_uri() + '#' + old)
            page.wait_for_url('**/core-05-ai-workspace-prototype.html#' + new)
            assert state('p().page') == new
        chat('请解释回测与验证的区别')
        count = state('p().messages.length')
        for route in ['data', 'pool', 'library', 'requirements', 'overview']:
            nav(route)
            assert state('p().messages.length') == count
            assert state('projects.length') == 1
        page.screenshot(path=str(OUT / 'unified-overview.png'))
    check('ST-S17-11', entry)

    def projects_case():
        nav('requirements')
        original = page.locator('#reqText').input_value()
        page.locator('#reqText').fill('')
        act('confirmReq')
        assert state('p().req') is None
        page.locator('#reqText').fill(original)
        page.locator('#allocation').fill('101')
        act('confirmReq')
        assert state('p().req') is None
        page.locator('#allocation').fill('100')
        act('confirmReq')
        assert state('p().req.id') == 1
        chat('解释验证区别')
        count = state('p().messages.length')
        page.locator('#newProject').click()
        page.locator('#newName').fill('独立研究项目')
        act('createProject')
        assert state('p().req') is None
        assert state('p().runs.length') == 0
        assert state('p().messages.length') == 1
        page.locator('#projectSelect').select_option('1')
        assert state('p().req.id') == 1
        assert state('p().messages.length') == count
    check('ST-S17-12', projects_case)

    def tasks_case():
        chat('你好，请帮我预测未来全部收益')
        assert '尚不能理解任意指令' in page.locator('#messages').inner_text()
        chat('生成需求草稿')
        act('cancelTask')
        page.wait_for_timeout(1000)
        assert state('p().req') is None
        assert '任务已取消' in page.locator('#messages').inner_text()
        page.locator('#showEvidence').click()
        act('simulateFailure')
        act('retryTask')
        task_done()
        assert '失败：已模拟' in page.locator('#messages').inner_text()
        assert state('p().req') is None
        act('retryTask')
        task_done()
        assert state('p().page') == 'requirements'
        assert state('p().retry') is None
        assert '需求草稿已整理' in page.locator('#messages').inner_text()
    check('ST-S17-13', tasks_case)

    def layout():
        chat('<img src=x onerror="window.injected=1">')
        assert state('window.injected') is None
        assert page.locator('#messages img').count() == 0
        assert '<img' in page.locator('#messages').inner_text()
        for width, height in [(1440, 1000), (1100, 900)]:
            page.set_viewport_size({'width': width, 'height': height})
            for route in ['overview', 'requirements', 'develop', 'data', 'pool', 'plan']:
                nav(route)
                assert state('document.documentElement.scrollWidth <= innerWidth'), route
                assert state('document.querySelector(".canvas").scrollWidth <= document.querySelector(".canvas").clientWidth'), route
            page.locator('#focusToggle').click()
            assert not page.locator('.chat').is_visible()
            page.locator('#focusToggle').click()
            assert page.locator('.chat').is_visible()
        page.goto(HTML.as_uri())
        page.set_viewport_size({'width': 1100, 'height': 900})
        page.screenshot(path=str(OUT / 'unified-1100.png'))
    check('ST-S17-14', layout)

    def diagrams():
        base()
        nav('design')
        assert page.locator('svg.diagram [data-node]').count() == 6
        page.locator('[data-node="size"]').click()
        assert '资金、价格、费用' in page.locator('#canvas').inner_text()
        act('nodeSource')
        assert state('document.activeElement.id') == 'code'
        assert 'quantity' in state('document.activeElement.value.slice(document.activeElement.selectionStart,document.activeElement.selectionEnd)')
        nav('design')
        page.locator('svg.diagram').scroll_into_view_if_needed()
        page.screenshot(path=str(OUT / 'unified-flow.png'))
        act('sequenceTab')
        assert page.locator('svg.diagram [data-node]').count() == 6
        assert '2026-01-05' in page.locator('svg.diagram').text_content()
        assert '2026-01-06' in page.locator('svg.diagram').text_content()
        page.locator('svg.diagram').scroll_into_view_if_needed()
        page.screenshot(path=str(OUT / 'unified-sequence.png'))
        page.locator('#designNote').fill('新的设计说明，保存后可追溯。')
        act('saveDesign')
        assert state('p().design.note') == '新的设计说明，保存后可追溯。'
        assert state('p().design.id') == 2
    check('ST-S18-11', diagrams)

    def versions():
        base()
        original = state('p().versions[0].source')
        page.locator('#code').fill('print("任意代码不得执行")')
        act('checkCode')
        assert '不能执行此源码' in page.locator('#canvas').inner_text()
        assert page.locator('[data-act="saveVersion"]').is_disabled()
        act('showDiff')
        assert state('p().code') == 'print("任意代码不得执行")'
        assert '预留 5 元费用' in page.locator('.diff').inner_text()
        act('applyFix')
        act('checkCode')
        act('saveVersion')
        assert state('p().versions.length') == 2
        assert state('p().versions[0].source') == original
        assert state('p().versions[1].variant') == 2
        nav('library')
        page.locator('[data-version="1"]').click()
        assert state('p().active') == 1
        nav('develop')
        page.screenshot(path=str(OUT / 'unified-development.png'))
    check('ST-S18-12', versions)

    def stale():
        fixed()
        frozen = state('JSON.stringify(p().runs)')
        nav('requirements')
        page.locator('#reqText').fill('需求变更：保留历史实验并重新研究。')
        act('confirmReq')
        assert not state('fresh(active())')
        nav('develop')
        assert page.locator('[data-act="runExperiment"]').is_disabled()
        assert state('JSON.stringify(p().runs)') == frozen
        nav('design')
        act('saveDesign')
        nav('develop')
        act('checkCode')
        act('saveVersion')
        assert state('fresh(active())')
        nav('data')
        act('updateData')
        task_done()
        assert not state('fresh(active())')
        assert state('JSON.stringify(p().runs)') == frozen
        nav('pool')
        page.locator('#poolSearch').fill('不存在')
        assert '没有匹配的样本' in page.locator('#canvas').inner_text()
        page.locator('#poolSearch').fill('')
        page.locator('#poolAmount').fill('2000')
        act('savePool')
        assert state('p().pool') == ['SYN-A']
        assert state('JSON.stringify(p().runs)') == frozen
    check('ST-S18-13', stale)

    def math_case():
        fixed()
        a, b = state('p().runs')
        assert (a['final'], a['rejected'], a['held']) == (10000, 1, 0)
        assert (b['held'], b['cash'], b['final']) == (900, 905, 10625)
        assert abs(b['returnPct'] - 6.25) < 1e-9
        assert b['equity'] == [10000, 10265, 10625]
        assert b['version']['pool']['symbols'] == ['SYN-A', 'SYN-B']
        page.screenshot(path=str(OUT / 'unified-experiments.png'))
    check('ST-S19-11', math_case)

    def events_case():
        fixed()
        act('divergence')
        assert state('selectedRun().events[p().eventIndex].node') == 'size'
        act('eventSource')
        assert '第 8 行' in page.locator('#dialogTitle').inner_text()
        assert 'open_price' in page.locator('#dialogBody').inner_text()
        act('closeDialog')
        page.locator('[data-filter="fill"]').click()
        assert page.locator('tr[data-event]').count() == 1
        assert '已成交' in page.locator('#canvas').inner_text()
        page.locator('#runSelect').select_option('0')
        page.locator('[data-filter="fill"]').click()
        assert '资金不足' in page.locator('#canvas').inner_text()
        page.screenshot(path=str(OUT / 'unified-debug.png'))
        page.locator('[data-filter=""]').click()
        act('rewind')
        before = state('p().eventIndex')
        act('step')
        assert state('p().eventIndex') > before
        act('play')
        page.wait_for_timeout(780)
        act('play')
        index = state('p().eventIndex')
        page.wait_for_timeout(750)
        assert state('p().eventIndex') == index
    check('ST-S19-12', events_case)

    def reports():
        base()
        run()
        act('makeReport')
        assert state('p().report.demo') == '失败'
        assert not state('reportReady()')
        nav('develop')
        act('showDiff')
        act('applyFix')
        act('checkCode')
        act('saveVersion')
        run()
        act('makeReport')
        assert state('p().report.demo') == '通过'
        assert state('p().report.formal') == '证据不足'
        assert page.locator('#canvas').inner_text().count('未运行') == 3
        page.screenshot(path=str(OUT / 'unified-validation.png'))
        nav('experiments')
        with page.expect_download() as event:
            act('exportExperiments')
        event.value.save_as(OUT / 'unified-experiments.json')
        exported = json.loads((OUT / 'unified-experiments.json').read_text())
        assert len(exported['runs']) == 2
        for run_record in exported['runs']:
            assert all(k in run_record['version'] for k in ['source', 'req', 'design', 'data', 'pool', 'simulator'])
    check('ST-S19-13', reports)

    def account():
        ready()
        page.locator('#planCash').fill('5000')
        fill_plan('#holding0', 500)
        page.locator('#sellable0').fill('500')
        act('makePlan')
        t = state('p().plan')
        assert t['total'] == 10500
        assert t['rows'][0]['target'] == 900
        assert t['rows'][0]['delta'] == 400
        assert t['buys'] == 4405
        assert t['input']['snapshot'] != t['version']['data']['id']
        act('checkPlan')
        assert state('p().plan.checked')
        page.screenshot(path=str(OUT / 'unified-plan.png'))
    check('ST-S20-11', account)

    def plan_blocks():
        ready()
        fill_plan('#holding1', 100)
        act('makePlan')
        act('checkPlan')
        assert not state('p().plan.checked')
        assert any('可卖数量不足' in x for x in state('p().plan.issues'))
        assert any('可用现金不足' in x for x in state('p().plan.issues'))
        assert page.locator('[data-act="exportPlan"]').is_disabled()
        fill_plan('#holding1', 0)
        page.locator('#planSnapshot').select_option('PLAN-20260105')
        act('makePlan')
        act('checkPlan')
        assert any('快照与计划交易日' in x for x in state('p().plan.issues'))
        page.locator('#planSnapshot').select_option('PLAN-20260108')
        act('makePlan')
        act('checkPlan')
        assert state('p().plan.checked')
        fill_plan('#planCash', 9000)
        assert not state('p().plan.checked')
        assert page.locator('[data-act="exportPlan"]').is_disabled()
        act('makePlan')
        act('checkPlan')
        assert state('p().plan.checked')
        nav('data')
        act('updateData')
        task_done()
        nav('plan')
        assert page.locator('[data-act="exportPlan"]').is_disabled()
        assert not state('reportReady()')
    check('ST-S20-12', plan_blocks)

    def export_case():
        ready()
        act('makePlan')
        act('checkPlan')
        act('exportPlan')
        assert page.locator('#dialog').is_visible()
        assert '正式策略验证仍为证据不足' in page.locator('#dialog').inner_text()
        with page.expect_download() as event:
            act('confirmExport')
        event.value.save_as(OUT / 'unified-demo-plan.csv')
        csv = (OUT / 'unified-demo-plan.csv').read_text(encoding='utf-8-sig')
        for token in ['演示计划', '非交易指令', 'v2', 'PLAN-20260108', '2026-01-09', '900', '计划编号', '研究数据快照']:
            assert token in csv
        assert '没有发送任何订单' in page.locator('#messages').inner_text()
    check('ST-S20-13', export_case)
    browser.close()
print('通过数：', 13 - len(failures), '失败：', failures, flush=True)
raise SystemExit(bool(failures))
