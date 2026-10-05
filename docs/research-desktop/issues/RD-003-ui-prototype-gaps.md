# RD-003 egui 实现与 core-05 原型视觉/交互缺口

- **类型**：UI 保真
- **优先级**：P1
- **状态**：open
- **权威原型**：`core-05-ai-workspace-prototype.html`
- **实现**：`theme.rs` / `layout.rs` / `icons.rs` / `app.rs`
- **已归档相关**：`desktop-fidelity-visuals`（图标 / hero 双环 / empty 图标已补；净值曲线明确缩减）

## 已知「验收口径允许」的近似（需在交付说明中显式保留）

| 原型能力 | egui 现状 | 代码注释位置 |
|----------|-----------|--------------|
| `backdrop-filter: blur(…)` 真实毛玻璃 | 半透明面板色 + 描边近似 | `theme.rs` 模块头 |
| 双径向环境柔光 | 角点色网格线性插值 | `theme::paint_ambient` |
| CSS `box-shadow` 导航辉光 / 品牌 glow | 基本缺失或仅实线指示条 | `nav_item` / 品牌 mark |

这些属于平台能力差，**不应假装已像素级一致**，但应在验收文档中写明「近似口径」。

## 仍构成不一致的缺口

1. **品牌 mark**  
   原型：`linear-gradient(145deg,#4ade80,#16a34a)` + `box-shadow` 辉光。  
   实现：单色 `ACCENT` 填充圆角块。

2. **导航激活指示**  
   原型：2px 绿条 + `box-shadow: 0 0 10px var(--accent-glow)`。  
   实现：2px 实线，无辉光。

3. **≤900px 断点**  
   原型：侧栏收成 65px 图标栏，隐藏品牌名 / section-label / count。  
   实现：仅 Comfortable(>1200) / Compact(≤1200)，最小窗 1100 仍显示文字侧栏。

4. **`prefers-reduced-motion`**  
   原型 CSS 关闭动画/过渡。  
   实现：无对应系统偏好尊重（若后续加动画需补）。

5. **中文字体「随包提供」**（core-02 / 设计期望）  
   实现：从系统路径探测加载；缺失时状态行告警。  
   与「随包提供」承诺不符（可作为交付诚实边界或补打包字体）。

6. **浅色主题切换**  
   见 RD-002；实现无 light 路径。

## 期望

- 列出「必须像素/交互对齐」vs「平台近似可接受」两张表，并入验收规格。
- P1 建议优先：品牌渐变 mark、≤900（或明确声明原生桌面不做该断点）、字体随包或改写规格。

## 证据

- 原型 CSS：`.brand-mark`、`.nav button.active:before`、`@media(max-width:900px)`。
- 实现：`theme.rs`、`layout.rs`（无 900 断点）、`app.rs::render_sidebar` / `nav_item`。
