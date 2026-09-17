# 技术选型

### 技术选型清单

| 功能领域 | 选定技术 / Crate | 版本 | 核心作用与理由 |
| :--- | :--- | :--- | :--- |
| **GUI 框架** | **Slint** | `1.5` | 原生 Rust 编写，非 Webview 方案，编译为本地机器码，内存占用仅约 15MB，支持 Linux 无边框透明窗口。 |
| **异步运行时** | **Tokio** | `1.36` | 工业级异步 I/O 引擎，提供非阻塞线程池与跨线程通信 Channel（`mpsc` / `oneshot`）。 |
| **网络请求** | **Reqwest** | `0.11` | 异步 HTTP 客户端，选用 `rustls-tls` 特性（纯 Rust 实现），杜绝 Linux 下 OpenSSL 动态链接库版本冲突。 |
| **全局快捷键** | **global-hotkey** | `0.5` | 跨平台全局键盘监听，底层封装 Linux X11/XCB 与 Wayland 协议事件。 |
| **系统托盘** | **tray-icon** | `0.14` | Linux 状态栏图标常驻与右键菜单，原生 DBus / StatusNotifierItem 实现。 |
| **文本注入引擎** | **Kitty IPC** + **Enigo** | `std` / `0.2` | Kitty 终端优先走原生 Unix Socket（`kitty @ send-text`），其他桌面应用降级走 `enigo` 模拟按键。 |
| **配置与序列化** | **Serde** + **Toml** | `1.0` / `0.8` | 编译期强类型反序列化，将 `config.toml` 直接映射为 Rust 结构体。 |
| **数据解析** | **serde_json** | `1.0` | 极速解析 API 返回的 JSON 候选词数组。 |
| **本地缓存** | **lru** | `0.12` | 内存 O(1) 访问的 LRU 缓存，避免重复拼音多次发起网络请求。 |
| **错误系统** | **thiserror** | `1.0` | 派生宏生成强类型枚举错误，替代复杂的异常树。 |
| **日志与追踪** | **tracing** | `0.1` | 结构化非阻塞日志系统，便于开发调试与运行时排错。 |


---

# 项目结构

```bash
pypin/
├── Cargo.toml                  # 依赖与编译元数据
├── build.rs                    # Slint 预编译脚本 (编译期将 .slint 转换为 Rust 代码)
├── config.toml                 # 运行期默认配置文件
│
├── ui/                         # [Layer 1: 视觉展现层] (纯 DSL 声明式界面)
│   ├── app_window.slint        # 主悬浮窗、输入框、候选词列表布局与交互定义
│   └── styles.slint            # 调色板、圆角、字体尺寸等设计系统变量
│
└── src/                        # [Rust 核心源码]
    ├── main.rs                 # 应用程序入口：初始化 Tokio、加载配置、拼装四大层
    ├── config.rs               # [Layer 2] 对应 config.toml 的数据结构定义
    ├── error.rs                # 全局统一 Error 枚举 (使用 thiserror)
    │
    ├── ingress/                # [Layer 1: 接入层适配]
    │   ├── mod.rs
    │   ├── hotkey.rs           # 全局快捷键监听事件源 (GlobalHotkey 包装)
    │   ├── tray.rs             # 托盘图标生命周期管理与右键菜单事件
    │   └── ui_bridge.rs        # Slint 句柄与线程事件调度 (Weak<AppWindow> 通信包装)
    │
    ├── coordinator/            # [Layer 2: 核心控制层]
    │   ├── mod.rs
    │   ├── coordinator.rs      # 核心协调器：汇聚各层事件，驱动状态迁移
    │   └── state.rs            # 状态机定义 (Hidden / Resolving / Presenting 等)
    │
    ├── pipeline/               # [Layer 3: 翻译业务管线层]
    │   ├── mod.rs              # 统一 Trait: `Translator` 接口定义
    │   ├── engine.rs           # Prompt 组装引擎 (组装 Pinyin -> Candidates 规范)
    │   ├── cache.rs            # 本地 LRU Cache 包装 (线程安全包装)
    │   └── providers/          # 翻译服务提供商实现
    │       ├── mod.rs
    │       ├── llm_openai.rs   # 兼容 OpenAI/DeepSeek 协议的实现
    │       └── traditional.rs  # 传统翻译 API 实现 (如百度、Bing 等)
    │
    └── platform/               # [Layer 4: 操作系统驱动与抽象层]
        ├── mod.rs
        ├── focus.rs            # 窗口焦点监控器与控制 (X11 / Wayland)
        └── injector/           # 文本注入抽象与多驱动实现
            ├── mod.rs          # `TextInjector` Trait
            ├── kitty_ipc.rs    # Kitty Unix Socket 注入器 (零延迟、原生)
            └── generic_sim.rs  # Enigo 虚拟按键模拟注入器 (通用桌面降级)
```