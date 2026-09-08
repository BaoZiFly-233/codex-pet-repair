# 致谢与许可

本项目代码采用 [MIT 许可](LICENSE)。第三方组件与素材遵循各自的许可。

## 参考来源

| 来源 | 参考内容 |
| --- | --- |
| [Codex #41513](https://github.com/openai/codex/issues/41513) | 故障排查线索 |
| [ChatGPT-Overlay-Fix](https://github.com/FoegiUpdate/ChatGPT-Overlay-Fix) | 浮窗交互恢复方法 |
| [Codex Tweaks](https://github.com/codex-tweaks/codex-tweaks) | 功能组织参考 |
| [Codex #33224](https://github.com/openai/codex/issues/33224#issuecomment-5563439305) | V2 宠物缺少真实鼠标视线输入的线索，以及使用已有鼠标事件补充视线输入的思路 |
| [Codex++](https://github.com/BigPizzaV3/CodexPlusPlus)（AGPL-3.0） | 通过本机 CDP 连接原生视线事件的架构参考；本项目独立实现连接、识别与生命周期管理，未复制其源码 |
| [Slint Gallery](https://github.com/slint-ui/slint/tree/master/examples/gallery/ui) | 界面组件示例 |
| [Microsoft Fluent](https://fluent2.microsoft.design/components/windows) | 界面设计参考 |

## 组件与许可

| 组件 | 用途 |
| --- | --- |
| [Rust](https://www.rust-lang.org/) | 编写程序 |
| [Slint 1.17.1](https://github.com/slint-ui/slint) | 界面与控件 |
| [winit](https://github.com/rust-windowing/winit) | 窗口与输入 |
| [softbuffer](https://github.com/rust-windowing/softbuffer) | 窗口绘制 |
| [Serde / serde_json](https://serde.rs/) | 设置与状态读写 |
| [windows-rs / windows-sys](https://github.com/microsoft/windows-rs) | Windows 功能调用 |
| [tungstenite](https://github.com/snapview/tungstenite-rs)（MIT / Apache-2.0） | 仅本机的 WebSocket 通信，不启用 TLS 或异步运行时 |
| [png](https://github.com/image-rs/image-png) | 开发 feature 的截图导出；图像依赖链也使用 PNG 解码 |

Slint 使用 Royalty-free Desktop, Mobile, and Web Applications License 2.0，窗口呈现、主题与菜单包含本项目的适配修改。

界面采用 Slint 提供的 Fluent 风格标准 Switch、Button、布局与菜单组件，不是微软官方 WinUI。正式构建默认关闭开发预览和截图入口；实际分发依赖的许可由打包脚本按默认 feature 重新生成。

依赖版本见 [核心](Cargo.lock)与[界面](experiments/native-ui/Cargo.lock)的锁定文件。许可原文见 [licenses](licenses) 和下载包中的 `THIRD-PARTY-NOTICES.txt`。

## 图标

图标参考 Codex 默认宠物并加入锤子元素，源文件为 [SVG](assets/pet-repair-pixel.svg)。原宠物形象的权利归相应权利人所有，代码许可不包含该形象的授权。
