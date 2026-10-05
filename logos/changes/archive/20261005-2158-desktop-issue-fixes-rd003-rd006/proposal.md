# 变更提案：desktop-issue-fixes-rd003-rd006

> module: core | created: 2026-10-05

## 变更原因

仓库 issue 清单（`docs/research-desktop/issues/`，GitHub #1–#7）中两个
代码可修复项——**RD-003**（egui 与 core-05 原型视觉缺口）与 **RD-006**
（footer 泄漏「HTML 原型」文案语义）合并修复：

- RD-003.1 品牌 mark：原型 145° 线性渐变（#4ade80→#16a34a）+ 辉光，
  实现为单色填充
- RD-003.2 导航激活指示：原型 2px 绿条 + 10px 辉光，实现仅实线
- RD-003.3 ≤900px 断点：原型收成 65px 图标栏（隐藏文字/计数/section-label）
- RD-003.5 中文字体「随包提供」：现为系统探测 + 缺失告警，与 core-02 承诺不符
- RD-006 footer：「本地原型 · 刷新重置」拷贝自 HTML 原型，原生桌面语义不符

RD-001（WSLg 启动）已由 desktop-firstframe-guard + desktop-root-layout 修复归档；
RD-002/004（文档口径）与 RD-005（域层净值序列）不在本提案代码范围。

## 变更类型
代码级

## 变更范围
- 影响的需求文档：无
- 影响的功能规格：无（权威原型规格不变，实现/文案对齐）
- 影响的业务场景：无（S15 桌面实现修正）
- 影响的 API：无
- 影响的 DB 表：无
- 影响的编排测试：无（补测并入既有 UT-S15-08，不新增用例 ID）

## 部署影响
- 是否需要部署：否
- 部署原因：本地原生桌面应用；本地构建 + 截图目检 + verify 覆盖
- 影响环境：无
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：否

## UI/UX 变更声明

```yaml
ui_impact: true             # 品牌渐变/导航辉光/≤900 图标栏/footer 文案对齐原型
design_system_mode: generated
design_system_fallback_reason: ""
pages:
  - id: S15
    prototype: core-05-ai-workspace-prototype.html
    description: 品牌 mark 渐变+辉光、导航激活辉光、≤900px 图标栏断点、footer 原生文案
```

## 变更概述

1. **品牌 mark 渐变+辉光**（app.rs）：品牌块由单色改 painter 145° 顶点
   插值 Mesh（#4ADE80→#16A34A），外加 14px 半透明辉光环
   （rgba(34,197,94,.35)）。
2. **导航激活辉光**（app.rs nav_item）：2px 指示条外加 10px 高斯近似辉光
   （三层透明度递减半透明条）。
3. **≤900 图标栏断点**（layout.rs + app.rs）：`LayoutPlan` 增 `Slim` 档
   （≤900：侧栏 65，隐藏品牌名/section-label/计数，导航项只显图标）；
   `plan_for_width` 三段分流；UT-S15-08 补断点与 Slim 侧栏宽断言。
4. **中文字体随包**（theme.rs + 构建）：把仓库内可再分发的 CJK 字体
   （优先 `assets/fonts/` 下已有 Noto Sans CJK 子集，否则回退系统探测）
   经 `include_bytes!` 嵌入，系统探测失败时用内嵌字体兜底，消除
   「字体缺失告警」路径；补 UT 断言内嵌字体非空。
5. **footer 文案**（app.rs）：改为「● 本地研究桌面 · 合成/导入样本
   可追溯 · 结果绑定版本与快照」，移除「原型」「刷新重置」HTML 语义；
   UT 补文案断言。
