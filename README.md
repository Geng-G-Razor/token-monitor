# Fufei Monitor

macOS 菜单栏应用，监控 [fufei.mossx.ai](https://fufei.mossx.ai) 的 API 消费情况。点击状态栏图标即可查看今日消费、请求数、RPM/TPM、各平台分布等数据。

## 功能

- **菜单栏常驻**：托盘图标旁显示今日消费金额（如 `$1.23`），左键点击弹出面板
- **登录 / 登出**：邮箱 + 密码登录，Token 存储在 macOS Keychain
- **实时数据**：今日消费、请求数、RPM、TPM、平均耗时
- **累计统计**：总消费、总 Token、总请求数
- **按平台分布**：各平台消费占比及进度条
- **定时刷新**：支持 1 / 5 / 10 / 30 分钟间隔，窗口获焦时自动刷新
- **Token 自动刷新**：Access Token 过期后自动使用 Refresh Token 续期
- **智能隐藏**：登录时窗口不自动隐藏；Dashboard 状态下失焦 3 秒后自动隐藏，重新获焦可取消

## 技术栈

| 层 | 技术 |
|---|---|
| 桌面框架 | Tauri 2 |
| 前端 | TypeScript + Vite，原生 HTML/CSS |
| 后端 | Rust（reqwest + keyring + thiserror） |
| 凭证存储 | macOS Keychain（keyring crate，apple-native） |

## 项目结构

```
fufei-api-app/
├── package.json              # 前端依赖及脚本
├── vite.config.ts            # Vite 构建配置
├── tsconfig.json
├── index.html                # 入口 HTML（登录 / Dashboard 三视图）
├── src/                      # 前端
│   ├── main.ts               # invoke Tauri commands、定时刷新、面板交互
│   └── style.css             # macOS 风格样式（不透明背景 / 暗色适配）
└── src-tauri/
    ├── Cargo.toml             # Rust 依赖
    ├── tauri.conf.json        # 窗口（无边框 / 常驻顶层）、托盘、打包配置
    ├── build.rs
    ├── icons/                 # 应用图标
    └── src/
        ├── main.rs           # 入口，调用 lib::run()
        ├── lib.rs            # Tauri Builder + Tray 注册 + 窗口失焦延迟隐藏
        ├── commands.rs       # Tauri commands: login / logout / is_logged_in / fetch_stats
        ├── api.rs            # reqwest 封装: login / refresh / fetch_stats（Envelope 解析）
        └── auth.rs           # macOS Keychain 存取 token
```

## 核心模块说明

### 前端 (`src/main.ts`)

- **三视图切换**：loading → login → dash，通过 `show()` 函数控制显隐
- **视图感知自动隐藏**：`show()` 在切换视图时调用 `set_auto_hide` — 登录视图禁用自动隐藏，Dashboard 视图启用
- **数据渲染**：`render(Stats)` 将后端返回的 `StatsData` 映射到 DOM；平台金额使用紧凑格式 `moneyShort`（≥ $1000 时显示为 `$1.2K`，否则四舍五入到整数），适配窄列布局
- **窗口高度自适应**：每次渲染后调用 `fitHeight()` 测量 `#view-dash` 实际高度并通知后端 `fit_height` 调整窗口，消除底部空白
- **托盘标题更新**：每次刷新后通过 `invoke("set_tray_title")` 更新菜单栏标题为 `$X.XX`
- **定时器**：`setInterval` 轮询，支持用户切换刷新间隔
- **焦点刷新**：`win.onFocusChanged` 在面板获焦时立即刷新
- **退出按钮**：登录页和 Dashboard 底部均有"退出"按钮，调用 `invoke("quit_app")`

### 后端 Rust

#### `lib.rs` — 应用入口与托盘

- 构建系统托盘图标，**左键点击**弹出 / 隐藏面板窗口（无右键菜单，退出通过 UI 按钮）
- 面板锚定在托盘图标正下方偏移 15px，位置根据点击坐标计算
- 窗口宽度固定 340，高度自适应内容（初次为 510，之后由前端 `fit_height` 动态调整）
- 打开时复用上次窗口高度，避免每次都撑回默认值
- 窗口失焦延迟隐藏机制：
  - `ALLOW_AUTO_HIDE` / `PENDING_HIDE` 两个 `AtomicBool` 控制行为
  - 失焦时设置 `PENDING_HIDE=true`，3 秒后若仍为 true 且 `ALLOW_AUTO_HIDE=true` 则隐藏
  - 重新获焦时取消 `PENDING_HIDE`，阻止延迟隐藏执行
- 托盘初始标题为空字符串，前端刷新数据后更新

#### `lib.rs` — 额外 Tauri Commands

| 命令 | 说明 |
|------|------|
| `set_auto_hide(enabled: bool)` | 控制是否允许失焦自动隐藏（登录时禁用） |
| `quit_app()` | 退出应用（替代原来的托盘右键菜单） |
| `set_tray_title(title: String)` | 更新托盘图标旁的标题文字 |
| `fit_height(height: f64)` | 按内容高度调整窗口（宽度固定 340），由前端渲染后调用 |

#### `commands.rs` — Tauri Commands

| 命令 | 签名 | 说明 |
|------|------|------|
| `login` | `(email, password) → bool` | 登录并将 token 存入 Keychain |
| `logout` | `() → bool` | 清除 Keychain 中的 token |
| `is_logged_in` | `() → bool` | 检查是否有存储的 access token |
| `fetch_stats` | `() → StatsData` | 拉取仪表盘数据，401 时自动 refresh 一次 |

#### `api.rs` — HTTP 客户端

- 基础地址：`https://fufei.mossx.ai`
- 三个接口：`/api/v1/auth/login`、`/api/v1/auth/refresh`、`/api/v1/usage/dashboard/stats`
- API 响应统一使用 Envelope 模式解析：`{ "code": 0, "data": { ... } }`
  - `LoginEnvelope` 解析 login/refresh 响应
  - `StatsEnvelope` 解析 stats 响应
- 20 秒超时，rustls TLS
- 统一错误处理：`ApiError`（Network / Unauthorized / Api / Status）

#### `auth.rs` — Keychain 操作

- Service: `ai.mossx.fufei-monitor`
- 存取 access token（用户 `fufei-account`）和 refresh token（用户 `fufei-refresh`）
- `clear_tokens` 清除两者

### 窗口与 UI 配置 (`tauri.conf.json`)

- 340 宽无边框不透明窗口，常驻顶层，不出现在 Dock 栏（高度自适应内容）
- `LSUIElement=true`（通过 `macOSPrivateApi`）实现纯菜单栏应用
- 打包格式：`.app` + `.dmg`

### macOS 托盘行为注意事项

在 macOS 上，给 `TrayIconBuilder` 挂载 `Menu` 会导致左键点击永远弹出菜单，即使设置 `show_menu_on_left_click(false)` 也无效，`on_tray_icon_event` 的 Left 事件不会触发。因此本应用**不使用托盘菜单**，所有交互（弹出面板、退出）均通过左键点击托盘图标或 UI 内按钮完成。

## 开发

```bash
# 安装前端依赖
pnpm install

# 开发模式
pnpm tauri dev

# 构建发布包
pnpm tauri build
```

## 前置要求

- macOS 11.0+
- Node.js + pnpm
- Rust toolchain（rustup）
- Xcode Command Line Tools（keyring crate 依赖）