# RD-009 本地 octos 接线缺口（Web bundle / 默认 OCTOS_HOME / GitHub 鉴权）

- **类型**：环境 / 基建
- **优先级**：P1
- **状态**：open（2026-10-11 本地闭环演练记录）
- **GitHub**：[#34](https://github.com/dorname/nautilus_trader/issues/34)
- **复现环境**：WSL2 · Cursor Agent · `octos` v2.0.3-rc.9 · `octoscode` 0.3.0-rc.9

## 现象

| 项 | 结果 |
|----|------|
| `octos init --cwd <repo> --defaults --force` | 成功，写出仓库 `.octos/config.json` |
| `OCTOS_HOME=<repo>/.local-octos octos doctor` | 22 passed / 5 warning；config/data dir 可写 |
| 默认 `~/.octos` / `~/.config/octos` | 在 Cursor Agent 沙箱中 **不可写**（`octos doctor` 报 fatal）；需改用项目内 `OCTOS_HOME` |
| `octos serve --host 127.0.0.1 --port 50081 --data-dir .local-octos` | 进程可监听；`/` 返回 `admin_bundle_missing`；`/app/` 返回 `web_bundle_missing` |
| `herdr agent list` | `server_not_running`（无 herdr.sock） |
| `gh issue create` / `gh api` | GitHub token 失效（`authentication failed` / Forbidden） |

## 期望

1. 文档明确：Agent/沙箱环境用 `OCTOS_HOME=<repo>/.local-octos`，勿依赖 `~/.octos` 可写。
2. 若交付依赖 octos App/Admin UI，需随安装包或重建脚本产出 web/admin bundle；否则接线验收以 `serve` 监听 + `doctor` 无 fatal 为准。
3. 外环/提 Issue 前完成 `gh auth login`（或有效 `GH_TOKEN`）。

## 本次已完成的接线

```bash
OCTOS_HOME=/home/kyle/nautilus_trader/.local-octos
octos init --cwd /home/kyle/nautilus_trader --defaults --force
OCTOS_HOME=$OCTOS_HOME octos doctor   # No fatal issues
OCTOS_HOME=$OCTOS_HOME octos serve --cwd /home/kyle/nautilus_trader \
  --host 127.0.0.1 --port 50081 --data-dir "$OCTOS_HOME"
```

## 证据

- doctor 摘要：`No fatal issues found`（项目 `OCTOS_HOME`）
- serve 响应：`admin_bundle_missing` / `web_bundle_missing`
- gh：`The github.com token in /root/.config/gh/hosts.yml is no longer valid`
