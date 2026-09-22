# Remote Camera

在 Windows 被控端运行 Minecraft 或其他 3D 程序时，RDP、AnyDesk、TeamViewer、RustDesk、ToDesk 等远程控制软件的绝对坐标和屏幕边缘限制可能把相对鼠标移动打乱，结果就是视角乱飞、突然反向，或光标转到边缘停住。

Remote Camera 是一个独立的 Windows companion tool：默认覆盖被控端的整个交互式桌面，过滤抖动和异常跳变，再把稳定的相对移动送回桌面；也可以切换为只处理 Minecraft 前台窗口。它不依赖具体远控软件或 RDP 会话标志。

**它不是 Minecraft Mod。** 不需要 Fabric、Forge、NeoForge、LiteLoader 或任何其他加载器，也不读取 Java 内存。因此它不绑定 Minecraft 版本，原则上适用于仍使用 Java Edition 窗口输入的旧版和新版客户端。

## 兼容范围

Remote Camera 的兼容边界在 Windows 输入层，而不在 Minecraft 的 Java API：它不加载游戏类、不依赖 mappings，也不针对某个客户端版本编译。因此 **Minecraft Java Edition 1.0 至当前版本都使用同一套 `.exe`**；桌面模式也不依赖 Minecraft 版本。

默认目标规则是前台窗口标题包含 `Minecraft`，进程名为 `java.exe` 或 `javaw.exe`。这是 Mojang/第三方启动器在旧版和新版 Java 客户端上共同保留的桌面边界。改过窗口标题或使用非标准 Java 进程名时，可以通过 `title_contains` 和 `process_names` 调整，而不需要改代码或重新编译。

这里的“全版本”指 **Java Edition 的桌面输入兼容性**；它不适用于 Bedrock Edition，也不能修复远控编码、网络延迟、游戏帧率或锁屏/UAC 安全桌面本身的问题。

## 功能

- 对任意交互式 Windows 桌面启用，兼容 RDP 和第三方远控软件
- `target_scope=desktop` 默认处理整个桌面；`target_scope=minecraft` 可限制到 Minecraft 前台窗口
- F8 快速开关，F9 立即退出
- One-Euro 自适应滤波：低速时稳，高速转向时保留响应
- deadzone 去除远控微抖，max delta / max output 限制瞬时跳变
- Minecraft 模式检查前台窗口标题和进程名（默认 `java.exe` / `javaw.exe`）
- 窗口切换、会话变化、配置变化都会重置状态机
- 配置文件热加载，出错字段单独忽略，不会破坏其他设置
- `--dry-run` 只观察和过滤，不注入、不吞鼠标事件
- `--verbose` 输出 target、会话和配置状态变化
- Windows GitHub Actions 生成 release `.exe` 与 SHA-256

## 使用

Windows x64 用户可以直接下载 [remote-camera.exe](https://github.com/HP-network/remote-camera/releases/download/v0.6.1/remote-camera.exe)，校验文件为 [remote-camera-windows-x64.sha256](https://github.com/HP-network/remote-camera/releases/download/v0.6.1/remote-camera-windows-x64.sha256)。

拓扑必须是：

```text
控制端（发起远程控制连接的电脑）
        │
        └── RDP / AnyDesk / TeamViewer / RustDesk / ToDesk ──> 被控端（运行 Windows 和 Minecraft 的电脑）
                          └── remote-camera.exe 在这里运行
```

1. 把 `remote-camera.exe` 放在被控端，而不是控制端。
2. 在被控端的同一个交互式用户会话中启动它，再启动 Minecraft Java Edition。
3. 进入世界后按 F8 开启或停用稳定器。
4. 如果使用了非标准启动器标题，在配置中修改 `title_contains`；如果 Java 进程名不同，修改 `process_names`。

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
process_names=javaw.exe,java.exe
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
target probe (desktop center or foreground HWND + title + Java process)
        |
        v
CameraEngine (Disabled / WaitingForTarget / Tracking)
        |
        v
One-Euro filter -> bounded SendInput relative motion
```

核心 `CameraEngine` 和 `MotionFilter` 不依赖 Win32，可以在 macOS/Linux 上跑单元测试；只有 `platform/windows.rs` 负责低级钩子、会话探测、窗口坐标和 `SendInput`。这样后续增加 Raw Input 或其他远程桌面后端时，不需要重写算法。

桌面模式会处理被控端当前交互式桌面；Minecraft 模式只会处理同时满足以下条件的窗口：

1. 是当前前台窗口；
2. 标题包含 `title_contains`；
3. 进程名匹配 `process_names`；
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

This is **not a Minecraft mod**. It does not require Fabric, Forge, NeoForge, LiteLoader, or a particular game version. It works outside the JVM and targets the Windows desktop input path, so the same executable can be used across Java Edition versions as long as the foreground window title contains `Minecraft`.

Download the Windows x64 executable from the [v0.6.1 release](https://github.com/HP-network/remote-camera/releases/tag/v0.6.1) and verify it with the published SHA-256 file.

Run `remote-camera.exe` on the **controlled Windows host where Minecraft runs**, inside the same interactive user session. Running it on the controlling/client computer cannot intercept input delivered to the controlled host.

Press F8 to toggle the filter and F9 to exit. The default configuration is `%APPDATA%\\RemoteCamera\\config.cfg`; `session_mode=any` supports RDP and third-party remote-control clients, while `session_mode=rdp` is strict. The default `target_scope=desktop` covers the interactive desktop; use `target_scope=minecraft` for a game-only filter. `--dry-run`, `--verbose`, and `--print-config` are available for diagnosis. Build with `cargo build --release --locked` on Windows. The non-Windows build is a harmless stub for tests and documentation only.

## 限制

- 仅支持 Windows 交互式桌面；Linux/macOS 不提供同等输入 API。
- 必须在运行 Minecraft 的被控端交互式会话中运行；在控制端运行不会拦截被控端的输入。服务会话、锁屏和 UAC 安全桌面不会处理。
- Minecraft 模式依赖前台标题中的 `Minecraft`，这是为了避免误伤其他应用；桌面模式会有意覆盖整个当前远控桌面。
- Minecraft 模式不支持 Bedrock Edition；Java 客户端被重命名时需要调整目标规则。桌面模式不检查 Minecraft 进程。
- 这是输入兼容工具，不是对远控编码、网络延迟或游戏帧率的修复。
