# Remote Camera

在 Windows 远程桌面里运行 Minecraft Java Edition 时，鼠标相对移动可能被 RDP 的绝对坐标和屏幕边缘限制打乱，结果就是视角乱飞、突然反向或转到边缘停住。

Remote Camera 是一个独立的 Windows companion tool：它在远程主机上观察前台的 Minecraft 窗口，过滤抖动和异常跳变，再把稳定的相对移动送回当前窗口。核心算法与 Windows 输入后端分离，方便测试、审计和后续增加其他桌面后端。

**它不是 Minecraft Mod。** 不需要 Fabric、Forge、NeoForge、LiteLoader 或任何其他加载器，也不读取 Java 内存。因此它不绑定 Minecraft 版本，原则上适用于仍使用 Java Edition 窗口输入的旧版和新版客户端。

## 兼容范围

Remote Camera 的兼容边界在 Windows 输入层，而不在 Minecraft 的 Java API：它不加载游戏类、不依赖 mappings，也不针对某个客户端版本编译。因此 **Minecraft Java Edition 1.0 至当前版本都使用同一套 `.exe`**，不需要按版本下载不同文件。

默认目标规则是前台窗口标题包含 `Minecraft`，进程名为 `java.exe` 或 `javaw.exe`。这是 Mojang/第三方启动器在旧版和新版 Java 客户端上共同保留的桌面边界。改过窗口标题或使用非标准 Java 进程名时，可以通过 `title_contains` 和 `process_names` 调整，而不需要改代码或重新编译。

这里的“全版本”指 **Java Edition 的桌面输入兼容性**；它不适用于 Bedrock Edition，也不能修复 RDP 编码、网络延迟、游戏帧率或锁屏/UAC 安全桌面本身的问题。

## 功能

- 对 RDP 会话自动启用，也可以在配置中允许本地桌面运行
- F8 快速开关，F9 立即退出
- One-Euro 自适应滤波：低速时稳，高速转向时保留响应
- deadzone 去除 RDP 微抖，max delta / max output 限制瞬时跳变
- 同时检查前台窗口标题和进程名（默认 `java.exe` / `javaw.exe`）
- 窗口切换、RDP 会话变化、配置变化都会重置状态机
- 配置文件热加载，出错字段单独忽略，不会破坏其他设置
- `--dry-run` 只观察和过滤，不注入、不吞鼠标事件
- `--verbose` 输出 target、RDP 和配置状态变化
- Windows GitHub Actions 生成 release `.exe` 与 SHA-256

## 使用

Windows x64 用户可以直接下载 [remote-camera.exe](https://github.com/HP-network/remote-camera/releases/download/v0.3.0/remote-camera.exe)，校验文件为 [remote-camera-windows-x64.sha256](https://github.com/HP-network/remote-camera/releases/download/v0.3.0/remote-camera-windows-x64.sha256)。

1. 把 `remote-camera.exe` 放在远程 Windows 主机上。
2. 在同一个远程桌面会话中启动它，再启动 Minecraft Java Edition。
3. 进入世界后按 F8 开启或停用稳定器。
4. 如果使用了非标准启动器标题，在配置中修改 `title_contains`；如果 Java 进程名不同，修改 `process_names`。

默认配置位置：

- Windows: `%APPDATA%\\RemoteCamera\\config.cfg`
- 其他系统（仅用于运行测试和查看 stub）: `~/.config/remote-camera/config.cfg`

示例配置：

```ini
enabled_on_start=true
require_rdp=true
recenter_cursor=true
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

`require_rdp=false` 会允许在普通本地桌面测试。`min_cutoff` 控制低速平滑程度，`beta` 控制高速运动时提高响应的幅度；先调整 `deadzone`，再微调 `sensitivity`。不要把 `max_delta` 和 `max_output` 设得过大，否则会重新放大远程桌面的跳变。

## 架构

```text
Windows WH_MOUSE_LL
        |
        v
target probe (foreground HWND + title + Java process)
        |
        v
CameraEngine (Disabled / WaitingForTarget / Tracking)
        |
        v
One-Euro filter -> bounded SendInput relative motion
```

核心 `CameraEngine` 和 `MotionFilter` 不依赖 Win32，可以在 macOS/Linux 上跑单元测试；只有 `platform/windows.rs` 负责低级钩子、RDP 探测、窗口坐标和 `SendInput`。这样后续增加 Raw Input 或其他远程桌面后端时，不需要重写算法。

工具只会处理同时满足以下条件的窗口：

1. 是当前前台窗口；
2. 标题包含 `title_contains`；
3. 进程名匹配 `process_names`；
4. 当前会话满足 `require_rdp`。

切换窗口、进程退出、RDP 状态变化或配置热加载都会清空滤波器历史，避免把上一窗口的速度带到下一窗口。

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

非 Windows 平台只提供编译 stub，不会安装鼠标钩子；实际功能必须在 Windows 远程主机上运行。低级鼠标钩子和 `SendInput` 只处理输入，不联网、不上传窗口内容、不修改 Minecraft 文件。注入事件带有专用 `dwExtraInfo` 标记，并会被钩子忽略，避免自激振荡。

## English

Remote Camera is a standalone Windows companion for the Minecraft Java Edition mouse problem that appears in RDP sessions. RDP can turn relative mouse input into absolute cursor jumps, edge locking, and sudden camera spins. The tool watches the foreground Minecraft window, removes small jitter and implausible jumps, recenters the cursor, and injects a bounded relative movement.

This is **not a Minecraft mod**. It does not require Fabric, Forge, NeoForge, LiteLoader, or a particular game version. It works outside the JVM and targets the Windows desktop input path, so the same executable can be used across Java Edition versions as long as the foreground window title contains `Minecraft`.

Download the Windows x64 executable from the [v0.3.0 release](https://github.com/HP-network/remote-camera/releases/tag/v0.3.0) and verify it with the published SHA-256 file.

Press F8 to toggle the filter and F9 to exit. The default configuration is `%APPDATA%\\RemoteCamera\\config.cfg`; set `require_rdp=false` to test on a local desktop. `--dry-run`, `--verbose`, and `--print-config` are available for diagnosis. Build with `cargo build --release --locked` on Windows. The non-Windows build is a harmless stub for tests and documentation only.

## 限制

- 仅支持 Windows 交互式桌面；Linux/macOS 不提供同等输入 API。
- 需要与 Minecraft 位于同一个远程会话；服务会话、锁屏和 UAC 安全桌面不会处理。
- 窗口识别依赖前台标题中的 `Minecraft`，这是为了避免误伤其他应用。
- 不支持 Bedrock Edition；Java 客户端被重命名时需要调整目标规则。
- 这是输入兼容工具，不是对 RDP 编码、网络延迟或游戏帧率的修复。
