# 开发说明

普通用户请使用 README 中的下载包。本页说明当前 Slint 版本的构建与验证。

## 代码结构

- `src`：独立 Windows 托盘核心，负责严格识别、调度、修复事务、设置和命名管道；不再包含手绘主界面。
- `experiments/native-ui`：正式使用的 Slint 界面，目录名沿用早期评估。采用 Slint 的 Fluent 风格标准组件，不是微软 WinUI。
- `shared`：两端共用的轻量 UI 状态与 Win32 通知事件。完整诊断仅在 `status`、日志或复制诊断等显式请求时生成。
- `assets/pet-repair-pixel.svg`：确定的图标源文件；仓库包含导出的 ICO，运行时不需要 Python。

## 构建

安装 Rust 1.94 或更新版本、Visual Studio C++ 桌面开发工具及 Windows SDK。在 x64 Developer PowerShell 中运行：

以下命令在源码目录执行；下载包中的开发文档不包含完整源码。

```powershell
$env:RUSTFLAGS = '-C target-feature=+crt-static'
cargo test --locked
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

## 验证

`PetRepair.exe --self-test <结果文件绝对路径>` 使用自建窗口验证严格分类、正常恢复、取消、并发样式变化、辅助进程崩溃、窗口销毁、父进程退出及待恢复事务，不操作真实宠物。

`measure-footprint.ps1 -Directory <解压目录> -Report <JSON 路径> -Samples 31 -RequireUI` 以 2 秒间隔测量 60 秒，统计目录下全部应用进程的工作集、私有内存、CPU 和句柄；关闭 UI 后不传 `-RequireUI` 重测。分别报告解压字节数和 ZIP 大小。完整 `status` 的 `metrics` 提供进程扫描、窗口枚举、定向窗口检查及 UI 查询累计次数。

使用 Computer Use 检查实际 Windows 呈现、主题、焦点、菜单、许可、窄窗口与 DPI，并覆盖遮挡恢复、尺寸变化和状态更新。应用离屏截图只用于布局排查，不能替代真实呈现验收。当前一轮的实测结果和工具限制见 `REFACTOR-VALIDATION.md`。

Slint 及补丁继续锁定在 1.17.1，详见 `experiments/native-ui/vendor/PATCHES.md`。主窗口不透明，Windows 软件渲染继续按需提交整帧，不添加空闲刷新循环；升级依赖时逐项复查补丁。窗口样式恢复成功不等于已经确认实际拖动有效。
