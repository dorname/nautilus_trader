---
name: code-tour-builder
description: 为大型代码库生成或优化证据优先、图优先的交互式 Code Tour / Code Explorer，并构建可离线打开的单文件 HTML。用于源码导读、架构图、依赖图、执行流、模块探索器和阅读路线等任务。
---

# Code Tour Builder

把“代码分析”转换为开发者可逐层钻取的源码学习界面，而不是统计报告。

## 默认交付

输出 `<repo>/docs/code-tour/index.html`：

- 单 HTML 文件，可离线打开
- CSS、JavaScript、JSON 数据全部内嵌
- 不依赖后端或外部 CDN
- 图优先，支持系统 → 模块 → 执行流 → 文件 → symbol
- 重要关系附 evidence 与 `verified | partial | unverified`

除 `index.html` 外，可保留可维护源文件：

```text
docs/code-tour/
├── tour.config.json
├── tour-data.json
├── tour-app.js
├── tour.css
├── build.py
├── validate.py
└── index.html
```

## 工作流

### 1. 定位与约束

1. 确认仓库根目录、目标分支和输出路径。
2. 读取仓库规则、workspace/package manifest 和主 README。
3. 识别核心代码范围；默认排除生成物、vendor、测试夹具、图片和大体量非核心前端。
4. 已有 Code Tour 时先验证现状，再决定增量修改或重建。

### 2. Evidence First

按优先级获取事实：

1. 代码知识图谱 / GRASP / 静态调用图
2. workspace/package manifest
3. README、架构文档、协议文档
4. 少量关键源码

不要按文件名、crate 名或 symbol 名猜关系。

每个关键节点至少记录：

```json
{
  "crate": "module-name",
  "file": "relative/path",
  "symbol": "optional_symbol",
  "responsibility": "一句话职责",
  "verification": "verified",
  "evidence": "事实来源"
}
```

无法确认 symbol 或调用跳转时留空并标记 `partial` / `unverified`。

### 3. 先建数据模型

先写 `tour-data.json`，再写视图。数据契约见 [reference.md](reference.md)。

最小顶层结构：

```json
{
  "meta": {},
  "overviewNodes": [],
  "overviewEdges": [],
  "modules": [],
  "flows": {},
  "readingPath": []
}
```

至少建立：

- 系统边界与核心组件
- 核心模块直接依赖
- 一条主执行链
- 推荐阅读路线

### 4. 图表设计

#### 系统架构图

- 用泳道表达层级或边界。
- 稳定颜色语义：Protocol / Runtime / Storage / Extension / Host / External。
- 至少区分主调用链、支撑关系、编排入口、规划中节点。
- 规划中或未落地模块使用虚线，不能画成已运行组件。

#### 依赖图

- 仅显示核心模块。
- 箭头定义必须写在页面上，例如 “A → B 表示 B 直接依赖 A”。
- 默认显示核心主链；提供完整直接依赖切换。
- 点击模块后同时突出：
  - dependencies：流入选中节点
  - dependents：从选中节点流出
- 选中节点的直接关系必须显示，即使不属于默认主链。

#### 执行流

- 节点按真实调用顺序排列。
- 工具调用、重试和循环使用显式回边。
- 节点详情显示 crate、file、symbol、上下游和 evidence。

不要使用无方向卡片堆叠、不可读 force graph 或大量 KPI。

### 5. Progressive Explorer

默认包含：

1. Overview
2. Architecture
3. Execution Flow
4. Module Explorer
5. Reading Path
6. Flow Library

实现：

- 节点点击与详情面板
- dependencies / dependents 高亮
- Flow 切换
- Module → Flow
- Flow Node → Source Detail
- 搜索 crate / file / symbol
- 返回上一级
- 状态保持
- 图缩放与窄屏滚动

### 6. 构建

将本 Skill 的脚本复制或包装到目标目录：

```bash
python ~/.cursor/skills/code-tour-builder/scripts/build_code_tour.py \
  docs/code-tour
```

构建器读取：

- `tour.config.json`
- `tour-data.json`
- `tour-app.js`
- `tour.css`

并确定性写出 `index.html`。

禁止把未经转义的 `</script>` 注入 HTML。

### 7. 验证

每次构建后执行：

```bash
python ~/.cursor/skills/code-tour-builder/scripts/validate_code_tour.py \
  docs/code-tour
```

验证器必须检查：

- 数据引用完整：节点、边、模块依赖、flow 边
- verification 枚举合法
- 关键视图与交互钩子存在
- `index.html` 内嵌 CSS / JS / 数据
- 无外部脚本、样式和图片依赖
- JavaScript 语法（环境有 Node.js 时）
- 生成物与源文件一致

### 8. 浏览器验收

使用本地 HTTP 服务打开页面，不用 `file://` 自动化：

```bash
python -m http.server 8765 --directory docs/code-tour
```

至少验证：

1. 首屏架构图可见且关系清楚。
2. 依赖图主链/完整模式可切换。
3. 点击核心模块后上下游颜色不同。
4. Flow 切换和节点详情有效。
5. 搜索可定位 crate / file / symbol。
6. 1440×900 与窄屏均无页面级横向溢出。
7. 页面没有运行时 JavaScript 异常。

停止本地预览服务后再交付。

## 完成标准

只有同时满足以下条件才算完成：

- 当前分支和分析范围正确
- 关键架构与调用关系有证据
- 数据验证通过
- 单文件构建通过且可重复
- JavaScript 语法通过
- 浏览器交互与响应式布局通过
- 首屏回答“系统如何工作”，而不是“仓库有多少文件”

