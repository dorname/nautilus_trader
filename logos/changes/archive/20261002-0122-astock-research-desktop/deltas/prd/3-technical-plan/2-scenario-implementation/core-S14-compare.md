## ADDED — S14 回测比较
需求：FR-R08。前置：工作区已打开且持有写锁；所有引用版本存在。接口契约必须由下列消息推导。

### 场景时序
```mermaid
sequenceDiagram
    participant G as Rust界面
    participant C as 应用协调器
    participant M as 元数据存储
    participant W as 后台工作进程
    participant E as 回测引擎
    G->>C: CompareRuns（运行ID与区间视图）
    C->>M: 读取已完成清单
    C->>W: 核验结果哈希并计算可比性
    W->>C: 指标、曲线和差异
    C->>G: CompareReady
    alt 校验或运行失败
        C-->>G: 错误码与修复入口
    else 用户取消
        G->>C: CancelTask
        C->>W: 取消并等待退出
        C-->>G: 终态或已完成
    end
```

### 输入输出与后置条件
输入2～5个已完成run_id和完整／交集模式；输出按各自条件标注的指标与曲线，交集视图另列起止日。基准缺失保留策略指标，超额值为空。

### 失败与取消
未完成运行→RUN_NOT_READY；结果哈希异常→CORRUPT_ARTIFACT；区间无交集→NO_OVERLAP；不能静默忽略失败运行。
通用任务遵循架构状态机；写入只在最终提交后可见，页面切换不取消任务。CancelTask 只作用于尚未完成的后台任务，比较查询等同步操作返回前完成则返回 ALREADY_TERMINAL。

## ADDED — 查询与保存子流程
```mermaid
sequenceDiagram
    participant G as Rust界面
    participant C as 协调器
    participant M as 元数据与内容存储
    G->>C: QueryRows
    C->>M: 读取已校验结果的分页明细
    M-->>C: 对象与版本
    C-->>G: 类型化响应或错误
```
