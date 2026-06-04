//! `ccs explain` — print a legend describing every statusline segment.

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const BOLD_CYAN: &str = "\x1b[1;36m";
const BOLD_MAGENTA: &str = "\x1b[1;35m";
const RED: &str = "\x1b[0;31m";
const YELLOW: &str = "\x1b[0;33m";
const GREEN: &str = "\x1b[0;32m";
const DIM: &str = "\x1b[2m";

pub fn run() -> anyhow::Result<()> {
    let out = format!(
        "\
{B}cc-status · 状态栏图例{R}

{B}第一行 · 环境{R}
  {C}~/path{R}                当前目录（最后 3 段路径，~ 代表 HOME）
  {DIM}wt:NAME{R}               git worktree 名（不在 worktree 时不显示）
  {M}branch{R}                git 分支名
  {DIM}⇡N{R} / {DIM}⇣N{R}              比远端领先 / 落后 N 个 commit
  {RED}[+!?]{R}                 + 已暂存   ! 已修改   ? 未跟踪
  {DIM}模型名{R}                  当前对话使用的模型

{B}第二行 · 本轮 + 缓存{R}
  ctx {GREEN}84%{R} {GREEN}█████{R} {DIM}133k/950k{R}  剩余比例 + 已用/可用容量（绿 ≥50  黄 20–50  红 <20）
                        可用 = 反推出的物理窗口 × CLAUDE_AUTOCOMPACT_PCT_OVERRIDE / 100
                        默认 95%。比如 1M Opus 的可用容量约 950k，超过即触发 compact。
  {DIM}↑12.3k{R}                上一轮 input token（含 cache hit）
  {DIM}↓341{R}                  上一轮 output token
  {DIM}+865{R}                  本轮新写入 cache 的 token（cache_creation, 1.25× 价）
  {DIM}🎯99%{R}                 上一轮缓存命中率
  {DIM}cache 4:42{R}            缓存 5min TTL 倒计时（{RED}红色 < 1min{R} / cache expired）
  {DIM}hit 96%{R}               整会话累计命中率
  {DIM}🔥 32.4k/min{R}          会话平均 token 速率

{B}第三行 · 本会话工具调用{R}
  {DIM}skills: jira×3 wiki×1{R}  Skill 工具调用次数（按次数倒序，最多 4 个）
  {DIM}mcp: github×2{R}         MCP 服务器调用次数（按服务器聚合）

{B}颜色含义{R}
  {C}加粗青{R}     路径
  {M}加粗紫{R}     git 分支
  {RED}红{R}        危险（dirty / ctx 低 / cache 即将过期）
  {YELLOW}黄{R}        中等（ctx 20–50%）
  {GREEN}绿{R}        健康（ctx ≥50%）
  {DIM}暗色{R}       次要数据

{B}计费速查{R}
  cache_read       0.1× 普通 input 价（命中越高越便宜）
  cache_creation   1.25× 普通 input 价（写入贵，但下次能命中）
  output           ~5× 普通 input 价

{B}相关命令{R}
  ccs status            打印当前会话详情面板（不输出 ANSI 也好读）
  ccs mode <name>       切换显示模式：compact / detailed / debug
  ccs config-path       配置文件位置
",
        B = BOLD,
        R = RESET,
        C = BOLD_CYAN,
        M = BOLD_MAGENTA,
        DIM = DIM,
        RED = RED,
        YELLOW = YELLOW,
        GREEN = GREEN,
    );
    print!("{}", out);
    Ok(())
}
