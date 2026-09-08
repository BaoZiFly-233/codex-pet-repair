# 开发说明

普通用户请使用 README 中的下载包。本页说明当前 Slint 版本的构建与验证。

## 代码结构

- `src`：独立 Windows 托盘核心，负责严格识别、调度、修复事务、设置和命名管道；不再包含手绘主界面。
- `experiments/native-ui`：正式使用的 Slint 界面，目录名沿用早期评估。采用 Slint 的 Fluent 风格标准组件，不是微软 WinUI。
- `shared`：两端共用的轻量 UI 状态与 Win32 通知事件。完整诊断仅在 `status`、日志或复制诊断等显式请求时生成。
- `src/gaze.rs`、`src/gaze_transport.rs`、`assets/gaze-bridge.js`：独立鼠标跟随开关、严格本机连接与原生视线事件桥接。
- `assets/pet-repair-pixel.svg`：确定的图标源文件；仓库包含导出的 ICO，运行时不需要 Python。

## 构建

安装 Rust 1.94 或更新版本、Visual Studio C++ 桌面开发工具及 Windows SDK。在 x64 Developer PowerShell 中运行：

以下命令在源码目录执行；下载包只包含程序和必要的许可文件，使用说明、更新记录、致谢与开发文档保留在源码仓库。

```powershell
$env:RUSTFLAGS = '-C target-feature=+crt-static'
cargo test --locked
node --experimental-vm-modules --test tests/gaze-bridge.test.cjs
cargo clippy --locked --all-targets -- -D warnings
cargo build --release --locked
cargo test --release --locked --all-features --manifest-path experiments/native-ui/Cargo.toml
cargo clippy --release --locked --all-targets --all-features --manifest-path experiments/native-ui/Cargo.toml -- -D warnings
cargo build --release --locked --manifest-path experiments/native-ui/Cargo.toml
.\package.ps1
```

资源编译器默认是 Windows SDK 的 `rc.exe`，也可通过 `RC` 指定 `llvm-rc.exe`。根目录 `package.ps1` 转发到当前原生界面打包脚本。

打包脚本默认生成 `dist/PetRepair-v1.1.0-windows-x64.zip`；指定 `-OutputDirectory` 可另选一个不存在的目录。使用自定义 Cargo 缓存时传入 `-CoreBinary` 和 `-UiBinary`。脚本拒绝覆盖已有目录，从锁定依赖生成许可清单，不分发用户 `data`、PDB 或构建缓存。

解压结构为 `PetRepair.exe` 与 `ui/PetRepair.UI.exe`，保留整个目录。关闭界面退出 UI 进程，托盘核心继续运行。

## 开发预览

默认构建不启用截图和预览入口。需要时单独构建：

```powershell
cargo build --release --locked --features dev-tools --manifest-path experiments/native-ui/Cargo.toml
.\experiments\native-ui\target\release\pet-repair-native-ui.exe --preview --dark --size 320x480 --capture D:\preview.png
```

另支持 `--menu`、`--license`、`--confirm`、`--disabled`、`--busy`、`--toggle-theme`。`--startup` 模拟启动后异步追加状态内容。预览不连接后台、不保存用户设置。`png` 的直接依赖仅在此 feature 启用；Slint 图像解码链仍可间接依赖 PNG，不能据此删除依赖文件。打包前重新构建默认 feature，勿把开发预览二进制分发。

执行 `experiments/native-ui/check-layout.ps1 -Executable <开发预览程序绝对路径>` 可检查 100%、150%、200% 缩放下的默认、深色、修复中、异步状态，以及手动窄窗口和宽窗口，共 14 个场景。PNG 旁的 JSON 记录窗口尺寸和滚动溢出量。自动适配使用一次性布局回调与物理尺寸换算；用户手动调整后保留其尺寸。

## 运行与兼容

启动时完整发现；已知进程的窗口事件以首个事件后 300 毫秒为固定截止时间合并，只核验涉及的 HWND。没有已知进程时每 10 秒兜底，已有进程时独立按 30 秒校准。身份缓存持有存活句柄，策略以 PID、启动时间、HWND 为键；修改和恢复仍完整核验身份，事务循环复用句柄与标记。

`--control` 保留 `status` 及原有控制命令，新增 `ui_status`。UI 等待同会话命名事件或用户命令，以 10 秒为兜底同步；等待倒计时由后台真实剩余期限在界面本地按秒递减。状态修订与请求代次分别处理后台变化和旧响应，操作失败回写实际设置，连接失败单独显示。命名事件只通知，不承载数据；状态仍经原有身份验证管道获取。

设置文件、确认过的版本列表和待恢复事务格式保持兼容。旧 `--legacy-ui` 和核心 `--ui-preview` 入口已删除。开机启动、自动修复与主题仍由用户分别控制。

看向鼠标通过 Windows 应用激活接口启动官方 MSIX，监听地址限于 `127.0.0.1:19227`。连接前核对监听 PID、包身份、会话、进程创建时间、严格窗口样式和唯一宠物页面；连接中持有进程句柄，检查窗口身份，并每 3 秒复查监听及窗口集合。HTTP 禁止代理、重定向和自动身份认证；WebSocket 使用有超时、大小上限的同步 socket。

桥接脚本只查找已加载的 `app-initial-*` / `vscode-api-*` 模块，通过唯一的消息分发与订阅接口连接，不按压缩变量名放宽窗口识别。精确宠物标记与 V2 图像尺寸同时通过后，才订阅并发送 `avatar-overlay-computer-use-cursor-changed`。输入来自浮窗收到的可信 `mousemove`，直接使用 CSS 客户区坐标；通过动画帧和 33 ms 最小间隔合并移动，不启动全局光标采样。原生电脑操作光标优先，移动停止 1.4 秒后释放视线，静止刷新事件不续期。窗口尺寸不一致时暂停。

后台每秒检查连接及窗口几何；原生窗口移动、页面 `resize`、修复暂停都会取消旧坐标和排队帧，恢复后等待下一次真实移动。监听器安装在页面窗口上，尺寸变化不重复注册；页面重载通过原有重连路径重新安装。自动修复与跟随均关闭后后台阻塞等待命令；桥接心跳中断超过 3 秒会自行清理。

`Policy` 统一保存进程实例、HWND、几何、处理记录和输入证据。尺寸或位置变化只使证据失效，不清除已处理标记。后台从 Win32 读取真实鼠标位置，页面只返回当前 V2 精灵实体像素是否属于宠物交互区域；仅支持已验证的 8 列 11 行背景图布局，透明像素、未知布局和读取失败均不作为故障证据。前后几何和鼠标位置必须一致，且排除按键及上层窗口遮挡，才比较 Windows 实际命中的根窗口。连续至少三次、跨度至少 2 秒的不匹配才允许自动重试；未知状态打断连续证据，证据超过 2.5 秒失效。窗口稳定 2 秒、自动间隔 30 秒、五分钟最多三次、版本确认和用户暂停规则继续生效。没有诊断连接时保留首次修复流程，不因缩放自动重试。

修复前通过带序号的暂停确认等待桥接清空视线，旧确认不能授权新修复；连接丢失后等待自清理宽限。准备阶段不阻塞 UI，8 秒未确认则取消并暂停自动尝试。实际窗口事务恢复后再解除跟随暂停；修复期间忽略健康样本，恢复后重新取证。关闭跟随但保留自动修复时，桥接只读检查输入。

日志使用 `INPUT_HEALTHY`、`INPUT_FAILURE_CONFIRMED`、`INPUT_PAUSE_TIMEOUT`、`GAZE_VIEWPORT_RESET`，保留原有 `SETTING_GAZE` 和 `GAZE_STATE`。诊断摘要记录命中检查结果和排除条件，不记录鼠标轨迹或页面正文。

## 验证

`PetRepair.exe --self-test <结果文件绝对路径>` 使用自建窗口验证严格分类、正常恢复、取消、并发样式变化、辅助进程崩溃、窗口销毁、父进程退出及待恢复事务，不操作真实宠物。

`measure-footprint.ps1 -Directory <解压目录> -Report <JSON 路径> -Samples 31 -RequireUI` 以 2 秒间隔测量 60 秒，统计目录下全部应用进程的工作集、私有内存、CPU 和句柄；关闭 UI 后不传 `-RequireUI` 重测。分别报告解压字节数和 ZIP 大小。完整 `status` 的 `metrics` 提供进程扫描、窗口枚举、定向窗口检查及 UI 查询累计次数。

使用 Computer Use 检查实际 Windows 呈现、主题、焦点、菜单、许可、窄窗口与 DPI，并覆盖遮挡恢复、尺寸变化和状态更新。应用离屏截图只用于布局排查，不能替代真实呈现验收。当前一轮的实测结果和工具限制见 `REFACTOR-VALIDATION.md`。

Slint 及补丁继续锁定在 1.17.1，详见 `experiments/native-ui/vendor/PATCHES.md`。主窗口不透明，Windows 软件渲染继续按需提交整帧，不添加空闲刷新循环；升级依赖时逐项复查补丁。窗口样式恢复成功不等于已经确认实际拖动有效。
