# 致谢与许可说明

感谢愿意公开排查过程、分享工具和维护组件的人。

## 修复思路与设计参考

| 来源 | 我们参考了什么 |
| --- | --- |
| [openai/codex #41513](https://github.com/openai/codex/issues/41513) | 宠物无法点击和拖动的现象、排查记录与线索 |
| [FoegiUpdate / ChatGPT-Overlay-Fix](https://github.com/FoegiUpdate/ChatGPT-Overlay-Fix) | 临时恢复浮窗交互的方法；本项目据此编写了独立的修复、取消和恢复流程 |
| [Codex Tweaks](https://github.com/codex-tweaks/codex-tweaks) | 早期功能包方向和操作组织的参考；不是本程序运行所需的组件 |
| [Slint Gallery](https://github.com/slint-ui/slint/tree/master/examples/gallery/ui) | 标准界面组件的使用示例 |
| [Microsoft Fluent](https://fluent2.microsoft.design/components/windows) | 颜色、按钮、开关和菜单的设计参考；本项目不使用微软 WinUI 作为正式版界面 |

这些项目与本工具相互独立。列出它们不表示作者为本工具提供了授权背书或效果保证。

## 使用的组件

| 组件 | 用途 | 许可说明 |
| --- | --- | --- |
| [Rust](https://www.rust-lang.org/) | 编写程序 | 相关随包说明见 licenses/rust-runtime |
| [Slint 1.17.1](https://github.com/slint-ui/slint) | 按钮、开关、菜单与界面显示 | 使用 Royalty-free Desktop, Mobile, and Web Applications License 2.0；应用内保留 AboutSlint 入口 |
| [winit](https://github.com/rust-windowing/winit) | Windows 窗口与输入 | 随包 THIRD-PARTY-NOTICES.txt |
| [softbuffer](https://github.com/rust-windowing/softbuffer) | 把界面显示到窗口 | 随包 THIRD-PARTY-NOTICES.txt |
| [Serde / serde_json](https://serde.rs/) | 读取、保存设置和状态 | MIT / Apache-2.0，详见 licenses |
| [windows-rs / windows-sys](https://github.com/microsoft/windows-rs) | 使用 Windows 提供的功能 | MIT / Apache-2.0，详见 licenses |
| [png](https://github.com/image-rs/image-png) | 开发时导出界面预览 | 随包 THIRD-PARTY-NOTICES.txt |

完整依赖以两个 Cargo.lock 文件为准。下载包里的 THIRD-PARTY-NOTICES.txt 汇总界面运行所需组件的许可，licenses 保留核心部分的许可。它们不是本项目 MIT 许可的替代品。

Slint 的窗口显示、菜单配色和字号做了少量调整。保留的第三方源文件仍遵循原有许可。

## 图标

图标以默认 Codex 宠物为参考，加入小锤子元素，以 [方格 SVG](assets/pet-repair-pixel.svg) 保存。Windows 所需的 ICO 从此 SVG 导出。

宠物形象的原始权益归原权利人所有；本项目不把它宣称为自己原创的角色或 OpenAI 官方标识，也不通过代码的 MIT 许可授予第三方形象权利。

## 本项目的许可

本项目自行编写的代码使用 [MIT 许可](LICENSE)。
