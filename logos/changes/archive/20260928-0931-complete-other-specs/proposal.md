# 变更提案：complete-other-specs

> module: core | created: 2026-09-28

## 变更原因
基线提案 add-baseline-docs（已归档）补齐了需求/设计/技术/测试/接口/实现规格，
但 logos.config.json 声明的 8 类文档中 scenario/（JSON）与 database/（SQL）仍为空目录。
"其他规格"应完整覆盖全部声明的资源类别。

## 变更类型
需求级（纯规格补全，无代码实现）

## 变更范围
- scenario/：新增 8 个场景编排 JSON（ST-01~ST-08 的机器可读版本）
- database/：新增 00-no-database-decision.sql（纯注释 DDL 决策记录：引擎无自有数据库）
- API/测试/需求/设计/技术文档：无变化

## 部署影响
- 是否需要部署：否（纯文档）
- 影响环境：无 / 数据迁移：否 / 回滚：否 / smoke：否

## 变更概述
把 test/scenario 中的 ST-01~ST-08 端到端旅程转为机器可读编排规格（JSON），
供未来自动化执行器消费；database/ 以注释 SQL 记录"无自有数据库、backing 外置"
的架构决策，使该目录的空状态有据可查。
