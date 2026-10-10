## ADDED — 视觉保真验收口径

生产桌面（egui）相对本原型（CSS）的视觉验收按两张表执行：**必须像素/交互对齐**项不一致即验收不通过；**平台近似可接受**项以近似口径为准，不得假装像素级一致，也不得再作为缺口提出。

### 必须像素 / 交互对齐

| 原型能力 | 验收要求 | 实现锚点 |
|---|---|---|
| `.brand-mark`：29×29、圆角 9、145° 线性渐变 `#4ade80→#16a34a`、外圈辉光、字色 `#03130a` | 渐变角度与起止色一致；辉光可见；「研」字居中 | `theme::paint_brand_mark` |
| `.nav button.active:before`：2px 绿色指示条 + 辉光 | 激活态左缘指示条 + 多层辉光；非激活无 | `app::nav_item` |
| `@media max-width:1200px` / `900px` 断点 | 三档布局：舒适档侧栏 212/对话栏 360；窄档（≤1200）178/320；图标档（≤900）侧栏 65 纯图标、隐藏品牌名与计数 | `layout::plan_for_width`（UT-S15-08） |
| 文字四层级 + 等宽数字/标签 | 颜色层级与等宽字体使用一致 | `theme::TEXT/MUTED/FAINT`、等宽 FontId |
| 键盘焦点清晰、文字状态不只依赖颜色 | 焦点环可见；状态同时有图标/文字承载 | 各交互控件 |

### 平台近似可接受（egui 即时模式无对应 CSS 原语）

| 原型能力 | 近似口径 | 代码注释位置 |
|---|---|---|
| `backdrop-filter: blur(…)` 真实毛玻璃 | 半透明面板色 + 顶部高光描边 | `theme.rs` 模块头 |
| 双径向环境柔光 | 角点色网格线性插值 | `theme::paint_ambient` |
| `box-shadow` 导航辉光 / 品牌 glow | 多层透明度递减叠加 | `app::nav_item` / `theme::paint_brand_mark` |
| `prefers-reduced-motion:reduce` 关闭动画 | 当前实现无动画/过渡，该项不适用；未来引入动画时必须尊重系统减少动态效果偏好 | 本节即口径 |

### 关闭声明

- **浅色主题**：不作要求。core-02「默认浅色」口径已在变更 desktop-issue-docs-rd002-rd004 中作废，core-05「纯黑科技 v3」深色为唯一视觉权威。
- **中文字体随包**：已满足。`assets/NotoSansSC-Regular.otf` 经 `include_bytes!` 内嵌为兜底，系统 Noto/PingFang 探测优先；缺失时状态行告警路径保留（`theme.rs` 字体安装段）。
