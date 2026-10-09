## ADDED — 窄视口布局对照用例（desktop-window-bootstrap 提案，issue #7）

# 窄视口布局对照用例（desktop-window-bootstrap 提案，crates/research-desktop tests/gui_walkthrough.rs，reporter 入账）

原型 `@media(max-width:900px)` 口径：侧栏收成 65px 纯图标栏（品牌名、
section-label、导航文字隐藏）。桌面侧以 880px 无头渲染核对 slim 布局生效，
截图落盘 docs/research-desktop/screenshots/ 作对照证据。

| ID | 验收点 | 输入与步骤 | 必须断言 |
|---|---|---|---|
| UT-GUI-04 | S15-AC-02 | 880px 宽无头渲染需求页（无 LLM） | 侧栏按 Slim 计划 65px 图标栏渲染；布局不 panic、无水平溢出断言失败；截图落盘 docs/research-desktop/screenshots/ut-gui-04-narrow.png |

### reporter
纯本地渲染断言，无外部调用；结果经共享 reporter 写入 test-results.jsonl，
fail 含 error，skip 不可计为通过。
