## MODIFIED — S15 测试用例

# S15 测试用例

## S15 测试用例

UT-S15-01～04、ST-S15-01 已由库验收（research-domain/research-worker）真实执行并通过
OpenLogos reporter；UT-S15-05/06 性能量测与 ST-S15-02/03 双平台 GUI 旅程待桌面应用
落地（astock-desktop-launch 提案批次 L3）后复评——性能量测仍需参考机环境（保留 skip
语义，skip 不可计为通过），GUI 渲染旅程由 crates/research-desktop 承载后按实际
可执行性诚实上报或人工验收。

| ID | 验收点 | 输入与步骤 | 必须断言 |
|---|---|---|---|
| UT-S15-01 | S15-AC-01 | 参考总资产10000、目标权重0.2、价10、持100股可卖100、步长100 | 目标200股、建议买100股，参考额1000；不产生券商调用 |
| UT-S15-02 | S15-AC-01 | 可卖量200超过持有100，或现金为负 | 拒绝生成且定位字段 |
| UT-S15-03 | S15-AC-01 | 名称以=开头、目录含中文空格、目标文件已存在 | CSV自由文本转义；路径正确；未确认覆盖时PATH_CONFLICT |
| UT-S15-04 | S15-AC-01 | as_of早于最近已结束交易日 | 默认STALE_DATA；显式确认后计划和CSV含历史计划标识 |
| ST-S15-01 | S15-AC-01 | GeneratePlan→GetTask→ExportPlan→SaveManualNote | 导出哈希一致、股数正确、数据版本和限制齐全；备注不更改回测fills |
| ST-S15-02 | S15-AC-02 | Windows GUI用固定数据完成五页、取消、重开、中文输入和150%缩放 | 业务结果与F-RUN一致，无自动下单入口，附运行日志和截图 |
| ST-S15-03 | S15-AC-02 | Linux Wayland／X11分别执行同旅程 | 两会话均可输入／导入／运行／导出，结果与Windows一致，附证据 |

### reporter
使用共享reporter写入真实断言结果；fail含error，skip不可算通过。详见 core-09-research-test-cases.md。

## 性能验收用例

| ID | 验收点 | 输入与步骤 | 必须断言 |
|---|---|---|---|
| UT-S15-05 | S15-AC-02 | Windows参考机和需求所列数据规模 | 冷启动≤5秒、输入p95≤100ms、预览≤10秒、回测≤60秒、GUI≤512MiB、worker≤8GiB，记录实测 |
| UT-S15-06 | S15-AC-02 | Linux参考机和相同数据规模 | 同上，分别保存环境及实测数据，不能复用Windows结果 |
