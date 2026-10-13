## ADDED — 路由画布状态隔离回归用例（fix-issues-40-41-20261013 / issue #40）

| ID | 验收点 | 输入与步骤 | 必须断言 |
|---|---|---|---|
| UT-S15-13 | S15-AC-02 | 无 LLM：源码静态检查画布滚动容器与文本编辑控件的 ID 命名空间；egui_kittest 需求页→回测实验页往返 | `render_canvas` 的 ScrollArea id_salt 含路由标识（禁止全路由共用裸 `"canvas-scroll"`）；画布内多行 TextEdit 显式指定路由命名空间下唯一 id（禁止依赖默认位置 ID）；往返切换后 AccessKit 树中实验页不含需求草稿 marker 文本（防御性断言——泄漏依赖真实 GPU 渲染路径时以静态检查为准） |

### reporter

纯本地断言（静态扫描 + 无头渲染，无外部调用）；结果经共享 reporter 写入
test-results.jsonl，fail 含 error，skip 不可计为通过。
