# 原型截图采集工具：保存同尺寸对照与计算样式，不代替生产验收。
from pathlib import Path
import json
from playwright.sync_api import sync_playwright
root=next(parent for parent in Path(__file__).resolve().parents if (parent/"logos/logos.config.json").is_file())
out=root/"docs/research-desktop/screenshots/rd003"
records=[]
with sync_playwright() as p:
    browser=p.chromium.launch(headless=True,args=["--no-sandbox"])
    for w in [900,901,1200,1201]:
        page=browser.new_page(viewport={"width":w,"height":900},device_scale_factor=1)
        page.goto((root/"logos/resources/prd/2-product-design/2-page-design/core-05-ai-workspace-prototype.html").as_uri())
        page.locator('nav button').filter(has_text="需求文档").click()
        page.screenshot(path=str(out/f"prototype-{w}.png"))
        records.append(page.evaluate("""() => ({width:innerWidth, brand:getComputedStyle(document.querySelector('.brand-name')).display, gradient:getComputedStyle(document.querySelector('.brand-mark')).backgroundImage, glow:getComputedStyle(document.querySelector('.brand-mark')).boxShadow, overflow:document.documentElement.scrollWidth>innerWidth, active:document.querySelector('nav button.active').textContent})"""))
        page.close()
    browser.close()
(out/"prototype-layout.json").write_text(json.dumps(records,ensure_ascii=False,indent=2))
print(json.dumps(records,ensure_ascii=False))
