# RD-007 原型 ≤900px 布局与窄视口对照证据

- **类型**：原型行为 / 核验证据
- **优先级**：P3
- **状态**：documented

## 在本次核验中的观察

使用 Cursor 内置浏览器打开本地原型（`python3 -m http.server 8766` →  
`core-05-ai-workspace-prototype.html`）时，视口宽度落入原型 `@media(max-width:900px)`：

- 侧栏变为约 65px **图标栏**（品牌名、section-label、导航文字隐藏）；
- 主区画布被严重挤压，出现纵向「项目产物 / 展开工作区」文字，对话栏占据绝大部分可视区域；
- 切换 `#overview` / `#data` / `#pool` 时，**无障碍树仍含正确页面标题与控件**（如股票池搜索、市场筛选），但视觉上主画布几乎不可读。

截图已存：

- `docs/research-desktop/screenshots/proto-overview.png`
- `docs/research-desktop/screenshots/proto-data.png`
- `docs/research-desktop/screenshots/proto-pool.png`

## 与原生桌面的关系

- 原生桌面最小宽 1100，**从不进入 900 断点**（见 RD-003）。
- 因此：窄视口截图 **不能** 作为「桌面 egui 与 1440 舒适档原型不一致」的直接像素证据；它们证明的是：
  1. HTML 原型在极窄视口下可用性差；
  2. 应用浏览器核验时必须强制 ≥1200（建议 1440）宽度。

## 期望

- 原型核验 SOP：视口 ≥1440×900 再截图归档。
- 评估是否弱化或删除 HTML 的 900px 断点（原生不做该档时，原型亦可只保留 1200）。
