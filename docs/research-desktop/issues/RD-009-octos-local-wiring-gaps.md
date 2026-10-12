# RD-009 本地 octos 接线缺口（Web bundle / 默认 OCTOS_HOME / GitHub 鉴权）

- **类型**：环境 / 基建
- **优先级**：P1
- **状态**：resolved（2026-10-12 复测关闭，三项期望全部达成）
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

## 2026-10-12 复测：关闭结论

| 期望 | 结果 |
|------|------|
| 1. `OCTOS_HOME` 文档明确 | ✅ 本节即归档；Agent/沙箱环境一律用项目内 `OCTOS_HOME=<repo>/.local-octos` |
| 2. bundle 验收口径 | ✅ 按本条口径关闭（见下「bundle 根因与验收口径」） |
| 3. `gh` 鉴权 | ✅ 已恢复：`gh issue list / view` 正常（本复测评论即经 `gh` 提交） |

### bundle 根因与验收口径

- **根因定位**：admin dashboard（`crates/octos-cli/static/admin/`）与 octos-web SPA（`/app`）经 `rust_embed` 在**构建期烘焙进二进制**。当前 PATH 中的 `octos 2.0.3-rc.9` 是 bare-binary 安装——二进制内嵌提示原文：*"this looks like a bare-binary install; Deploy the full bundle (scripts/build-local-bundle.sh / scripts/install.sh), not just the octos binary."* 因此 `/` 与 `/app/` 只能回 503，运行时放文件无法补齐。
- **验收口径（按期望 2 的否则分支）**：research-desktop 交付**不依赖** octos App/Admin UI。`embed-octos-runtime` 提案（2026-10-12 归档，验收 Gate 3.6 PASS）后，desktop 经 `backend_ensure` 三级解析（内嵌资源 → `~/.octos/bin` → PATH）定位 octos，以 OUP（stdio JSON-RPC，`octos-ui/v1alpha1`）直连 `octos serve --stdio`，HTTP serve 端口不在接线路径上。接线验收以 **`serve` 可监听 + `doctor` 无 fatal + OUP stdio 闭环实证**为准——后者已由 ST-S17-17（真实 LLM 单路 turn）、ST-S18-17（工具可见性 + 托管工作区产物落盘）覆盖。
- **可选重建路径**（非交付必需）：octos 源码仓 `scripts/build-local-bundle.sh`（dashboard vite 构建 + `octos-web` 子模块 + cargo release 全量构建），产物 tarball 由 `scripts/install.sh` 自动识别安装。构建成本高（release 全量），仅在需要 App/Admin UI 时执行。

### 复测环境

WSL2 · `octos` v2.0.3-rc.9（`/root/.cargo/bin/octos`）· `gh` 已鉴权 · a-stock @ `034088cc2d`。
