## ADDED — S13 策略研究
需求：FR-R05、FR-R06、FR-R07、FR-R10。前置：工作区已打开且持有写锁；所有引用版本存在。接口契约必须由下列消息推导。

### 场景时序
```mermaid
sequenceDiagram
    participant G as Rust界面
    participant C as 应用协调器
    participant M as 元数据存储
    participant W as 后台工作进程
    participant E as 回测引擎
    G->>C: SubmitRun（配置和版本引用）
    C->>M: 写入排队任务与实验
    C->>W: 预检并逐日运行
    W->>E: 初始化引擎／开盘执行／收盘估值
    E->>W: 成交／账户／拒单事件
    W->>C: 进度与结果清单
    C->>M: 提交结果及终态
    C->>G: RunCompleted
    alt 校验或运行失败
        C-->>G: 错误码与修复入口
    else 用户取消
        G->>C: CancelTask
        C->>W: 取消并等待退出
        C-->>G: 终态或已完成
    end
```

### 输入输出与后置条件
输入策略、窗口、调仓、成本／规则、区间和快照；输出 run_id、指标、逐日净值、成交、限制及版本清单。网格生成最多100个独立子运行；每个配置哈希唯一，预热不计入收益。

### 失败与取消
预检失败不启动引擎；引擎错误→failed；取消在交易日边界响应，超时终止专属工作进程；崩溃→interrupted，重试创建新run。
通用任务遵循架构状态机；写入只在最终提交后可见，页面切换不取消任务。CancelTask 只作用于尚未完成的后台任务，比较查询等同步操作返回前完成则返回 ALREADY_TERMINAL。

## ADDED — 查询与保存子流程
```mermaid
sequenceDiagram
    participant G as Rust界面
    participant C as 协调器
    participant M as 元数据与内容存储
    G->>C: GetTask
    C->>M: 返回状态与已持久化最后事件序号
    M-->>C: 对象与版本
    C-->>G: 类型化响应或错误
```
