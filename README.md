# Remote Camera

在 Windows 被控端运行 Minecraft 或其他 3D 程序时，RDP、AnyDesk、TeamViewer、RustDesk、ToDesk 等远程控制软件的绝对坐标和屏幕边缘限制可能把相对鼠标移动打乱，结果就是视角乱飞、突然反向，或光标转到边缘停住。

Remote Camera 是一个独立的 Windows companion tool：默认覆盖被控端的整个交互式桌面，过滤抖动和异常跳变，再把稳定的相对移动送回桌面；也可以切换为只处理 Minecraft 前台窗口。它不依赖具体远控软件或 RDP 会话标志。

**它不是 Minecraft Mod。** 不需要 Fabric、Forge、NeoForge、LiteLoader 或任何其他加载器，也不读取游戏内存。因此它不绑定 Minecraft 版本，原则上适用于 Java Edition 和 Windows Bedrock 客户端。

## 兼容范围

Remote Camera 的兼容边界在 Windows 输入层，而不在 Minecraft 的游戏 API：它不加载游戏类、不依赖 mappings，也不针对某个客户端版本编译。因此 **Minecraft Java Edition 1.0 至当前版本，以及 Windows Bedrock 的当前和历史客户端，都使用同一套 `.exe`**；桌面模式也不依赖 Minecraft 版本。

默认目标规则按前台可见窗口所属进程匹配 Java 或 Windows Bedrock 的常见进程名，不依赖窗口标题、不依赖中文/英文名称，也不依赖 Minecraft 版本。旧配置里的 `title_contains=minecraft` 作为兼容默认值时不会强制检查标题；只有填写其他文本时才启用标题限制。使用非标准客户端进程名时，修改 `process_names`，不需要重新编译。

这里的“全版本”指 **Windows 桌面输入兼容性**；Linux/macOS 客户端和锁屏/UAC 安全桌面不在支持范围内，也不能修复远控编码、网络延迟或游戏帧率本身的问题。

## 功能

- 对任意交互式 Windows 桌面启用，兼容 RDP 和第三方远控软件
- `target_scope=desktop` 默认处理整个桌面；`target_scope=minecraft` 可限制到 Minecraft 前台窗口
- F8 快速开关，F9 立即退出
- One-Euro 自适应滤波：低速时稳，高速转向时保留响应
- deadzone 去除远控微抖，max delta / max output 限制瞬时跳变
- Minecraft 模式检查前台或可见顶层 Java/Bedrock 窗口；可选地检查自定义窗口标题
- 窗口切换、会话变化、配置变化都会重置状态机
- 配置文件热加载，出错字段单独忽略，不会破坏其他设置
- `--dry-run` 只观察和过滤，不注入、不吞鼠标事件
- `--verbose` 输出 target、会话和配置状态变化
- Windows GitHub Actions 生成 release `.exe` 与 SHA-256

## 使用

Windows x64 用户可以直接下载 [remote-camera.exe](https://github.com/HP-network/remote-camera/releases/download/v0.6.5/remote-camera.exe)，校验文件为 [remote-camera-windows-x64.sha256](https://github.com/HP-network/remote-camera/releases/download/v0.6.5/remote-camera-windows-x64.sha256)。

拓扑必须是：

```text
控制端（发起远程控制连接的电脑）
        │
        └── RDP / AnyDesk / TeamViewer / RustDesk / ToDesk ──> 被控端（运行 Windows 和 Minecraft 的电脑）
                          └── remote-camera.exe 在这里运行
```

1. 把 `remote-camera.exe` 放在被控端，而不是控制端。
2. 在被控端的同一个交互式用户会话中启动它，再启动 Minecraft。
3. 进入世界后按 F8 开启或停用稳定器。
4. 如果需要严格限制窗口，在配置中修改 `title_contains`；如果 Java 进程名不同，修改 `process_names`。

默认配置位置：

- Windows: `%APPDATA%\\RemoteCamera\\config.cfg`
- 其他系统（仅用于运行测试和查看 stub）: `~/.config/remote-camera/config.cfg`

示例配置：

```ini
enabled_on_start=true
session_mode=any
recenter_cursor=true
target_scope=desktop
title_contains=minecraft
process_names=javaw.exe,java.exe,Minecraft.Windows.exe,Minecraft.exe
min_cutoff=1.2
beta=0.015
derivative_cutoff=1.0
deadzone=0.15
max_delta=300
max_output=40
sensitivity=1.0
poll_interval_ms=8
recenter_settle_ms=3
config_reload_secs=5
```

`session_mode=any`（默认）兼容 RDP 和第三方远控软件；`session_mode=rdp` 才会强制要求 Windows RDP 会话。`require_rdp` 仍接受旧配置，但没有 `session_mode` 的旧文件会迁移为 `any`；新配置请使用 `session_mode`。`min_cutoff` 控制低速平滑程度，`beta` 控制高速运动时提高响应的幅度；先调整 `deadzone`，再微调 `sensitivity`。不要把 `max_delta` 和 `max_output` 设得过大，否则会重新放大远程桌面的跳变。

`target_scope=desktop` 使用当前 Windows 虚拟桌面的中心点，不检查窗口标题和 Java 进程。它是默认模式，会影响被控端当前会话里的其他桌面应用，按 F8 可立即停用。需要只处理 Minecraft 时改为 `target_scope=minecraft`。

## 架构

```text
Windows WH_MOUSE_LL
        |
        v
target probe (desktop center or foreground HWND + title + game process)
        |
        v
CameraEngine (Disabled / WaitingForTarget / Tracking)
        |
        v
One-Euro filter -> bounded SendInput relative motion
```

核心 `CameraEngine` 和 `MotionFilter` 不依赖 Win32，可以在 macOS/Linux 上跑单元测试；只有 `platform/windows.rs` 负责低级钩子、会话探测、窗口坐标和 `SendInput`。这样后续增加 Raw Input 或其他远程桌面后端时，不需要重写算法。

桌面模式会处理被控端当前交互式桌面；Minecraft 模式只会处理同时满足以下条件的窗口：

1. 是当前前台窗口，或远控软件短暂报告辅助窗口时可从可见顶层窗口回退探测；
2. 进程名匹配 `process_names`（默认包含 Java 和 Windows Bedrock 客户端）；
3. `title_contains` 不是默认值 `minecraft` 时，标题才必须匹配它；
4. `session_mode=rdp` 时当前会话必须是 RDP。

切换窗口、进程退出、会话状态变化或配置热加载都会清空滤波器历史，避免把上一窗口的速度带到下一窗口。

## 构建

本地检查 motion filter：

```powershell
cargo test --locked
```

观察模式（不会注入或吞掉鼠标事件）：

```powershell
remote-camera.exe --dry-run --verbose
```

查看实际配置：

```powershell
remote-camera.exe --print-config
```

构建 Windows x64：

```powershell
cargo build --release --locked
```

生成文件：`target\\release\\remote-camera.exe`。

非 Windows 平台只提供编译 stub，不会安装鼠标钩子；实际功能必须在 Windows 被控端运行。低级鼠标钩子和 `SendInput` 只处理输入，不联网、不上传窗口内容、不修改 Minecraft 文件。工具只忽略自己带有专用 `dwExtraInfo` 标记的注入事件，其他远控软件生成的输入仍会被处理。

## English

Remote Camera is a standalone Windows companion for unstable mouse input on a controlled Windows desktop. RDP and third-party remote-control clients can turn relative mouse input into absolute cursor jumps, edge locking, and sudden camera spins. By default the tool applies a bounded filter to the whole interactive desktop. Set `target_scope=minecraft` to limit handling to the foreground Minecraft window.

This is **not a Minecraft mod**. It does not require Fabric, Forge, NeoForge, LiteLoader, or a particular game version. It works outside the game runtime and targets the Windows desktop input path, so the same executable can be used across Java Edition and Windows Bedrock versions. The default process filter also works with localized and custom window titles.

Download the Windows x64 executable from the [v0.6.5 release](https://github.com/HP-network/remote-camera/releases/tag/v0.6.5) and verify it with the published SHA-256 file.

Run `remote-camera.exe` on the **controlled Windows host where Minecraft runs**, inside the same interactive user session. Running it on the controlling/client computer cannot intercept input delivered to the controlled host.

Press F8 to toggle the filter and F9 to exit. The default configuration is `%APPDATA%\\RemoteCamera\\config.cfg`; `session_mode=any` supports RDP and third-party remote-control clients, while `session_mode=rdp` is strict. The default `target_scope=desktop` covers the interactive desktop; use `target_scope=minecraft` for a game-only filter. `--dry-run`, `--verbose`, and `--print-config` are available for diagnosis. Build with `cargo build --release --locked` on Windows. The non-Windows build is a harmless stub for tests and documentation only.

## 限制

- 仅支持 Windows 交互式桌面；Linux/macOS 不提供同等输入 API。
- 必须在运行 Minecraft 的被控端交互式会话中运行；在控制端运行不会拦截被控端的输入。服务会话、锁屏和 UAC 安全桌面不会处理。
- Minecraft 模式默认不依赖前台标题，只依赖进程规则；桌面模式会有意覆盖整个当前远控桌面。
- Minecraft 模式默认包含 Java Edition 和 Windows Bedrock 的常见进程名；客户端被重命名时需要调整 `process_names`。桌面模式不检查 Minecraft 进程。
- 这是输入兼容工具，不是对远控编码、网络延迟或游戏帧率的修复。
