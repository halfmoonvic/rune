# Rune 设计文档

> 纯 Rust 跨平台桌面小工具，基于 `eframe`/`egui`，提供 CLI 驱动的表单弹窗与流式输出窗口。
> API 设计参考 `zenity` 的命令习惯，但用结构化配置（TOML/JSON）取代 zenity 的"逐个 `--add-xxx`"语法来覆盖复杂场景。

## 0. 定位与非目标

**定位**：介于 `zenity` 和完整 GUI 应用之间的轻量工具，核心诉求是启动快、跨平台、CLI 友好、易脚本化。

**非目标（第一版明确不做）**：

- 插件系统
- 主题深度自定义（只提供 `light` / `dark` / `system` 三档）
- 网络通信（仅本地 CLI / pipe）
- 表单 item 之间的条件显示、动态校验规则、自定义正则
- 表单与流式输出在同一窗口内的模式切换

---

## 1. 命令结构

```text
rune form    [OPTIONS]              # 表单场景
rune stream  [OPTIONS] [-- CMD...]  # 流式输出场景

# 语法糖（本质是 form 的单 item 预设）
rune confirm <TEXT> [OPTIONS]       # 是/否
rune alert   <TEXT> [OPTIONS]       # 纯展示，单按钮关闭
rune input   <TEXT> [OPTIONS]       # 单行输入
```

与 zenity 的命令习惯对比：

| zenity | Rune | 说明 |
|---|---|---|
| `zenity --question` | `rune confirm` | 是/否，退出码表达结果 |
| `zenity --info` | `rune alert` | 纯展示 |
| `zenity --entry` | `rune input` | 单行输入，结果走 stdout |
| `zenity --forms --add-entry --add-entry ...` | `rune form --config x.toml` | zenity 逐个 `--add-*` 拼表单；Rune 用结构化配置文件描述，CLI flag 仅做 ≤3 个 item 的简化场景 |
| `zenity --progress` | `rune stream` | zenity 的进度条是百分比驱动，Rune 的 stream 是日志驱动（追加文本流），语义不同，未做百分比进度条（可作为后续扩展） |

---

## 2. 通用窗口参数（`form` 与 `stream` 共用）

这些是"业务/行为"类 flag，每次调用都可能不同，因此走 CLI（也可以打包进 `--config`，见第 3 节）：

| Flag | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `--title <STR>` | string | `"Rune"` | 窗口标题 |
| `--width <PX>` | u32 | 自适应 | 窗口宽度，高度默认自适应内容 |
| `--always-on-top` | flag | false | 窗口置顶（也可在全局样式配置中设默认值，见 §3.2） |
| `--theme <light\|dark\|system>` | enum | `system` | 主题（也可在全局样式配置中设默认值，见 §3.2） |
| `--timeout <SECS>` | u32 | 无 | 超时自动关闭，对齐 zenity 语义，退出码 `5` |

> `--theme`、`--always-on-top` 比较特殊：它们既可以当作"全局默认值"写进全局样式配置（§3.2），也可以当作"这次调用"的字段被 CLI flag 临时覆盖——同一个字段同时存在于两层，正是"CLI flag 优先级最高"这条规则要解决的典型场景。

---

## 3. 配置体系

Rune 的参数来源分三层，**职责严格分离**，互不越界：

```text
内置默认值
  ↓ 被覆盖
全局样式配置 ~/.config/rune/config.toml   （隐式加载，只管"长什么样"）
  ↓ 被覆盖
--config 指定的文件                        （只是一堆 CLI flag 的打包形式，只管"做什么"）
  ↓ 被覆盖（最高优先级）
CLI flag                                    （一次性微调，永远生效）
```

### 3.1 三层各自的职责边界

| 层级 | 回答的问题 | 典型字段 | 加载方式 |
|---|---|---|---|
| 全局样式配置 | "Rune 窗口长什么样" | 圆角半径、header 高度、字体大小、配色、阴影 | 隐式加载，每次调用自动生效，**不需要在 CLI 里指定** |
| `--config <FILE>` | "这次调用要做什么" | title、width、items 数组、子进程命令…… | 显式通过 `--config` 指定，**等价于把这些字段展开成对应的 CLI flag**，不是独立的新协议 |
| CLI flag | "这次临时改一下什么" | 任意字段的一次性覆盖 | 显式传入，优先级永远最高，哪怕 `--config` 里写了同名字段也会被 flag 覆盖 |

**核心心智模型**：`--config some.toml` 不是一个新的"配置语言"，它只是"我懒得敲一长串 flag，把它们写进文件里"。所以 `--config` 文件里能写的字段，**理论上都对应一个同名的 CLI flag**（哪怕这个 flag 平时很少有人直接敲，比如 `--items`）。这保证了文档心智的一致性：用户随时可以把 `--config` 文件内容"展开"理解成等价的命令行调用。

全局样式配置则是另一个维度，它**不**对应任何"业务" flag——没有人会为了改一次圆角去敲 `rune form --corner-radius 8 --title ... --input ...`，圆角这种东西应该写一次、长期生效。CLI flag 里仍保留少数高频外观 flag（如 `--theme`、`--always-on-top`）作为"临时覆盖全局样式"的逃生通道，但大多数样式字段（圆角、header 高度、字体、配色）**只在全局配置文件里出现，不提供对应 CLI flag**，避免 `--help` 膨胀。

### 3.2 全局样式配置（隐式加载）

路径遵循 XDG 规范（用 `directories` crate 处理跨平台差异）：

- Linux: `~/.config/rune/config.toml`
- macOS: `~/Library/Application Support/rune/config.toml`
- Windows: `%APPDATA%\rune\config.toml`

```toml
# ~/.config/rune/config.toml —— 只管样式，没有 title/items 这类业务字段

[window]
theme = "system"          # light | dark | system；可被 --theme 临时覆盖
always_on_top = false      # 可被 --always-on-top 临时覆盖
corner_radius = 8          # 整体圆角，对应 egui 的 Rounding；无对应 CLI flag
shadow = true               # 模态卡片是否带阴影；无对应 CLI flag

[window.header]
height = 40
font_size = 16
show_icon = true

[window.body]
padding = 16
font_size = 14
line_height = 1.5

[window.colors]            # 仅在 theme = "custom" 时生效，否则忽略
background = "#1e1e1e"
text = "#e0e0e0"
accent = "#5b8def"
border = "#333333"
```

所有字段都有内置默认值（`#[serde(default)]`），全局配置文件可以完全不存在，也可以只写想覆盖的几行，不需要写全。

### 3.3 `--config`：CLI 参数的打包形式

```bash
rune form --config deploy.toml
# 或
rune form --config deploy.json
```

靠扩展名自动选择解析器，字段结构完全一致（同一份数据模型的两种表达）：

```toml
# deploy.toml —— 等价于把下面这些字段逐个敲成 --title "Deploy Settings" --width 480 ...

title = "Deploy Settings"
width = 480
submit_label = "Deploy"
cancel_label = "Cancel"

[[items]]
type = "markdown"
content = "## Deploy Checklist\nReview before continuing."

[[items]]
type = "input"
id = "branch"
label = "Branch name"
default = "main"
required = true

[[items]]
type = "select"
id = "env"
label = "Environment"
options = ["dev", "staging", "prod"]
default = "staging"
```

```bash
# 用 --config 打包大部分参数，再用 CLI flag 临时覆盖其中一个字段
# flag 优先级最高，最终 title 会是 "Hotfix Deploy"，而不是文件里的 "Deploy Settings"
rune form --config deploy.toml --title "Hotfix Deploy"
```

支持从 stdin 读取（子命令级区分，不与 `stream` 的 stdin 冲突）：

```bash
cat deploy.toml | rune form --config-stdin --format toml
```

### 3.4 辅助命令

```bash
rune config init    # 在默认路径生成一份带注释的全局样式配置
rune config show    # 打印当前生效的合并后样式配置（调试"为什么圆角没生效"）
rune config path     # 打印全局配置文件应在的路径
```

### 3.5 实现要点

每层配置都用 `#[serde(default)]` + 手写 `Default impl`，不需要手动写 merge 逻辑：

```rust
#[derive(Deserialize, Default)]
#[serde(default)]
struct StyleConfig {
    window: WindowStyle,
}

#[derive(Deserialize)]
#[serde(default)]
struct WindowStyle {
    theme: Theme,
    always_on_top: bool,
    corner_radius: f32,
    shadow: bool,
    header: HeaderStyle,
    body: BodyStyle,
    colors: Option<ColorStyle>, // 仅 custom 主题时存在
}

impl Default for WindowStyle {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            always_on_top: false,
            corner_radius: 8.0,
            shadow: true,
            header: HeaderStyle::default(),
            body: BodyStyle::default(),
            colors: None,
        }
    }
}
```

加载顺序：

```rust
fn resolve_style() -> WindowStyle {
    // 1. 内置默认值
    let mut style = WindowStyle::default();

    // 2. 全局样式配置文件（如果存在），整份覆盖（serde(default) 已处理缺失字段）
    if let Some(path) = global_config_path() {
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(parsed) = toml::from_str::<StyleConfig>(&text) {
                style = parsed.window;
            }
        }
    }

    style
    // 3. CLI flag 的覆盖（--theme / --always-on-top 等）在 clap 解析完之后，
    //    由调用方逐字段 `if let Some(v) = cli.theme { style.theme = v; }` 完成
}
```

`--config` 的解析则直接走 `CallConfig`（即 form 的业务参数结构），与样式配置是两条独立的解析路径，互不干扰：

```rust
fn load_call_config(path: &Path) -> Result<CallConfig> {
    let text = std::fs::read_to_string(path)?;
    match path.extension().and_then(|s| s.to_str()) {
        Some("json") => Ok(serde_json::from_str(&text)?),
        _ => Ok(toml::from_str(&text)?), // 默认按 toml 解析
    }
}
```

`corner_radius` 最终映射为 egui 的 `Rounding`，`shadow` 映射为 `Shadow`，这些在程序启动时一次性 apply 到 `egui::Style` / 卡片 `Frame` 上，运行期不变化，没有性能顾虑。

---

## 4. Form 场景设计

### 4.1 CLI 简化语法

仅推荐用于 ≤3 个简单 item 的场景，每种 item 类型用专门 flag，避免冒号/逗号 DSL 带来的转义歧义：

```bash
rune form \
  --title "Deploy Settings" \
  --text "This will deploy to production." \
  --input branch="Branch name" --default branch="main" \
  --select env="Environment" --options env="dev,staging,prod" \
  --checkbox confirm="I understand this is irreversible" --required confirm
```

> 超过 3-4 个 item 或需要 textarea / markdown 长文本时，请切换到 `--config`（见第 3 节）。

### 4.2 结构化配置（核心，承载复杂表单）

```bash
rune form --config deploy.toml
# 或
rune form --config deploy.json
```

字段与解析规则见 §3.3，此处仅展示完整示例 `deploy.toml`：

```toml
title = "Deploy Settings"
width = 480
submit_label = "Deploy"
cancel_label = "Cancel"

[[items]]
type = "markdown"
content = "## Deploy Checklist\nReview before continuing."

[[items]]
type = "text"
content = "This will deploy to **production**."
style = "warning"

[[items]]
type = "input"
id = "branch"
label = "Branch name"
default = "main"
placeholder = "e.g. main"
required = true

[[items]]
type = "textarea"
id = "notes"
label = "Release notes"
rows = 4
default = ""

[[items]]
type = "select"
id = "env"
label = "Environment"
options = ["dev", "staging", "prod"]
default = "staging"

[[items]]
type = "checkbox"
id = "confirm"
label = "I understand this is irreversible"
required = true
default = false
```

等价的 JSON 表达（程序化生成场景更友好）：

```json
{
  "title": "Deploy Settings",
  "width": 480,
  "submit_label": "Deploy",
  "cancel_label": "Cancel",
  "items": [
    { "type": "markdown", "content": "## Deploy Checklist\nReview before continuing." },
    { "type": "text", "content": "This will deploy to **production**.", "style": "warning" },
    { "type": "input", "id": "branch", "label": "Branch name", "default": "main", "required": true },
    { "type": "select", "id": "env", "label": "Environment", "options": ["dev", "staging", "prod"], "default": "staging" },
    { "type": "checkbox", "id": "confirm", "label": "I understand this is irreversible", "required": true, "default": false }
  ]
}
```

### 4.3 Item 类型与字段

| Type | 用途 | 需要 `id` | 关键字段 |
|---|---|---|---|
| `text` | 静态文字展示 | 否 | `content`, `style: info\|warning\|danger` |
| `markdown` | 富文本展示（用 `egui_commonmark` 渲染） | 否 | `content` |
| `input` | 单行输入 | 是 | `label`, `default`, `placeholder`, `required` |
| `textarea` | 多行输入 | 是 | `label`, `default`, `rows`, `required` |
| `select` | 下拉单选 | 是 | `label`, `options: [string]`, `default` |
| `checkbox` | 勾选项 | 是 | `label`, `default: bool`, `required` |

**设计要点**：

- 只有交互类 item（input/textarea/select/checkbox）需要 `id`；`id` 即输出 JSON 的 key。展示类 item（text/markdown）不参与输出。
- `required` 语义按类型区分：input/textarea 要求非空字符串；checkbox 要求值为 `true`；select 默认始终有值，`required` 对其无意义（文档中需明确声明，避免实现时产生歧义）。
- 校验失败时阻止提交，在对应 item 下方显示一行错误提示，不做弹窗式打扰。

### 4.4 输出协议

提交成功：stdout 输出**单行** JSON（仅含交互类 item），退出码 `0`：

```json
{"branch":"main","notes":"Fixed bug X","env":"prod","confirm":true}
```

用户取消（点 Cancel 或关闭窗口）：stdout 为空，退出码 `1`。

脚本示例：

```bash
result=$(rune form --config deploy.toml) || { echo "用户取消"; exit 1; }
env=$(echo "$result" | jq -r .env)
```

---

## 5. Stream 场景设计

### 5.1 两种模式

**模式 A：纯 pipe（被动接收，对上游进程无控制权）**

```bash
some_long_task.sh | rune stream --title "Build Log"
```

窗口里仅提供 "Close" 按钮（不叫 "Stop"，因为 Rune 并不能真正中止上游任务，UI 文案需诚实反映这一点）。

**模式 B：托管子进程（主动控制，推荐作为主推用法）**

```bash
rune stream --title "Build Log" -- cargo build --release
```

Rune 自己 `spawn` 子进程，捕获 stdout/stderr 管道进窗口，因此可以：

- 提供真正的 "Stop" 按钮（先 `SIGTERM`，超时后 `SIGKILL`）
- 显示运行状态：`Running...` → `Exited (code 0)` / `Exited (code 1)`
- Rune 自身退出码 = 子进程退出码，便于脚本链式判断

### 5.2 输入协议

| Flag | 说明 |
|---|---|
| 默认 | 纯文本逐行追加，等宽字体渲染 |
| `--ansi` | 解析 ANSI 颜色/样式转义序列，渲染为对应颜色的 `RichText` |
| `--format ndjson`（预留，v2） | 每行为 `{"level":"info\|warn\|error","text":"...","ts":"..."}`，按 `level` 着色/加 icon |

### 5.3 性能与生命周期

| Flag | 默认值 | 说明 |
|---|---|---|
| `--max-lines <N>` | 5000 | 环形缓冲上限（`VecDeque`），超出丢最旧行，避免长任务拖垮内存与重绘 |
| `--on-finish <keep\|close>` | `keep` | 进程退出/EOF 后是否保留窗口展示最终结果 |

**滚动行为**：默认贴底自动滚动；用户手动上滚后停止自动滚动，并在右下角浮一个"回到底部"按钮。

---

## 6. 退出码总览

| 场景 | 退出码 | stdout | stderr |
|---|---|---|---|
| form 提交 | 0 | JSON 结果 | - |
| form 取消 | 1 | 空 | - |
| stream 模式 B，子进程正常退出 | 子进程退出码 | - | - |
| stream 用户点 Stop | 130（SIGINT 惯例值） | - | - |
| `--timeout` 触发 | 5（对齐 zenity 语义） | 空 | - |
| `--config` 文件不存在 / 解析失败 | 2 | 空 | 错误信息 |
| `--config` 含未知 item type | 2 | 空 | 错误信息，指明具体 item |

**铁律**：stdout 只装"结果数据"，所有错误信息和过程提示走 stderr。这是该类 CLI 工具能否被干净脚本化的硬约束。

---

## 7. 实现要点（crate 选型参考）

| 用途 | crate |
|---|---|
| CLI 解析 | `clap`（derive 模式，配合子命令结构） |
| 配置/数据序列化 | `serde` + `toml` + `serde_json`（同一份数据模型同时支持两种格式） |
| 跨平台配置路径 | `directories`（XDG / macOS / Windows 路径差异处理） |
| markdown 渲染 | `egui_commonmark` |
| 子进程托管（模式 B） | `std::process::Command` + 管道；后台线程读 stdout/stderr，经 `mpsc::channel` 推给 egui 主循环；`update()` 中 `try_recv` 拉取新行并 `ctx.request_repaint()` |
| ANSI 解析 | 暂无现成的直接转 egui `RichText` 的库，需自行实现轻量状态机（逻辑量不大） |
| 字体（中文支持） | egui 默认字体对 CJK 支持有限，需捆绑思源黑体子集或做系统字体探测，需在实现阶段提前规划 |

---

## 8. 后续可扩展方向（暂不在第一版范围内）

- `stream` 的百分比进度条模式（对齐 `zenity --progress`）
- NDJSON 结构化日志的完整渲染（按 level 着色/过滤/搜索）
- 表单 item 间的条件显示
- 多实例窗口管理（第一版按"每次调用独立进程"处理，足够简单可靠）
