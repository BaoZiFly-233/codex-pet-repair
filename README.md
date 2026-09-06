# Codex 宠物修复

<img src="assets/pet-repair-pixel.svg" width="112" height="112" alt="蓝色宠物和左上角的小锤子">

宠物还在桌面上，却怎么也拖不动？这个小工具可以帮你试着恢复它。

这是一个给 Windows 用户准备的非官方小工具。打开后点一下「立即修复」，等几秒，再试着拖动宠物。也可以让它留在右下角的托盘里，需要时再打开。

**[下载正式版](https://github.com/BaoZiFly-233/codex-pet-repair/releases/latest) · [遇到问题？告诉我们](https://github.com/BaoZiFly-233/codex-pet-repair/issues)**

## 开始使用

1. 在下载页面找到 `PetRepair-v1.0.0-windows-x64.zip`。想直接使用的话，不用下载名为 Source code 的文件。
2. 把压缩包完整解压到一个方便找到的文件夹，例如 D 盘。不要直接在压缩包里运行，也不要只取出其中一个文件。
3. 打开文件夹里的 `PetRepair.exe`，然后打开 Codex，让宠物出现在桌面上。
4. 点击「立即修复」，等待约 3 秒。期间可以正常点击其他地方或打字，先不要拖动宠物。
5. 如果出现效果确认，请亲手拖一下宠物，再选择「可以拖动」或「仍然不行」。

目前提供 Windows 64 位版本。不需要另外安装 .NET、浏览器或 Notepad4。完整解压约 11 MB；设置和日志会在使用后另外产生。

## 按照你的习惯来

| 功能 | 可以帮你做什么 |
| --- | --- |
| 立即修复 | 宠物拖不动时，手动试一次；修复中也可以取消 |
| 自动修复 | 确认当前 Codex 版本确实有效后，宠物重新出现时自动尝试，不用每次打开界面 |
| 仅托盘启动 | 下次打开工具时，直接留在桌面右下角，不弹出主窗口 |
| 开机启动 | 登录 Windows 后自动在托盘里运行 |
| 深色 | 一个开关切换浅色和深色，下次打开会记住你的选择 |
| 打开日志 | 查看发生了什么；旁边的小箭头还能复制诊断摘要或打开日志文件夹 |

自动修复和开机启动默认关闭，由你决定是否开启。确认修复有效，不会替你改变自动修复开关。Codex 更新后，需要重新确认一次效果。

点击「托盘」或右上角的关闭按钮，只会收起界面，已经开启的自动修复会继续工作。双击托盘图标可以重新打开。想完全关闭，请点击「退出程序」，或使用托盘菜单里的「退出」。

## 有几件事先说明白

- 这不是 OpenAI 官方产品，也没有获得 OpenAI 的授权或背书。
- 它提供的是临时恢复办法，不会修改 Codex 的安装文件。Codex 更新后，效果可能会变化。
- 「窗口状态已重置」表示工具完成了这次操作，是否真正恢复拖动，仍要以你实际试用的结果为准。
- 请尽量只保留一个宠物浮窗，并先关闭语音等其他浮窗。有些浮窗很相似，工具不一定能分清；发现多个时会先等待。
- 如果已经开着其他宠物修复工具，请先停用它，避免同时操作。使用本工具不需要安装 Codex Tweaks。
- 修复不会因为普通点击或打字而取消。锁屏、进入系统安全提示、宠物关闭，或你主动取消时，工具会尝试恢复原状。
- 为了避免反复打扰，连续修复之间会留一点间隔。暂时不能操作时，界面会告诉你原因。

## 设置、日志和更新

设置和日志保存在程序旁边的 `data` 文件夹。程序不会自动上传日志，也不会读取你的聊天内容、账号或密钥。

日志会用电脑默认的文本查看程序打开；打不开时会尝试 Windows 记事本，不要求安装其他软件。

更新时，先从工具里选择「退出程序」，再把新版解压到原文件夹并替换文件，保留 `data` 即可沿用设置。如果移动了文件夹，请重新关闭、开启一次「开机启动」，让它记住新的位置。

不想继续使用时，先关闭「开机启动」，退出程序，再删除整个文件夹即可。

## 遇到问题，欢迎告诉我们

在「打开日志」旁边点小箭头，选择「复制诊断摘要」，然后到 [问题反馈](https://github.com/BaoZiFly-233/codex-pet-repair/issues) 留言。可以一起写上：

- 你原本想做什么，实际发生了什么；
- Windows 和 Codex 的版本；
- 是偶尔出现，还是每次都能遇到；
- 如果是界面显示不完整，附上截图会很有帮助。

发送前可以先看一眼摘要和截图，去掉不想公开的内容。不必理解里面每个词，把发生的情况说明白就很有帮助。

## 感谢这些项目和朋友

这个工具能做出来，离不开社区公开的排查过程和现成组件：

- [openai/codex #41513](https://github.com/openai/codex/issues/41513)：记录了宠物无法点击、拖动的现象和排查线索，是本项目的起点。
- [ChatGPT-Overlay-Fix](https://github.com/FoegiUpdate/ChatGPT-Overlay-Fix)：修复方法的重要参考，给了我们恢复浮窗操作的思路。
- [Codex Tweaks](https://github.com/codex-tweaks/codex-tweaks)：早期探索和功能组织的参考。本工具后来选择独立运行，两者不需要一起安装。
- [Slint](https://github.com/slint-ui/slint) 和它的 [Gallery 示例](https://github.com/slint-ui/slint/tree/master/examples/gallery/ui)：提供界面里的按钮、开关、菜单等组件。
- [Microsoft Fluent](https://fluent2.microsoft.design/components/windows)：界面设计的参考。这里使用的是 Slint 提供的 Fluent 风格，并非微软的 WinUI 组件。
- [Rust](https://www.rust-lang.org/)、[Serde](https://serde.rs/)、[windows-rs](https://github.com/microsoft/windows-rs)、[winit](https://github.com/rust-windowing/winit) 和 [softbuffer](https://github.com/rust-windowing/softbuffer)：帮助工具在 Windows 上运行、保存设置和显示界面。

也感谢在实际使用中指出问题、反复帮忙确认效果的人。

## 开源说明

本项目自己的代码使用 [MIT 许可](LICENSE)。第三方组件有各自的使用条款，并不会因为放在这个项目里就全部变成 MIT。Slint 使用其免费桌面应用许可；界面中的「组件许可」、下载包里的 `THIRD-PARTY-NOTICES.txt` 和 `licenses` 文件夹保留了相关说明。

想查看依赖和借鉴的详细记录，可以看 [致谢与许可说明](CREDITS.md)。
