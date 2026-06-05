# cc-status 中文使用说明

cc-status 是给 [Claude Code](https://docs.claude.com/en/docs/claude-code) 用的状态栏工具。一个 Rust 二进制，没有守护进程，平均 ~20ms 渲染一次。

设计哲学：**状态栏被动展示信号，详情面板按需打开**。状态栏只放你每次都该看到的关键数字；想看清细节、不懂某个图标，再用命令展开。

---

## 1. 安装与接入

> **重要：`npx` 是"试用"，不是"安装"**。`npx` 跑完命令就走，不会把 `ccs` 留在你的 PATH 里。Claude Code 的状态栏每次刷新都要执行 `ccs render`，所以你需要**长期可用**的安装方式 —— 用 `npm install -g`、`curl`、或 Homebrew，**不要**靠 npx。

### 1.1 npm 全局安装（推荐，最快）

只要你有 Node.js（用 Claude Code 就有）：

```sh
npm install -g @cc-status-line/cli
ccs --version
```

如果默认 npm registry 慢（淘宝镜像同步可能滞后），加 `--registry`：

```sh
npm install -g --registry=https://registry.npmmirror.com @cc-status-line/cli
```

或者切到官方：

```sh
npm install -g --registry=https://registry.npmjs.org @cc-status-line/cli
```

如果只是**想试一下**（不长期装），用 npx：

```sh
npx -y @cc-status-line/cli --version
```

但试完 `ccs` 就**不在 PATH 了**，要用 Claude Code 的状态栏还是得走上面的 `-g` 安装。

### 1.2 curl 安装脚本

不想装 npm 包，直接拉 GitHub Release tarball：

```sh
curl -fsSL https://raw.githubusercontent.com/hankeGui/cc-status/main/install.sh | sh
```

装到 `~/.local/bin/ccs`。用 `--bin-dir` 自定义路径、`--version` 装指定版本。

### 1.3 Homebrew

```sh
brew install hankeGui/tap/ccs
```

### 1.4 手动下载

从 [Releases](https://github.com/hankeGui/cc-status/releases) 选对应平台的 tarball，解压，把 `ccs` 放到 PATH 上任意目录。

### 1.5 从源码编译

需要 Rust 工具链。

```sh
cargo install --git https://github.com/hankeGui/cc-status --locked
```

> **注意**：`cargo install` 把二进制放在 `~/.cargo/bin/ccs`。如果你的 PATH 里没有 `~/.cargo/bin`（rustup 安装时通常会加，但有些手动配置的 shell 没加），加上：
>
> ```sh
> export PATH="$HOME/.cargo/bin:$PATH"
> ```
>
> 即使 PATH 没加，`ccs setup` 也会写绝对路径到 settings.json，所以 Claude Code 仍能找到。

或者克隆仓库自己 build：

```sh
git clone https://github.com/hankeGui/cc-status
cd cc-status
cargo build --release
cp target/release/ccs ~/.local/bin/
```

### 1.6 接到 Claude Code

**推荐：用 `ccs setup` 自动配置**

```sh
ccs setup
```

它会：
1. 检查 `~/.claude/settings.json` 是否存在 / 合法
2. 显示要做的改动（让你预览）
3. 问 y/N 确认
4. 备份原文件（`settings.json.bak-<时间戳>`）后写入

参数：
- `--yes` 跳过确认（脚本里用）
- `--check` 只报告状态、不改动
- `--uninstall` 移除 statusLine 配置

**或者手动改** `~/.claude/settings.json`：

```json
{
  "statusLine": {
    "type": "command",
    "command": "/Users/YOU/.local/bin/ccs render"
  }
}
```

**用绝对路径**。Claude Code 的状态栏 shell 不一定继承你登录 shell 的 PATH。

重启 Claude Code，状态栏顶部就会出现：

```
~/hanke-dev/cc-status main  Claude Opus 4.7  ctx 86% █████▏ 154.6k/950k
```

---

## 2. 三种交互方式

### 2.1 状态栏（被动）

每次提问刷新一次，没有交互。看一眼就走。

### 2.2 详情面板：`ccs status`

在终端任何地方都能跑：

```sh
ccs status
```

输出一个完整的会话仪表盘，每个数字旁边都有解释：

```
┌─ cc-status · 当前会话
│ 环境
│   目录       /Users/I547149/hanke-dev/cc-status
│   模型       Claude Opus 4.7 (1m)
│   会话 ID    08d24865-...
│   显示模式   detailed
│ Context 窗口
│   已用 / 可用   154.6k / 950k tokens  (84% 剩余)
│   状态         健康
│   物理窗口 1.0M, auto-compact 阈值 95%（CLAUDE_AUTOCOMPACT_PCT_OVERRIDE）
│ 上一轮（最近一次 assistant 回复）
│   发送 input  154.6k tokens（含 cache hit）
│     ├ 命中缓存 153.7k (0.1× 折扣价)
│     ├ 写入缓存 850   (1.25× 贵价，下次能命中)
│     └ 新 input  6    (普通 input 价)
│   模型输出  185 tokens
│   命中率    99%
│ Prompt Cache（5min TTL）
│   剩余       4:47  缓存有效
│ 会话累计
│   总 input    16.3M tokens
│   总 output   125.7k tokens
│   总命中率    97%
│   会话时长    275m11s
│   平均速率    59.7k tokens/min
│ 工具调用
│   Skills    jira×3  wiki×1
│   MCP       github×2
└─
```

`ccs status` **不需要任何参数** — 它会按当前 cwd 自动找 Claude Code 的 transcript（取最新修改的那个 `.jsonl`）。

### 2.3 图例：`ccs explain`

不懂状态栏某个符号？

```sh
ccs explain
```

打印一份完整图例：每个段落、每个图标、每种颜色都有说明。

---

## 3. 切换显示模式

cc-status 内置三种预设模式：

```sh
ccs mode compact     # 单行，仅 dir/git/model/ctx
ccs mode detailed    # 三行，全部指标（默认）
ccs mode debug       # 六行，每个指标独占一行带标签
```

切换会写到配置文件 `current_mode = "..."`，下次状态栏刷新立即生效。

### 3.1 列出所有可选段

```sh
ccs segments
```

会打印所有 `{name}` 段的列表 + 示例 + 中文说明。

### 3.2 列出已有模式

```sh
ccs mode list      # 或：ccs mode（无参）
```

带星号的是当前模式。每个模式下方列出它的所有行模板。

### 3.3 添加自定义模式（推荐方式）

```sh
ccs mode add mine \
  -l "{dir} {git} {ctx}" \
  -l "{last_turn} {hit_rate}" \
  -l "{skills} {mcp}"
```

每个 `-l` 加一行。模板里 `{name}` 引用段，其它字符（包括标签 `last:`、空格、Unicode）原样输出。

如果模式名已存在，需要加 `--force` 才会覆盖：

```sh
ccs mode add mine -l "{dir} {ctx}" --force
```

引用了未知段名时会有 warning，但不会拒绝保存（方便你引用未来版本的新段或纯文本）：

```
ccs mode add bad -l "{dir} {nonexistent}"
warning: unknown segment(s) referenced: nonexistent (run `ccs segments` for the list)
mode 'bad' saved with 1 line(s)
```

### 3.4 删除模式

```sh
ccs mode rm mine
```

如果删的是当前激活模式，会自动切到任意一个其他模式。不允许删除最后一个剩下的模式。

### 3.5 直接编辑配置文件

`ccs mode add` 本质上是修改 TOML，所以你也可以直接打开配置改：

```sh
ccs config-path
# macOS: ~/Library/Application Support/dev.hanke.cc-status/config.toml
```

```toml
[modes.minimal]
lines = ["{dir} {ctx}"]

[modes.token-focused]
lines = [
  "{dir} {git} {model}",
  "{ctx_tokens}  {last_turn}  {hit_rate}",
]
```

保存后 `ccs mode minimal` 即可生效。

---

## 4. 状态栏每段含义

| 段 | 示例 | 含义 |
|---|---|---|
| `{dir}` | `~/hanke-dev/cc-status` | 当前目录最后 3 段，`~` 代表 HOME |
| `{git}` | `wt:foo main ⇡2⇣1 [+!?]` | worktree 名 / 分支 / 领先落后 / 暂存!修改?未跟踪 |
| `{model}` | `Claude Opus 4.7` | CC 报告的模型名 |
| `{ctx}` | `ctx 86% █████▏ 154.6k/950k` | 剩余 % + 进度条 + 已用/可用容量 |
| `{ctx_tokens}` | `154.6k/950k` | 只显示 token 数 |
| `{last_turn}` | `↑12.3k ↓2.1k +865 🎯89%` | 上一轮 input↑ / output↓ / 写入 cache+ / 命中率🎯 |
| `{cache_ttl}` | `cache 3:42` | prompt cache 5min TTL 倒计时（红 < 1min） |
| `{hit_rate}` | `hit 96%` | 整会话累计命中率 |
| `{burn}` | `🔥 32.4k/min` | 会话平均 token 速率 |
| `{skills}` | `skills: jira×3 wiki×1` | Skill 调用次数（按次数倒序，最多 4 个） |
| `{mcp}` | `mcp: github×2` | MCP 服务器调用次数 |
| `{mode}` | `[detailed]` | 当前模式名 |

### 颜色约定

| 颜色 | 含义 |
|---|---|
| 加粗青 | 路径 |
| 加粗紫 | git 分支 |
| 红 | 危险（dirty / ctx 低 / cache 即将过期） |
| 黄 | 中等 |
| 绿 | 健康 |
| 暗色 | 次要数据 |

---

## 5. Context 容量怎么算

这是 cc-status 最容易被误解的地方。

**问题背景**：Claude Code 给状态栏的 `remaining_percentage`（剩余百分比）**不是**按模型物理窗口算的，而是按"距离自动 compact 还有多少"算的。Compact 默认在窗口的 95% 触发，所以即便你切到 1M 模型，CC 的百分比仍然按 950k 算。

cc-status 的做法：

```
physical_window = used_tokens / (1 - CC_remaining_percentage / 100)
capacity        = physical_window × CLAUDE_AUTOCOMPACT_PCT_OVERRIDE / 100
```

简单说：**反推 CC 内部用的窗口大小，再乘以 compact 阈值，得到"你真正能用到多少 token"**。

### 5.1 怎么用满 1M 模型

在 `~/.claude/settings.json` 的 `env` 块加：

```json
{
  "env": {
    "ANTHROPIC_MODEL": "anthropic--claude-opus-latest[1m]",
    "CLAUDE_CODE_AUTO_COMPACT_WINDOW": "1000000"
  }
}
```

| Env 变量 | 作用 | 默认 |
|---|---|---|
| `CLAUDE_CODE_AUTO_COMPACT_WINDOW` | 把窗口大小设为 N tokens | 模型自动检测 |
| `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE` | 在窗口的 N % 触发 compact | `95` |

注意：你的 API 代理（如果用了）必须**转发** `anthropic-beta: context-1m-2025-08-07` header，否则 API 会按 200k 处理。

### 5.2 用更激进的阈值

```json
"CLAUDE_AUTOCOMPACT_PCT_OVERRIDE": "99"   // 用满 1M
"CLAUDE_AUTOCOMPACT_PCT_OVERRIDE": "80"   // 更早 compact，留更多余量
```

cc-status 每次渲染都会读这个 env，状态栏数字会自动跟着变。

---

## 6. 主题与阈值

```toml
[theme]
ctx_low = 20    # 剩余 < 20% 显示红色
ctx_med = 50    # 剩余 < 50% 显示黄色
```

对 1M 模型用户，`ctx_low = 5` / `ctx_med = 20` 更合理（剩 200k 不算危险，剩 50k 才算）。

---

## 7. 数据从哪来

cc-status 不直接读模型 API。所有 token 数据来自 Claude Code 在 `~/.claude/projects/<sanitized-cwd>/<session-id>.jsonl` 写入的 transcript：

- **`message.usage`**：每条 assistant 消息的 input/output/cache_read/cache_creation token 数
- **`tool_use`**：Skill / MCP 调用记录
- **`timestamp`**：用来算 burn rate 和 cache TTL

这些都是 Claude Code 写盘的，cc-status 只是**增量读取并聚合**。状态栏不会向 Anthropic 发任何网络请求。

---

## 8. 故障排查

### 状态栏不显示

```sh
# 1. 确认二进制能被 CC 找到
ls -la /Users/YOU/.local/bin/ccs

# 2. 手动模拟 CC 调用
echo '{"cwd":"'$PWD'","model":{"display_name":"Test"},"context_window":{"remaining_percentage":80},"session_id":"test"}' | ccs render

# 3. 看 settings.json 是不是用了绝对路径
grep -A2 statusLine ~/.claude/settings.json
```

### 显示 `(unknown)` 或没有 token 数

`ccs status` 找不到 transcript。原因通常是 cwd 与 Claude Code project 目录对不上。手动确认：

```sh
ls -t ~/.claude/projects/-Users-YOU-path-to-project/*.jsonl | head -1
```

### 切换模式后没生效

状态栏只在 Claude Code 刷新时才重渲染。任何用户消息都会触发，发一句话就行。

### 1M 配上后 ctx 还是显示 200k

代理没透传 `anthropic-beta: context-1m-2025-08-07`，或者你切的模型名 CC 没识别成 1M-capable。看 [README capacity calculation 段落](../README.md#capacity-calculation-1m-context-support)。

---

## 9. 命令速查

```sh
ccs render                  # 给 CC 用的，读 stdin 输出 ANSI
ccs setup                   # 配置 ~/.claude/settings.json（交互式）
ccs setup --yes             # 同上，跳过确认
ccs setup --check           # 只报告状态，不改
ccs setup --uninstall       # 移除 statusLine 配置
ccs status                  # 详情面板
ccs explain                 # 图例
ccs segments                # 列出所有可选段
ccs mode                    # = ccs mode list
ccs mode list               # 列出所有已有模式
ccs mode <name>             # 切到某模式
ccs mode add <name> -l "..." [-l "..."] [--force]
                            # 添加 / 覆盖自定义模式
ccs mode rm <name>          # 删除模式
ccs init                    # 写默认配置（如果不存在）
ccs init --force            # 强制覆盖
ccs config-path             # 打印配置文件路径
ccs --help                  # 帮助
```
