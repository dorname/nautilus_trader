# RD-005 回测实验净值曲线仍缺失（域层未就绪）

- **类型**：功能缺口
- **优先级**：P2
- **状态**：open（已在 `desktop-fidelity-visuals` 提案中**有意缩减**，需独立跟进）
- **原型**：`equityChart()`（最近两次实验 polyline + 面积渐变 + 网格 + 悬停）
- **实现**：`render_experiments` 仅展示版本数 / 实验数 / 最近收益字符串与运行参数；**未使用 egui_plot 绘制净值曲线**

## 背景

归档提案 `logos/changes/archive/20261005-1840-desktop-fidelity-visuals/proposal.md` 写明：

> 桌面端实验执行委托研究协调器，仅回传汇总 `total_return` 字符串，  
> `domain::protocol::Comparison` 亦无逐日净值序列字段；绘制曲线需先扩展域层 API。

依赖 crate 中已有 `egui_plot`，但无序列数据源则无法对齐原型。

## 期望

1. 域层协议扩展：实验结果携带逐日（或采样）净值序列。  
2. 桌面 `render_experiments` 用 egui_plot 对齐原型最近两次对比语义（含「仅并列查看，不作代码效果归因」横幅）。  
3. 在未完成前，页面应显式占位：「净值曲线待域层序列字段就绪」，避免空白误以为已实现。

## 建议修复方向

新建变更提案「净值序列域层扩展 + 桌面曲线」，勿混入纯视觉保真提案。
