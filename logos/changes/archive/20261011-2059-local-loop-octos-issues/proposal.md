# 变更提案：本地启动 / octos 接线闭环问题归档

## 变更原因

2026-10-11 执行「本地启动 + 接线 octos + 业务闭环」演练：CLI 研究链路（S21）已全链路跑通；桌面 GUI 在 Agent 沙箱/WSLg 下仍无法稳定验收窗口与截屏；octos 需项目内 `OCTOS_HOME` 才能 doctor 通过，serve 缺 web/admin bundle；GitHub `gh` 鉴权失效无法远程建 Issue。需把缺口落入 `docs/research-desktop/issues/` 本地归档，待鉴权恢复后同步 GitHub。

## 变更类型

代码级（文档归档 / 问题跟踪；不改业务代码、不改 logos/resources 规格）

## 变更范围

- 影响的需求文档：无
- 影响的功能规格：无
- 影响的业务场景：无（演练覆盖 S21 CLI 链路，不改场景规格）
- 影响的部署方案：无（可参考 core-03 本地启动前置说明，本提案不改）
- 影响的 API：无
- 影响的数据库：无
- 影响的测试用例：无
- 影响的代码：无
- 影响的文档：`docs/research-desktop/issues/`（新增 RD-009/RD-010，更新 README 与 RD-001 复测注记）

## 部署影响分析

- 是否需要部署：否
- 是否需要数据迁移：否
- 是否需要 smoke 验证：否

## 不在范围内

- 修复 WSLg / wgpu 显示合成
- 重建 octos web/admin bundle
- 恢复 `gh` 鉴权（需人类登录）
- 修改研究域或桌面业务代码
