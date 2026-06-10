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

### 1.x 对话式助手（推荐）

`ccs setup` 默认会问要不要装一个 **Claude Code skill**（路径 `~/.claude/skills/cc-status/`）。装上以后你不用记任何命令，直接在 Claude Code 对话里说话就行：

- "把状态栏切成 detailed"
- "在状态栏加上今天的花费"
- "做个插件显示我 jira 上未读 issue 数"
- "状态栏怎么是空的，帮我看看哪里有问题"
- "🎯99% 是啥意思"

Claude 会读 skill 的 SKILL.md，**自己跑**对应的 `ccs` 命令，给你看渲染效果，破坏性操作前会和你确认。

强制装：`ccs setup --with-skill`。跳过：`ccs setup --no-skill`。卸载：`ccs setup --uninstall`（会同时清掉 statusLine 配置和 skill）。

### 1.y 可视化编辑器：`ccs config edit`

不想记 segment 名字？直接拖拽：

```sh
ccs config edit
```

启动一个临时的 127.0.0.1 网页（随机端口 + URL token），自动打开浏览器：

- **顶部 mode tabs**：点击切换正在编辑的 mode
- **每行的 segment 块**可以拖拽：行内重排、跨行移动、拖回 palette = 删除、点 `×` = 删
- **底部 palette**：所有内置 segment + 你 `<config>/plugins/` 下的插件
- **实时预览**：用 mock 数据渲染，无需真实会话也能看效果
- **Save** 写入 `config.toml`，server 5 秒后自动退出

纯 stdlib HTTP，无新增依赖；30 分钟空闲也会自动退出。改 mode 排版最直观的入口。

> 局限：模板里的字面量文字（如 debug mode 里 `cache: {cache_ttl}` 的"cache:"）保存时会被丢掉。需要保留字面量请用 `ccs mode edit` 文本编辑。

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

cc-status 内置 7 个预设模式：

```sh
ccs mode compact     # 单行，基础信息（默认）
ccs mode minimal     # 单行，只 dir + ctx，最简
ccs mode detailed    # 三行，所有指标
ccs mode cost        # 两行，专注美元成本
ccs mode tokens      # 三行，专注 token 流（上轮+命中率+速率+缓存 TTL）
ccs mode tools       # 三行，专注 Skill / MCP 调用
ccs mode debug       # 八行，每个指标独占一行带标签
```

切换会写到配置文件 `current_mode = "..."`，下次状态栏刷新立即生效。

### 3.1 列出所有可选段

```sh
ccs segments
```

会打印所有 `{name}` 段的列表 + 示例 + 说明。

### 3.2 列出已有模式

```sh
ccs mode list      # 或：ccs mode（无参）
```

带星号的是当前模式。每个模式下方列出它的所有行模板。

### 3.3 快速给当前模式加段（推荐）

```sh
# 给当前模式末尾加新行
ccs mode append cost_today

# 多个段加同一行
ccs mode append hit_rate burn cache_ttl

# 加到指定行末尾（1-based）
ccs mode append --line 1 git

# 给非当前模式加段
ccs mode append --mode detailed cost_today
```

`{name}` 既可以写 `cost_today`（裸名）也可以写 `{cost_today}`。引用未知段会 warn 但仍会保存。

### 3.4 用编辑器自由编辑

```sh
ccs mode edit              # 编辑当前模式
ccs mode edit detailed     # 编辑指定模式
```

打开 `$EDITOR`（或 `VISUAL`，默认 `vi`）。每行一个模板，空行和 `#` 开头的注释会被忽略。保存退出即生效。

### 3.5 添加完整自定义模式

```sh
ccs mode add mine \
  -l "{dir} {git} {ctx}" \
  -l "{last_turn} {hit_rate}" \
  -l "{skills} {mcp}"
```

每个 `-l` 加一行。模式名已存在时需要 `--force`。

### 3.6 删除模式

```sh
ccs mode rm mine
```

如果删的是当前激活模式，会自动切到任意一个其他模式。不允许删除最后一个剩下的模式。

### 3.7 直接编辑配置文件

`ccs mode add` / `append` / `edit` 本质上都是改 TOML，所以你也可以打开配置直接改：

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
| `{dir}` | `~/hanke-dev/cc-status` | 当前目录最后 3 段；HOME 显示为 `~`。 |
| `{git}` | `wt:foo main ⇡2⇣1 [+!?]` | git: worktree（仅在 worktree 内）/ 分支 / 领先⇡落后⇣上游 / `+` 已暂存 `!` 已修改 `?` 未跟踪。 |
| `{model}` | `claude-opus-4-7 [1m]` | Claude Code **实际调用**的 model id。优先级：transcript → stdin → settings.json。`[1m]` 表示 1M 上下文档位。会剃掉 `anthropic--` / `anthropic/` 噪音前缀，但保留 `bedrock/` / `vertex_ai/` 等部署目标前缀。 |
| `{ctx}` | `ctx 54% ▰▰▰▰▰▱▱▱▱▱ 504.1k/1.0M` | 电池条：剩余容量百分比 + 形象化电量条（▰=剩余，▱=已用）+ 已用/容量。颜色：≥50% 绿 / 20–50% 黄 / <20% 红。容量是 auto-compact 触发阈值（默认物理窗口的 95%）。 |
| `{ctx_tokens}` | `ctx-used 504.1k/1.0M` | 只显示数字，无条无百分比。当 `{ctx}` 太宽时用。 |
| `{last_turn}` | `↑504.1k ↓618 cache+581 🎯100%` | 最近一轮：↑ 发给模型的 token（含 cache hit + cache write）/ ↓ 输出 / `cache+N` 这一轮写入 cache 的量 / 🎯 **本轮单独**的 cache 命中率。 |
| `{cache_ttl}` | `cache 4:42` | prompt cache 5 分钟 TTL 倒计时。<1 分钟或已过期变红 — 下一轮会按全价 input 算钱，不走 cache_read 折扣。 |
| `{hit_rate}` | `hit 96%` | 整会话累计 cache 命中率（所有轮聚合）。跟 `{last_turn}` 的 🎯（仅最近一轮）不同。 |
| `{burn}` | `🔥 32.4k tok/min` | 会话平均 token 吞吐（总 token ÷ 距首轮分钟数）。 |
| `{session_age}` | `1h23m` | 距第一次 assistant 回复的时长：`42s` / `12m` / `1h23m` / `2d3h`。 |
| `{skills}` | `skills: jira×3 wiki×1` | Skill 调用前 4（按次数倒序）。无调用时此段隐藏。 |
| `{mcp}` | `mcp: github×2` | MCP 服务器调用按服务器名聚合。无调用时此段隐藏。 |
| `{cost_last}` | `last $0.012` | 上一轮花了多少美元（按当前模型价格，含 cache 折扣）。 |
| `{cost_session}` | `sess $1.42` | 整个当前会话累计成本（USD）。 |
| `{cost_today}` | `today $4.18` | 今天（本地时间日）所有会话所有模型累计。 |
| `{cost_week}` | `7d $24.50` | 最近 7 天滚动窗口累计。 |
| `{cost}` | `last $0.012 \| today $4.18` | `cost_last` + `cost_today` 的紧凑组合。 |
| `{mode}` | `[balanced]` | 当前 mode 名。 |
| `{plugin:NAME}` | *（插件 stdout）* | 跑 `<config>/plugins/NAME`，stdout 作为段值。250ms 硬超时，80 字符上限，允许 ANSI SGR。见 `ccs plugin new`。 |

> 状态栏看到字符串不知道含义？跑 `ccs explain` 看图例，或 `ccs status` 看当前会话各项数据带完整标签。

### 插件段（自定义命令）

需要 cc-status 没自带的指标？内置脚手架直接生成可执行模板。

#### 30 秒上手

```sh
ccs plugin new hello                  # 用 sh 模板创建（推荐）
ccs plugin run hello                  # 调试运行：看 stdout/stderr/exit/耗时
ccs mode append plugin:hello          # 加到当前 mode
```

或者用 Python 模板：

```sh
ccs plugin new my-metric --lang python
```

#### 管理插件

```sh
ccs plugin list                       # 列已装的插件，标可执行状态
ccs plugin doctor                     # 全部插件体检（耗时 / 孤儿 / chmod）
ccs plugin path                       # 插件目录路径
ccs plugin run my-metric --warm       # 跑两次取第二次的耗时（避开冷启动）
ccs plugin new my-metric --force      # 覆盖已存在的插件文件
```

`ccs plugin doctor` 把每个插件暖热跑一次，按以下规则打分：

- ✗ **fail** — 没可执行位、exec 失败、文件为空、或暖热耗时 > 250 ms（render 时一定超时）
- ⚠ **warn** — 没 shebang、退出码非 0、stdout 为空、或没被任何 mode 引用（孤儿）
- ✓ **ok** — 全过

写完插件、改完插件、或者怀疑某个老插件还在但忘了用，跑一下 `doctor` 就能一眼定位问题。

#### 约定

插件就是 `<配置目录>/plugins/<NAME>` 下的可执行文件 —— shebang 脚本（sh / python / ruby …）或编译过的二进制都行。cc-status 把它当子进程跑，约定如下：

- 可执行文件的 stdin 是 Claude Code 的状态栏 JSON（schema：`{cwd, model.{id,display_name}, context_window.remaining_percentage, session_id, transcript_path}`）
- **stdout** 即段值；换行/制表符塌成空格，ANSI SGR 颜色保留，其他控制字符剥掉，最长 80 字符
- 硬超时 **250ms** —— 超时插件被杀，段渲染为空。用 `ccs plugin run --warm` 测稳态耗时（macOS Gatekeeper 第一次跑会加 200ms+）
- 退出码非 0 / stdout 为空 / 文件不存在 → 段渲染为 `""`（周围空格自动塌缩）
- 插件继承用户的 `$PATH` 和环境变量

#### 性能预算

| 运行时 | 冷启动 | 暖热 | 评价 |
|---|---|---|---|
| 原生二进制（Go / Rust） | <5 ms | <5 ms | 最佳 |
| sh / bash | 5–20 ms | 5–10 ms | 很好 |
| python3 | 30–80 ms | ~30 ms | 逻辑紧凑 OK |
| node | 70–150 ms | ~70 ms | 复杂逻辑容易超 |
| 任何网络调用 | 100ms+ | 100ms+ | **别做** —— 一定超时 |

插件**每次状态栏刷新**都跑一遍 —— 每条 prompt 都跑 —— 必须快。

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
ccs mode append <seg> [seg2 ...] [--mode N] [--line K]
                            # 给当前/指定模式末尾加段（推荐）
ccs mode edit [name]        # 在 $EDITOR 里编辑模式
ccs mode rm <name>          # 删除模式
ccs init                    # 写默认配置（如果不存在）
ccs init --force            # 强制覆盖
ccs config-path             # 打印配置文件路径
ccs --help                  # 帮助
```
