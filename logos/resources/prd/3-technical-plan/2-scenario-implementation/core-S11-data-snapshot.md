# S11 数据维护

## S11 数据维护

需求：FR-R01、FR-R02。前置：工作区已打开且持有写锁；所有引用版本存在。接口契约必须由下列消息推导。

### 场景时序
```mermaid
sequenceDiagram
    participant G as Rust界面
    participant C as 应用协调器
    participant M as 元数据存储
    participant W as 后台工作进程
    participant E as 回测引擎
    G->>C: ImportData（源、口径、辅助文件）
    C->>M: 保存 queued 任务
    C->>W: 导入暂存数据
    W->>C: 进度与质量报告
    C->>W: 校验分区和能力
    W->>C: 产物清单
    C->>M: 提交快照和 succeeded
    C->>G: SnapshotReady
    alt 校验或运行失败
        C-->>G: 错误码与修复入口
    else 用户取消
        G->>C: CancelTask
        C->>W: 取消并等待退出
        C-->>G: 终态或已完成
    end
```

### 输入输出与后置条件
输入目录／源配置、价格口径、辅助数据；输出 snapshot_id、覆盖范围、质量与限制。重复导入相同内容复用内容哈希；冲突先保留报告再按同口径优先规则处理。

### 失败与取消
解析错误或磁盘不足→failed；网络失败允许保留原快照，不能把部分数据提交为完整快照；取消删除未提交暂存文件。
通用任务遵循架构状态机；写入只在最终提交后可见，页面切换不取消任务。CancelTask 只作用于尚未完成的后台任务，比较查询等同步操作返回前完成则返回 ALREADY_TERMINAL。

## 查询与保存子流程

```mermaid
sequenceDiagram
    participant G as Rust界面
    participant C as 协调器
    participant M as 元数据与内容存储
    G->>C: GetTask
    C->>M: 按last_seq返回状态
    M-->>C: 对象与版本
    C-->>G: 类型化响应或错误
    G->>C: ListSnapshots
    C->>M: 查询已提交快照
    M-->>C: 对象与版本
    C-->>G: 类型化响应或错误
```
