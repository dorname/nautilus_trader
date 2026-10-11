# Code Tour 数据契约

本文定义 `tour-data.json` 的通用结构。允许增加字段，但不要改变字段语义。

## meta

```json
{
  "project": "项目名",
  "branch": "目标分支",
  "commit": "分析时提交",
  "analysisSession": "可选知识图谱 session",
  "sources": ["knowledge graph", "manifest", "README", "source"],
  "note": "验证策略"
}
```

## overviewNodes

系统架构节点：

```json
{
  "id": "runtime",
  "label": "Agent Runtime",
  "layer": "runtime",
  "responsibility": "执行 agent turn",
  "modules": ["agent-runtime"],
  "keyFiles": ["src/runtime.rs"],
  "keySymbols": ["Runtime::run"],
  "dependsOn": ["llm"],
  "usedBy": ["host"],
  "readingPriority": 2,
  "verification": "verified",
  "evidence": "source: src/runtime.rs"
}
```

约束：

- `id` 全局唯一。
- `layer` 使用稳定语义：`protocol | runtime | storage | extension | host | external`。
- `dependsOn` / `usedBy` 引用 overview node id。
- 未确认 symbol 时使用空数组，不能编造。

## overviewEdges

简单形式：

```json
["client", "protocol"]
```

扩展形式：

```json
{
  "from": "client",
  "to": "protocol",
  "kind": "main",
  "label": "request"
}
```

`kind` 推荐值：

- `main`：主调用链
- `support`：运行时支撑
- `orchestration`：编排入口
- `planned`：计划中或 scaffold

## modules

模块、crate 或 package：

```json
{
  "id": "agent-runtime",
  "role": "runtime",
  "responsibility": "Agent loop 与工具执行",
  "deps": ["core", "llm"],
  "dependents": ["cli-host"],
  "keyFiles": ["src/loop.rs"],
  "keySymbols": ["run_turn"],
  "entryPoints": ["src/lib.rs"],
  "modules": ["loop", "tools", "context"],
  "evidence": "manifest + knowledge graph"
}
```

约束：

- `deps` 必须来自 manifest 或可验证构建依赖。
- `dependents` 应由 `deps` 反推；若手写，验证器检查双向一致性。
- 页面中的依赖箭头统一从 dependency 指向 dependent。

## flows

```json
{
  "agent-turn": {
    "id": "agent-turn",
    "title": "Agent Turn",
    "priority": "P0",
    "summary": "一次 turn 的主执行链",
    "nodes": [
      {
        "id": "turn-1",
        "label": "Dispatch request",
        "crate": "host",
        "file": "src/transport.rs",
        "symbol": "dispatch",
        "responsibility": "分发请求",
        "verification": "verified",
        "evidence": "source call site"
      }
    ],
    "edges": [["turn-1", "turn-2"]]
  }
}
```

约束：

- Flow 内 node id 唯一。
- 每条边的端点必须属于当前 Flow。
- `verification` 只能是 `verified | partial | unverified`。
- 回环允许，但页面必须以可识别的回边呈现。
- 无证据的跳转不能为 `verified`。

## readingPath

```json
{
  "id": "step-1",
  "module": "core",
  "why": "先掌握共享类型",
  "whatToRead": ["src/types.rs"],
  "whatToUnderstand": ["核心类型边界"],
  "skip": ["平台适配细节"]
}
```

阅读路线宜为 5–10 步，每一步控制在少量文件和 symbol。

## tour.config.json

```json
{
  "title": "Project Code Tour",
  "lang": "zh-CN",
  "brand": "PROJECT",
  "accent": "Code Tour",
  "subtitle": "branch · offline",
  "searchPlaceholder": "搜索 module / file / symbol…",
  "detailHeading": "详情",
  "detailEmpty": "点击图中的节点，查看职责、文件、符号与证据。"
}
```

构建器只负责 HTML 外壳与安全内嵌。业务视图由 `tour-app.js` 读取 `TOUR` 数据渲染。

## 验证强度

### 必须失败

- JSON 不合法
- 重复 id
- 悬空边或悬空依赖
- 非法 verification
- 缺少四个源文件
- 生成物包含外部 `<script src>`、stylesheet 或远程图片
- JavaScript 语法失败
- `index.html` 与源文件不一致

### 警告

- 重要节点缺 evidence
- Flow 全部为 partial/unverified
- Reading Path 超过 12 步
- 默认依赖图节点超过 30 个
- 首屏缺少系统架构图

