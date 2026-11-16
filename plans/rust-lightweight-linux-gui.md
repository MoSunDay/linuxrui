# Rust 极轻量 Linux 桌面组件实施计划（Wayland-only）

> 状态：工程计划，非完成报告；所有能力、预算与排期均须经原型和测量验证。
>
> 产品形态：从 launcher 起步，逐步增加 panel、notification、settings 的模块化轻量桌面组件；不是通用 GUI toolkit，也不是一次性重写完整 XFCE。

## 1. 执行摘要与明确决策

首版采用 **Wayland-only**：常规 launcher/settings 窗口使用 `xdg-shell`；panel、launcher overlay、通知 popup 使用 `wlr-layer-shell-unstable-v1`。完整 shell 能力先只面向 **Sway/labwc（wlroots）**，首版不实现 compositor、WM、session 或完整文件管理器。普通窗口可运行于其它现代 Wayland compositor，但不因此承诺 layer-shell、tasklist 等能力。

默认技术路线为 **Slint + 软件渲染/`wl_shm` 思路 + zbus**，必要时使用 `wayland-client`、`wayland-protocols`、`wayland-protocols-wlr`、`smithay-client-toolkit`（SCTK）。不用 WebView，不默认引入 GPU、wgpu 或 Skia，iced/wgpu 也不作为默认方案。

Slint 与 SCTK 能否在**同一进程、同一 Wayland 连接和事件循环所有者**下工作，由 M0 真实纵切证明。Slint 自身集成失败可归档并切到 **裸 GTK4 + CSS + gtk4-layer-shell**；只有两路线均不能通过共同硬门槛才阻塞 M0。仍坚持 Wayland-only，libadwaita 非必选，不保留旧 GUI 栈。

Rust 实现不等于依赖闭包零 C：Wayland、字体、输入法、无障碍及工具包仍可能引入 C 库。不宣称绝对内存排名，不在没有可复现数据时宣称性能达标。

### 决策清单

| 编号 | 决策 |
| --- | --- |
| D1 | 仅实现 Wayland client/shell；Sway/labwc 是首期完整能力目标，按实际协议检测启用功能。 |
| D2 | 默认验证 Slint；其集成失败只触发 GTK4 路线切换，最终选定后端须通过全部共同硬门槛。 |
| D3 | UI 与 headless core 分离；D-Bus 使用 zbus；配置、桌面项解析和策略逻辑可独立测试。 |
| D4 | launcher/settings 按需、panel 少量共享常驻；不建设“一组件一 daemon”或配置 daemon。 |
| D5 | 仅借鉴 XFCE 职责划分与 freedesktop 标准，不承诺 XFCE API、插件 ABI 或配置兼容。 |

## 2. 目标、非目标与支持矩阵

### 2.1 目标

- 提供启动快、空闲开销低、视觉简约且可键盘完整操作的 launcher。
- 在相同 core 上逐步交付 panel、标准通知服务和按需 settings，而非复制完整桌面环境。
- 正确支持中文 IME、Orca/AT-SPI、多显示器、混合 DPI、分数缩放、输出热插拔与剪贴板。
- 复用 `.desktop`、XDG、D-Bus、SNI、Notifications 及 Thunar、logind、UPower 等现有标准和服务。
- 以能力检测和显式降级处理 compositor 差异，以自动化和人工证据支撑发布。

### 2.2 非目标

- 不编写自有 compositor/WM，不接管 session 生命周期，不实现完整文件管理器或桌面图标系统。
- 不提供 X11、GTK3、EWMH、XEmbed 兼容路线，不以 XWayland 补救功能缺失。
- 不逐控件翻译 XFCE，不复刻 `xfconfd`，不实现 XFCE 面板插件兼容层。
- 首版不做后台全文索引、完整 XDG Menu XML、动态库插件稳定 ABI、自有同步或跨设备配置。
- Smithay 若未来用于完整会话或自有 compositor，必须另立项目、威胁模型和验收计划。

### 2.3 能力与 compositor 支持矩阵

| 场景 | 协议/机制 | 首版承诺与降级 |
| --- | --- | --- |
| 普通 launcher、settings | `xdg-shell`、可用时 `xdg-activation-v1` | 面向现代 Wayland compositor；无 token 时可启动但不保证焦点。 |
| Sway/labwc 完整 shell | `xdg-shell` + wlr layer-shell | 首期主要验收目标；运行时检测全局协议和版本，不只按 compositor 名称放行。 |
| panel、launcher overlay、通知 popup | `wlr-layer-shell-unstable-v1` | 验证 anchor、layer、exclusive zone、键盘交互与焦点释放；缺协议时不承诺完整 shell 行为。 |
| launcher 入口 | compositor keybinding 调用 `launcher-app --toggle` | layer-shell `keyboard_interactive` 只管 surface 键盘，不是全局热键；不监听全局输入或另造快捷键协议。 |
| tasklist | `wlr-foreign-toplevel-management-unstable-v1` | M2 做 wlr adapter；缺协议即隐藏并诊断，不伪造通用窗口列表。 |
| 后续标准 tasklist | 合适的 `ext-*` 协议 | 仅列后续 adapter，未实现、未实测前不写入支持声明。 |
| 缩放与输出 | `fractional-scale-v1` + `viewporter` + output scale | 由后端处理并实测窗口跨屏、layer surface 重配、热插拔和混合 DPI。 |
| 中文输入 | 客户端 `text-input-v3`；compositor/IME 桥接及工具包 IM module | M0 固定并实测至少一个参考组合（建议 Sway + Fcitx5），相同环境比较路线；仅承诺矩阵中已实测项。 |
| 托盘 | SNI over D-Bus；最小 DBusMenu client | shell 内建轻量 Watcher + Host；已有 Watcher 时 Host 对接，不抢名；菜单子集未验收则发布关闭 tray。 |
| 剪贴板与无障碍 | Wayland data-device、AT-SPI | 首版仅承诺来源存活期间 Unicode 复制/粘贴、大文本限制与不阻塞 UI；外部 manager 保留须检测并实测。 |

普通窗口可用与完整 shell 可用必须在文档、日志和设置中区分。IME 参考组合通过而其它组合失败记为平台限制；目标部署指定的组合仍是硬门槛，中文输入和 Orca/AT-SPI 要求不降低。首版不自建 clipboard manager。

## 3. XFCE 组件映射与首版取舍

| XFCE mirror | 借鉴点 | 首版取舍 |
| --- | --- | --- |
| [libxfce4util](https://github.com/xfce-mirror/libxfce4util) | 公共工具、XDG 路径与资源定位 | Rust 公共 crate 提供基础能力，不移植 API。 |
| [libxfce4ui](https://github.com/xfce-mirror/libxfce4ui) | 共享视觉和对话框约定 | 用语义 token 与自有组件，不逐控件翻译。 |
| [xfconf](https://github.com/xfce-mirror/xfconf) | 类型化配置、订阅、按域拆分 | 用 serde、原子文件和变更通知；不读取 XFCE 配置。 |
| [garcon](https://github.com/xfce-mirror/garcon) | 桌面项元数据与应用分类 | 只解析 launcher 所需 `.desktop`/Actions，不做完整 Menu XML。 |
| [exo](https://github.com/xfce-mirror/exo) | 启动与桌面项辅助能力边界 | 实现所需安全启动和标准解析，不复制 exo。 |
| [xfce4-panel](https://github.com/xfce-mirror/xfce4-panel) | panel 布局和内建插件边界 | 少量静态 Rust trait 模块，不兼容原面板插件。 |
| [xfwm4](https://github.com/xfce-mirror/xfwm4) | 窗口管理与 shell/UI 边界 | 窗口管理交给 compositor。 |
| [xfdesktop](https://github.com/xfce-mirror/xfdesktop) | 壁纸与桌面呈现拆分 | M3 仅可选 wallpaper；不做桌面图标和文件管理。 |
| [xfce4-session](https://github.com/xfce-mirror/xfce4-session) | 会话启动顺序 | 交给外部 session/compositor；按需进程与 Notifications activation 即可。 |
| [xfce4-settings](https://github.com/xfce-mirror/xfce4-settings) | 设置分域和即时反馈 | 按需 settings 仅覆盖本项目，不做全系统控制面板。 |
| [xfce4-power-manager](https://github.com/xfce-mirror/xfce4-power-manager) | 状态展示与策略服务解耦 | 复用 UPower/logind，不另写电源策略 daemon。 |
| [xfce4-notifyd](https://github.com/xfce-mirror/xfce4-notifyd) | 通知入口、展示与动作解耦 | M2 实现 Notifications 服务及 popup，控制不可信内容。 |
| [thunar](https://github.com/xfce-mirror/thunar) / [tumbler](https://github.com/xfce-mirror/tumbler) | 文件管理与缩略图异步化 | 复用现有服务；自有不可信缩略图 worker 后置。 |
| [xfce4-appfinder](https://github.com/xfce-mirror/xfce4-appfinder) | 轻量查找、排序和执行 | 指导 M1 prefix/subsequence 与 Actions，不复制其配置。 |

同名概念不代表 XFCE API、配置路径、主题格式或插件 ABI 兼容。

## 4. 技术决策与 P0（M0）同一纵切原型闸门

### 4.1 技术决策表

| 领域 | 默认方案 | 约束/替代 |
| --- | --- | --- |
| UI | Slint，锁版与最小 feature | 自身集成失败则裸 GTK4 + CSS；共同门槛不在切换时降低。 |
| 渲染 | 软件渲染、共享内存 `wl_shm` 思路 | 不默认 GPU/wgpu/Skia；实际缓冲路径由原型和依赖证据确认。 |
| Wayland | wayland-client/protocols、必要 wlr protocols、SCTK | 只引入实际使用协议；SCTK 与 Slint 集成由 M0 决定。 |
| 回退 shell | GTK4 + gtk4-layer-shell | 仅 Wayland；同样验证 IME、AT-SPI、缩放、软件渲染和协议行为。 |
| D-Bus/async | zbus，优先默认执行器 | 若强制 Tokio，workspace 统一版本/feature；禁止各 crate 私建 runtime。 |
| 数据 | serde、XDG、原子配置、文件事件 | headless core 不依赖 UI 工具包，不先造配置 daemon。 |
| 扩展 | 首期内建 Rust trait、静态模块 | 暂不承诺动态库 ABI；未来插件另做版本和沙箱设计。 |
| Web/索引 | 无 WebView、无后台全文索引 | launcher 只扫描规定目录并按事件刷新。 |

**许可闸门：** Slint 的 Royalty-free、GPLv3、Commercial 三选一不得误写为 MIT/Apache。发布时固定 Slint tag/commit 并留存对应许可文本和审阅决定；Royalty-free 的披露/展示要求落实到 About、NOTICE 和发布物。可变 `master` 链接仅作在线阅读入口，不作为归档依据；许可不可接受即选 GTK4。

### 4.2 M0 原型必须一次纵切验证

每个候选路线在同一可执行程序中完成：一个 UI 事件循环所有者、一个 Wayland 连接所有者，zbus/headless core 并发工作；真实显示普通窗口和软件渲染 layer-shell 表面。不得以离屏图片、两个转发连接或“编译通过”代替。

| 共同硬门槛 | 最终选定后端的通过证据 |
| --- | --- |
| layer-shell 真实集成 | Sway 与 labwc 上显示、重配和销毁 panel/overlay；日志证明单 owner，无嵌套循环竞争；集成方式有 ADR。 |
| 键盘与焦点 | Tab/方向键/Enter/Esc 可操作；overlay 正确取得并释放键盘，通知不无意偷焦点；记录 focus-loss。 |
| 中文 IME | 固定参考组合及目标指定组合（如有），验证预编辑、提交、取消、cursor rectangle、候选窗定位、移动/缩放/跨屏；归档 `compositor × IME` 矩阵。 |
| 无障碍 | Orca/AT-SPI 读出角色、名称、状态、列表变化和焦点，纯键盘无陷阱；不能只检查依赖存在。 |
| HiDPI/几何 | integer/fractional scale、混合 DPI、跨输出、热插拔无模糊错位；region、anchor、exclusive zone 经协议与状态断言验收。 |
| 剪贴板 | 来源存活期间 Unicode、大文本限制和不阻塞 UI；来源退出后的外部 manager 保留只是条件能力，不作选型或核心阻断项。 |
| 后端与渲染 | 运行时证明实际为 Wayland，软件路径可复现；feature 未误带 WebView、默认 GPU 或旧 GUI 栈。 |
| 许可与依赖 | 锁版许可、crate/原生动态库闭包、NOTICE 草案完成。 |

M0 的通过定义是**最终选定后端的共同硬门槛全部通过**。Slint 自身集成失败须留档后切 GTK4，不等于 M0 必然失败；两路线的共同门槛均失败才暂停后续 GUI 阶段。M0 同时固定 runtime、协议版本、测量方法和 GUI 选型 ADR。

## 5. 架构与进程边界

### 5.1 核心原则

- **UI/headless 分离：** core 只处理模型、配置、解析、排序和 D-Bus 类型；UI adapter 独占协议表面与工具包对象。
- **统一 async：** 每进程一个统一 runtime；阻塞 I/O、解析和解码进入有界队列，不阻塞 UI。
- **配置格式：** 遵循 XDG 路径；整数 `schema_version` 经逐版本纯函数迁移。写锁内重读 revision，再备份、flush/fsync、同目录原子替换并按需同步目录。
- **配置竞争：** 未来版本不得被旧进程覆盖；N-1→N→N-1 回滚时旧进程只读识别新增字段或明确拒绝写入，绝不重置。冲突重放单域修改，不用旧快照覆盖。
- **变更监听：** 监听父目录以承受 rename；事件溢出强制重读。非法候选保留现场并维持最后可用状态，不静默清空。
- **桌面项与启动：** core 实现第 6.1 节契约；目录扫描有界、可取消，不持续全盘爬取。
- **日志与隐私：** 默认不记原始查询、通知正文、clipboard、URL/Exec 敏感参数，无遥测；诊断使用阶段、错误类别和必要安全元数据。

### 5.2 workspace crate 与进程

以下名称为逻辑边界，不在本计划阶段创建代码：

| crate/模块 | 职责 | 所属进程 |
| --- | --- | --- |
| `core-model` / `config-core` | 纯数据、trait、XDG 配置与迁移 | 被前端复用，无独立进程 |
| `desktop-entry` / `launch-core` | 解析、去重、排序、安全启动 | launcher/settings 测试复用 |
| `dbus-core` | zbus、logind/UPower/SNI/Notifications 类型 | 按功能链接 |
| `wayland-shell` | 单连接/事件循环 owner、surface、能力检测 | 唯一 UI adapter |
| `ui-slint` 或 `ui-gtk4` | M0 后二选一并固定 feature | 各 UI 可执行程序共享 |
| `launcher-app` / `settings-app` | 按需 launcher 与设置 | 各自单例进程 |
| `shell-app` | panel、通知 popup、SNI Watcher + Host | panel 常驻或 notify-only 按需 |
| `media-worker`（后置） | 壁纸/不可信缩略图隔离 | 有界短生命周期 worker |

```text
launcher-app / settings-app / shell-app
              │ 调用纯 Rust core；UI 侧仅一个 Wayland owner
              ├── wayland-shell ── xdg-shell / layer-shell / xdg-activation-v1
              ├── zbus ── logind / UPower / Notifications / SNI
              ├── config-core ── XDG 原子文件 + 文件事件
              └── 有界队列 ── 按需 I/O 或隔离 worker（不直接操作 UI）
```

`xdg-activation-v1` token 只由 UI/Wayland adapter 获取，`launch-core` 接收不透明启动上下文。普通 launcher 可消费 compositor 注入的 token；layer-shell overlay 依靠其键盘交互语义，不靠 token 抢焦点。启动目标时 adapter 基于有效 serial 获取新的未消费 token，经 `XDG_ACTIVATION_TOKEN`/`DESKTOP_STARTUP_ID` 只注入目标环境；不缓存、复用或泄露给其它子进程，无 token 仍可启动但不保证焦点。

### 5.3 会话部署与生命周期

- launcher 与 shell 在每图形会话内单例，settings 也固定单例。第二次 launcher 调用向 owner 传 toggle 和可用 activation token 后退出，不叠 surface；应用内串行处理并去重。
- 参考部署仍要求每会话独立 session bus；共享 bus 时，launcher/settings 可用包含 Wayland session 身份的自定义单例键隔离，禁止跨会话转发，但 `org.freedesktop.Notifications` 与标准 SNI Watcher 是总线级固定名称，共享 bus 并行多图形会话时这些标准服务只能有唯一 owner；不承诺完整通知/托盘会话隔离，只提供明确降级。owner 崩溃后释放键并允许重建，测试崩溃恢复与不同会话隔离。
- shell 的 autostart 与 Notifications D-Bus activation 共用同一单例入口；panel 模式常驻，notify-only 在队列空、无 pending action 后经宽限退出。退出期间新请求撤销退出，崩溃后可重激活。
- Wayland 或 session bus 断连即清理退出，不忙重试；panel 异常退出可由外部 supervisor 有界退避重启，正常退出不重启。
- panel 不靠 D-Bus activation 自动启动，launcher/settings 不加入常驻 autostart。参考启动路径只选 compositor autostart 一种，禁止 Sway/labwc、service file 等多路径重复拉起同一模式。
- 启动前置为 session bus 与 Wayland socket 就绪，并在 activation environment 配置 `WAYLAND_DISPLAY`、`XDG_CURRENT_DESKTOP`；不强制 systemd。

## 6. 产品路线与交互约束

1. **Launcher：** 按需读取有界缓存/目录快照，无后台全文索引；排名先 prefix 再 subsequence，名称/频率次级规则确定且可复现。
2. **频率历史：** 首版默认仅内存，关闭即清空；未来若落盘才另定 `$XDG_STATE_HOME`、0600、容量和清除契约，不提前宣称实现。
3. **Panel：** 少量内建模块；tasklist 缺协议即隐藏。tray 内建轻量 SNI Watcher + Host 以支持最小 Sway/labwc，已有 Watcher 时不抢名而由 Host 对接。
4. **SNI 生命周期：** 处理 Item/Host 消失、重复注册和 Watcher 重启重注册；仅交付最小 `com.canonical.dbusmenu` client 子集并测试，未通过 M2 验收则发布关闭 tray。
5. **Notification：** 支持 D-Bus activation、replace id、action、关闭原因与有界 popup；已有 `org.freedesktop.Notifications` owner 时不抢名、不双弹窗，panel 继续并显示诊断。
6. **Settings：** M3 按需打开，每项标注“立即/下次打开/重启组件/compositor 外部管理”，区分已保存与已应用；热键由 compositor 管理，不得伪报 settings 已应用。
7. **可选能力：** wallpaper/worker 未交付记 N/A、发布时关闭且不阻塞核心；只有启用时才要求资源上限和畸形文件证据。

### 6.1 首版行为契约

- **入口与退出：** M1 提供 Sway/labwc keybinding 示例调用 `launcher-app --toggle`；支持的 compositor 绑定禁用 auto-repeat，应用仍串行/去重。选择项目、Esc 或可配置失焦退出；IME 预编辑、组合输入和错误对话框存在时不得因失焦误关。
- **可见性：** 仅合法 `Type=Application` 进入 launcher；高优先级 `Hidden=true` 遮蔽同 desktop ID 的低优先级项。按规范处理 `NoDisplay` 及 `OnlyShowIn`/`NotShowIn`，`$XDG_CURRENT_DESKTOP` 是冒号分隔多值；未设置、冲突键和损坏项均入 fixture。
- **扫描与本地化：** `TryExec` 在扫描及启动前检查。解析本地化 Name/GenericName/Comment/Keywords/Action，locale 回退同时用于显示、搜索和 `%c`；非法 Action 只丢弃该 Action 并诊断，不拖垮整项或扫描。
- **Exec：** 覆盖 `%f/%u/%F/%U/%i/%c/%k/%%`，按规范删除废弃代码（`%d/%D/%n/%N/%v/%m`），未知代码拒绝。argv 直接创建进程，不做 shell 拼接；`Terminal=true` 参考 Wayland 原生 `foot` argv adapter，未安装可见失败；`Path` 必须 chdir 成功。
- **D-Bus 启动：** `DBusActivatable=true` 调 `org.freedesktop.Application` 的 `Activate(a{sv})`/`ActivateAction(s, av, a{sv})`；Desktop Entry Action 无额外参数时 `av` 为空；platform-data 中 `desktop-startup-id` 为 string variant，有 token 才写入，无 token 省略；失败可见，MVP 不自动 Exec 回退，超时或结果未知严禁重试造成双启动。
- **启动结果：** 可执行不存在、权限、chdir、exec、D-Bus 各阶段可诊断；成功 spawn 只表示已创建，不伪报应用持续正常。launcher 退出不杀目标，直接子进程退出状态须回收且不遗留僵尸。
- **通知隐私：** 首版不持久化通知正文、无遥测。锁屏感知是条件能力；若参考部署承诺锁屏隐私，必须绑定实测 locker 与 logind `LockedHint` adapter，通知进程不抢 `ext-session-lock` locker 角色。
- **锁定行为：** 锁定时销毁已有 popup、遮蔽新正文并禁动作，状态不可信时保守遮蔽；无可信 adapter 不得宣称锁屏隐私，并须在发布说明醒目标注。测试已有 popup 时锁定、锁中通知和解锁。

## 7. 视觉、主题、i18n 与可访问性 token

- 显式项目设置优先；跟随系统时使用已实测 XDG Settings Portal appearance，缺 portal 用内置回退而非硬依赖，并测试无 portal 与运行时切换。
- fontconfig 解析字体、freedesktop icon-theme 查图标，提供内置回退；不捆绑大体积字体或图标集作为硬依赖。
- light/dark/high contrast 共用语义 token；普通文本对比度至少 4.5:1，大文本及关键控件/焦点至少 3:1，人工检查但不宣称完整 WCAG 认证。
- 禁止背景模糊和持续装饰动画；短过渡可中断，reduced-motion 本地设置可覆盖跟随系统。
- 自有 UI、可访问名称和错误使用消息 ID，不拼接最终句子；定义 locale 与英文回退，首版英文 + zh_CN。
- clock 日期时间和 12/24 小时制跟随 locale 并允许用户覆盖；完整 RTL 与区域排序后置，截断不得隐藏关键操作或可访问名称。
- 通知动作可经 panel Tab 到通知入口，或由 compositor 绑定调用 `shell-app --focus-notifications`；交互暂停过期，关闭恢复原焦点，Orca 验证正文、动作和状态。
- 所有操作有键盘路径和可访问名称，颜色不是唯一状态提示；焦点不被动画或 popup 无意重置。

## 8. M0–M4 阶段、交付物与退出条件

时间仅为**单人或小团队初始估算**，M0 后依据实测重校准。MVP 约 **8–12 周**，指 launcher + 基础 panel/notify/settings 的可验证组合，不是完整 DE。

| 阶段/范围 | 交付物 | 退出条件与测试证据/命令 |
| --- | --- | --- |
| **M0 技术闸门（1–2 周）** | Slint 纵切、必要 GTK4 对照、选型/runtime/许可 ADR、基线工具；规划并提交 `rust-toolchain.toml`、`Cargo.lock` | **最终选定后端共同硬门槛全通过**；Slint 集成失败可归档切 GTK4，两路线共同门槛均失败才阻塞；`cargo test --workspace --locked`、依赖树和 `ldd` 归档。 |
| **M1 core + launcher（2–3 周）** | desktop/config/launch core、launcher、Actions、两种 surface 模式、Sway/labwc 禁用 auto-repeat 的 binding 示例 | 第 6.1 节解析/启动/locale fixture 全过；单例 toggle/token 转发、选择/Esc/失焦/IME、owner 崩溃和会话隔离测试通过；launcher→app activation 与僵尸检查通过；形成 M1 数值 resource-limits 表；无后台索引。 |
| **M2 panel + notifyd（2–3 周）** | panel、tasklist、SNI Watcher + Host/最小 DBusMenu、Notifications、notify-only 生命周期 | 几何/热插拔/缺协议降级、服务名冲突、SNI 重注册通过；通知恶意输入、sender 所有权、并发激活/退出、崩溃重激活通过；notification action→app activation 和锁定/解锁测试通过；补齐 M2 数值表，DBusMenu 未验收则关闭 tray。 |
| **M3 settings + 可选 wallpaper（1–2 周）** | settings、迁移/冲突恢复、主题/i18n/协议诊断；可选 wallpaper/worker | 原子写入、未来版本/回滚、rename/溢出、保存与应用状态验收；无 portal、运行时主题与 reduced-motion 测试通过。可选能力仅启用时要求资源/畸形文件证据；未交付为 N/A 且发布关闭，不阻塞核心。 |
| **M4 加固发布（约 2 周）** | 精确版本 Debian/Fedora 包、完整部署清单、许可/NOTICE/SBOM、性能报告与限制 | 全量 CI、IME/Orca/多屏 gate、发布硬预算、升级/卸载通过；真实全新登录及注销再登录验证单一路径、环境、单例和回收；无可信锁屏 adapter 或关闭 tray 等限制醒目标注，无 secrets。 |

每阶段保留发行版、kernel、compositor、工具包锁版、协议日志、原始输出和复现命令。截图只佐证几何与交互，不替代 a11y、IME 或协议断言。

## 9. 隐私、资源上限与性能预算

> 以下预算是待验证的发布目标，不是现有成绩。M0 可用 ADR 校准并说明数据，M1 起冻结启动、PSS、idle 为发布硬预算；未测项写“未知”，不得静默放宽。

### 9.1 固定环境和口径

- 固定 x86_64、4 核、8 GiB 专用 VM，记录 CPU/频率策略、精确 Debian/Fedora 版本与快照、kernel、compositor、Mesa、字体和工具包；两组数据不混合。
- 固定 fixture：最终后端与默认 launcher 模式、300 个有效桌面项加重复/隐藏/损坏边界项、历史为空、固定 panel 模块/toplevel/SNI 数量、locale/字体，单输出 1920×1080、scale=1；多屏另测。
- release profile 固定 opt-level、LTO、codegen-units、panic、strip/debuginfo；同构建和 feature 比较，禁止混用 debug/release。
- 启动从创建进程到首个可操作 UI 帧并响应握手，报告 P50/P95，warm/cold 各至少 30 次；panel 会话启动与通知 D-Bus 冷激活分开计时。
- cold 必须使用 runner 在每个 measured sample 前受控重置，保持 compositor/session bus 运行，只受控清页缓存，不删除 font/icon cache；确认旧进程退出，若测通知还须确认 `org.freedesktop.Notifications` 名称已释放；首轮建缓存另测，不与 cold 稳态混算。不能把同一 hyperfine 命令直接当作 30 次 cold。
- PSS 以 `/proc/<pid>/smaps_rollup` 或 smem 为主，RSS 单列；核心常驻名单仅为 `shell-app`（panel 模式）及其明确线程，不把按需 launcher/settings/worker 算入。
- idle 测 10 min 无可视变化/通知/输入场景，另测时钟变化；wakeups 用归档的 bpftrace/perf sched 脚本按“被唤醒任务的 TGID”聚合，覆盖目标 TGID 全线程并报告次数/600s；不得误用 bpftrace 当前上下文 `pid`（通常是唤醒者），不声称 pidstat 可直接完成。
- crate、GTK/Wayland/font 等原生动态库和包体积全量留档；依赖体积是观测项，增长超过 10% 触发审查，不冒充已验证性能硬预算。

### 9.2 初始发布预算

| 指标 | 初始门槛/记录要求 |
| --- | --- |
| launcher 新进程启动，warm / cold | warm P50 ≤150 ms、P95 ≤250 ms；cold P50 ≤400 ms、P95 ≤700 ms。 |
| settings 新进程启动，warm / cold | warm P50 ≤200 ms、P95 ≤350 ms；cold P50 ≤500 ms、P95 ≤800 ms。 |
| panel 会话启动，warm / cold | warm P50 ≤250 ms、P95 ≤400 ms；cold P50 ≤600 ms、P95 ≤900 ms。 |
| 通知 D-Bus 冷激活到可接收请求 | 单独报告 cold P50/P95，初始预算 P50 ≤600 ms、P95 ≤900 ms；不与 panel 启动合并。 |
| 稳定 PSS | launcher ≤50 MiB；shell-app panel 模式（含通知/SNI）≤60 MiB；settings ≤70 MiB；核心常驻合计 ≤100 MiB。 |
| 10 min idle | 核心常驻合计平均 CPU ≤0.1%，wakeups 平均 ≤2 次/s，同时给出 600s 原始计数；外部事件和周期 UI 分开报告。 |
| 退出与回落 | launcher/settings 退出后 PSS 为 0，总 PSS 回基线 ±5 MiB；100 次通知/launcher 循环后常驻 PSS 净增 ≤5 MiB。 |
| 依赖闭包 | 观测 crate/动态库/installed-size；不得误带 WebView、默认 GPU 或 X11 client feature，增长超 10% 必须审查。 |

建议命令与证据：

```sh
# warm
hyperfine --warmup 3 --runs 30 '<启动探针，首帧握手后退出>'
smem -P '<进程名>'
cat /proc/<pid>/smaps_rollup
pidstat -p <pid> 1 600
cargo tree --locked -e features,normal,build
ldd <可执行文件>
```

`pidstat` 只用于其能直接提供的 CPU 等数据；唤醒须另附 bpftrace/perf sched 脚本和 TGID 原始结果。没有原始数据即写“未验证”。

### 9.3 M1/M2 数值 resource-limits 表

下表是必须填实测定值并转为发布配置的模板，当前不虚构已验证数值；每项同时归档正常、边界和超限 fixture。

| 资源 | 阶段 | 必须确定的数值字段 | 超限与所有权契约 |
| --- | --- | --- | --- |
| desktop 文件/扫描缓存 | M1 | 单文件字节、项数、缓存项数/总字节、扫描时间 | 超大文件 reject，缓存有界淘汰；坏项隔离诊断，不阻断其它项。 |
| clipboard | M1 | 单 offer 与总缓冲字节、读取时间 | 超限 reject 或按明确文本策略 truncate，不阻塞 UI；仅当前来源可撤销 offer。 |
| 通知文本/action/速率/队列 | M2 | 正文字节、action 数、每 sender/全局速率、队列数/字节 | 文本 truncate，非法 action drop/reject，洪泛与队列超限 drop 并诊断；sender 只能 replace/close/触发自己的通知。 |
| 通知/SNI 图片 | M2 | 编码字节、解码像素/字节、解码时间 | 任一维度超限 reject 图片并保留安全占位，不拖垮服务；远程资源默认不取。 |
| 可选 wallpaper/worker | M3 仅启用时 | 输入/输出字节、像素、CPU/内存/时间 | reject/drop 并终止隔离 worker；未启用为 N/A。 |

## 10. CI、测试与发布

### 10.1 自动化层次

- **Unit：** desktop ID/优先级、环境多值、locale、Exec/Action、排序、迁移/回滚/原子替换/冲突、通知限速和 sender 所有权。
- **Integration：** 隔离 session bus 测 Notifications、SNI/DBusMenu、logind/UPower mock、D-Bus activation、服务名冲突和对象生命周期。
- **Headless UI：** 统一 wrapper 自举独立 `XDG_RUNTIME_DIR`/`WAYLAND_DISPLAY`、虚拟输出/输入、compositor 配置与清理；取消 `DISPLAY`，GTK 显式 `GDK_BACKEND=wayland`，运行时断言实际后端。
- **协议矩阵：** 模拟缺少 layer-shell、foreign-toplevel、fractional-scale、xdg-activation、portal，验证可观察降级；token 不缓存、不复用、不跨子进程泄露。
- **压力/故障：** 重复 toggle/启动退出、auto-repeat 风暴、100 次通知、目录事件风暴/溢出、配置损坏/未来版本、对端消失、owner 崩溃、并发激活退出和断连。
- **会话与隐私：** 测 launcher→app、notification action→app，已有 popup 时锁定/锁中通知/解锁，以及真实全新登录、注销再登录；截图不替代语义断言。

### 10.2 锁版 CI 与人工 gate

每次变更至少运行：

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo audit
cargo deny check
cargo tree --locked -e features,normal,build
ldd <每个发布可执行文件>
```

CI/打包统一使用 `--locked`。依赖检查确认未误引入 X11 client；不要把 Wayland 也需要的 `libxkbcommon` 误判为 X11。发行版 GTK 多后端包的单列基线不能伪称为项目启用了其它后端。

固定精确 Debian/Fedora 版本的最小 VM 做打包和运行验收，不使用永远滚动的环境标签。容器不能替代真实 session；参考及目标 IME 组合、Orca/AT-SPI、纯键盘、缩放/多屏热插拔、layer-shell 焦点、锁屏 adapter 和来源存活期间剪贴板必须人工签字。

### 10.3 发布物与供应链

- 部署清单包含 launcher/settings desktop files、图标/翻译、Notifications session-bus service file，以及 Sway `exec`/labwc autostart 的 shell 示例；示例只选一种参考启动路径并写明环境前置。
- 发布 LICENSE、NOTICE、固定 Slint tag/commit 对应文本及所需披露；SBOM、NOTICE 和漏洞审查同时覆盖 Rust crate 与 GTK/Wayland/font 等原生动态库。
- `cargo vendor` 是否采用由发行版策略决定，vendored 源码仍须审计；构建说明固定 toolchain 和锁文件。
- 发布物与日志不得包含 token、密码、私钥、用户配置、通知正文或测试个人数据；CI 执行 secrets 扫描。
- 发布说明列出已测 compositor/协议、降级模块、未达预算和已知限制；无可信锁屏 adapter 时醒目标注，不把 roadmap 写成已完成。

## 11. 风险与回退

| 风险 | 早期信号 | 应对/回退 |
| --- | --- | --- |
| Slint 集成不成立 | 第二连接/嵌套循环、frame 停滞、surface 无法重配 | 归档并切 GTK4；不因路线切换降低共同门槛，两路线均失败才阻塞 M0。 |
| IME/a11y 不足 | 预编辑丢失、候选窗错位、Orca 无角色/焦点 | 同环境对照；所选路线参考/目标组合失败则阻塞，外部未承诺组合记平台限制。 |
| 单例/激活错误 | 叠 surface、跨会话 toggle、重复启动、token 复用 | 会话身份单例键、串行去重、一次性 token 注入；结果未知不重试，故障测试阻断发布。 |
| 会话生命周期失控 | 多路径拉起、退出竞态、断连忙重试、僵尸 | 单一入口、有界退出状态机、断连即退出、外部有界退避和子进程回收。 |
| 服务名/tray 冲突 | 双通知、Watcher 抢占、Item 泄漏、菜单失败 | 不抢已有名字，panel 降级继续；重注册/消失测试，DBusMenu 未验收则关闭 tray。 |
| 锁屏隐私误判 | 无 LockedHint、状态过期、通知进程争 locker | 无可信 adapter 不承诺；状态不可信即遮蔽，绑定实测 locker/logind，发布醒目标注。 |
| 配置竞争或损坏 | 丢更新、旧版覆盖、rename 后失联、事件溢出 | 锁内重读 revision、逐版迁移、父目录监听、溢出重读、最后可用状态和单域重放。 |
| 通知/媒体资源耗尽 | 巨型图标、洪泛、远程跟踪、解码超时 | 数值上限、truncate/drop/reject、sender 隔离；可选 worker 未启用不纳入核心证据。 |
| 性能口径失真 | 常驻名单扩大、cold 混算、RSS 重复计数 | 固定 fixture/runner、PSS 为主、拆分启动路径、TGID 全线程原始数据；预算调整走 ADR。 |
| 供应链/运行时分裂 | 锁文件漂移、重复 Tokio/zbus、原生库漏审 | `--locked`、统一 feature/runtime，SBOM/NOTICE/漏洞审查覆盖 crate 与动态库。 |

## 12. 顺序行动 checklist（最多 10 项）

1. [ ] 固定 toolchain/锁文件与 Slint 许可文本，完成 GTK4 对照及供应链基线。
2. [ ] 完成 M0 纵切，以最终后端共同硬门槛和 IME/Orca/HiDPI 证据选型。
3. [ ] 固定 crate/进程、会话单例、部署入口与配置迁移/竞争契约。
4. [ ] 交付 M1 launcher、compositor binding、Desktop Entry/安全启动及 token 端到端测试。
5. [ ] 交付 M2 panel/通知/SNI，完成服务冲突、生命周期、锁屏和 action 激活测试；DBusMenu 未验收则关闭 tray。
6. [ ] 交付 M3 settings 与主题/i18n；可选 wallpaper/worker 未启用记 N/A 并关闭。
7. [ ] 填定 M1/M2 数值 resource-limits 表，验证 truncate/drop/reject 和 sender 所有权。
8. [ ] 在固定 fixture/cold runner 执行启动、PSS、600s wakeups、回落与依赖观测，公开原始结果。
9. [ ] 用统一 headless wrapper 和精确 Debian/Fedora VM 跑 CI/压力/真实重登录及人工 gate。
10. [ ] 完成部署清单、SBOM/NOTICE/原生库漏洞审查、无 secrets 检查和 M4 发布复核。

## 13. 参考规范与项目

- XDG：[Base Directory](https://specifications.freedesktop.org/basedir-spec/latest/)、[Desktop Entry](https://specifications.freedesktop.org/desktop-entry-spec/latest/)、[Menu](https://specifications.freedesktop.org/menu-spec/latest/)、[Icon Theme](https://specifications.freedesktop.org/icon-theme-spec/latest/)。
- Wayland：[官方文档](https://wayland.freedesktop.org/docs/html/)、[wayland-protocols](https://gitlab.freedesktop.org/wayland/wayland-protocols)、[wlr-protocols](https://gitlab.com/wlroots/wlr-protocols)、[SCTK](https://github.com/Smithay/client-toolkit)。
- UI/主题：[Slint](https://github.com/slint-ui/slint)、[Slint LICENSE（在线入口）](https://github.com/slint-ui/slint/blob/master/LICENSE.md)、[gtk-rs](https://gtk-rs.org/)、[gtk4-layer-shell](https://github.com/wmww/gtk4-layer-shell)、[Portal appearance](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Settings.html)。
- D-Bus/桌面服务：[zbus](https://dbus2.github.io/zbus/)、[SNI](https://www.freedesktop.org/wiki/Specifications/StatusNotifierItem/)、[DBusMenu](https://github.com/AyatanaIndicators/libdbusmenu)、[Notifications](https://specifications.freedesktop.org/notification-spec/latest/)、[UPower](https://upower.freedesktop.org/)。
- 会话/终端：[logind Session 与 LockedHint](https://www.freedesktop.org/software/systemd/man/latest/org.freedesktop.login1.Session.html)、[foot](https://codeberg.org/dnkl/foot)。
- 可访问性与输入：[AT-SPI](https://gitlab.gnome.org/GNOME/at-spi2-core)、[Orca](https://gitlab.gnome.org/GNOME/orca)、[IBus](https://github.com/ibus/ibus)、[Fcitx5](https://github.com/fcitx/fcitx5)。

以上链接用于核对规范与许可；实际锁版、能力检测和测试结果以各阶段 ADR 与发布报告为准。
