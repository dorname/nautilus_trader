# 临时技能导出

`code-tour-builder/` 是本机技能的完整可安装副本，包含技能说明、数据契约、构建器与验证器；不包含 Python 缓存。此目录用于跨机器迁移。

## 在其他机器安装到 Codex

在仓库根目录执行：

```bash
mkdir -p ~/.codex/skills
cp -R skill-exports/code-tour-builder ~/.codex/skills/
```

若设置了 `CODEX_HOME`，请将目标目录改为 `$CODEX_HOME/skills/`。若已有同名技能，先备份旧目录再复制。安装后新建 Codex 会话，以读取技能。

使用提示词：`请使用 code-tour-builder 为当前仓库生成交互式源码导读。`

原技能文档中的构建与验证示例使用 Cursor 安装路径。在 Codex 中对应命令为：

```bash
python3 ~/.codex/skills/code-tour-builder/scripts/build_code_tour.py docs/code-tour
python3 ~/.codex/skills/code-tour-builder/scripts/validate_code_tour.py docs/code-tour
```

脚本仅使用 Python 标准库；验证器在有 Node.js 时额外检查 JavaScript 语法。也可将同一技能目录复制到 `~/.cursor/skills/`。
