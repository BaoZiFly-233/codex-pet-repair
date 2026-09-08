<div align="center">

<img src="assets/pet-repair-pixel.svg" width="120" height="120" alt="Codex 宠物修复图标">

# Codex 宠物修复

宠物还在桌面上，却怎么也拖不动？点一下，试着恢复它。

**[⬇ 下载使用](https://github.com/BaoZiFly-233/codex-pet-repair/releases/latest)**　·　[📖 开始使用](#三步开始)　·　[💬 反馈问题](https://github.com/BaoZiFly-233/codex-pet-repair/issues)

Windows 64 位 · 解压即用 · 完整解压约 11 MB

</div>

---

## 🖼️ 界面展示

浅色、深色都支持，右上角就能切换。

<table>
<tr><th align="center">浅色</th><th align="center">深色</th></tr>
<tr>
<td width="50%"><img src="assets/screenshots/light.png" width="432" alt="Codex 宠物修复浅色界面预览"></td>
<td width="50%"><img src="assets/screenshots/dark.png" width="432" alt="Codex 宠物修复深色界面预览"></td>
</tr>
</table>

<a id="三步开始"></a>

## 🚀 三步开始

**1 · 下载并解压**<br>
在下载页选择 Windows 压缩包，完整解压到一个方便找到的文件夹。

**2 · 打开工具和宠物**<br>
运行 `PetRepair.exe`，打开 Codex，让宠物出现在桌面上。请先关闭其他宠物修复工具，并尽量只保留一个宠物浮窗。

**3 · 点一下，再试着拖动**<br>
点击「立即修复」，等待约 3 秒后试着拖动宠物。如出现效果确认，选择「可以拖动」或「仍然不行」。

## 🎛️ 按你的习惯使用

<table>
<tr><th width="150" align="left">想做什么</th><th align="left">可以这样做</th></tr>
<tr><td><strong>需要时修一下</strong></td><td>点击「立即修复」，过程中也可以取消。</td></tr>
<tr><td><strong>少操一点心</strong></td><td>确认当前 Codex 版本有效后，开启「自动修复」。宠物重新出现时，工具会自动尝试。</td></tr>
<tr><td><strong>让 V2 宠物看向鼠标</strong></td><td>开启「看向鼠标」，按卡片提示以跟随模式启动 Codex。移动宠物附近的鼠标，视线由 Codex 自己绘制。</td></tr>
<tr><td><strong>让界面收起来</strong></td><td>点击「托盘」或关闭窗口；双击托盘图标可重新打开。开启「仅托盘启动」后，下次也会直接收起来。</td></tr>
<tr><td><strong>登录时启动</strong></td><td>开启「开机启动」，工具会在登录 Windows 后留在托盘中。</td></tr>
<tr><td><strong>换个明暗风格</strong></td><td>右上角的「深色」开关可以直接切换，并记住你的选择。</td></tr>
<tr><td><strong>查看运行记录</strong></td><td>点击「打开日志」；旁边的小箭头还能复制诊断摘要、打开日志文件夹。</td></tr>
</table>

自动修复、开机启动和看向鼠标默认关闭，开关分别保存；Codex 更新后，需要重新确认拖动修复效果。

想看看有哪些变化？可以翻翻 [更新记录](CHANGELOG.md)。构建方法见 [开发说明](DEVELOPMENT.md)。

## ❓ 常见问题

<details>
<summary><strong>“看向鼠标”怎么开启？</strong></summary>

开启「看向鼠标」，按界面提示启动跟随模式，再打开一个 V2 宠物。若 Codex 已在运行，请先从它的菜单完全退出，再点击「以跟随模式启动 Codex」。

在宠物周围移动鼠标，它就会改变视线；停下后恢复待机。拖动和点击宠物仍照常使用。

跟随模式会开启本机调试连接，请勿将其开放到网络。完全退出 Codex 后，正常启动即可恢复普通模式。

</details>

<details>
<summary><strong>关闭窗口后，工具还在运行吗？</strong></summary>

是的，关闭窗口只会收起界面，已经开启的自动修复会继续工作。想完全关闭，请点击「退出程序」，或使用托盘菜单里的「退出」。

</details>

<details>
<summary><strong>为什么有时需要等待，或者修复没有效果？</strong></summary>

连续修复之间会留一点间隔，原因会显示在界面上。宠物和语音等浮窗有时很相似，发现多个时工具会先等待，请先关闭其他浮窗再试。

修复不会修改 Codex 的安装文件，效果可能随 Codex 更新而变化。

</details>

<details>
<summary><strong>设置放在哪里？怎么更新或卸载？</strong></summary>

设置和日志保存在程序旁边的 `data` 文件夹。

更新时先选择「退出程序」，再把新版解压到原文件夹并替换文件，保留 `data` 就能沿用设置。

移动文件夹后，请重新关闭、开启一次「开机启动」。不再使用时，先关闭「开机启动」，退出程序，再删除文件夹即可。

</details>

<details>
<summary><strong>日志怎么看？会自动上传吗？</strong></summary>

日志会用电脑默认的文本查看程序打开，不会自动上传。工具不会读取你的聊天内容、账号或密钥。

</details>

## 💬 遇到问题，告诉我们

在「打开日志」旁边点小箭头，选择「复制诊断摘要」，然后 [新建一条问题反馈](https://github.com/BaoZiFly-233/codex-pet-repair/issues)。写清楚原本想做什么、实际发生了什么、是否每次都会出现；界面显示异常时，截图也很有帮助。

发送前先看一眼摘要和截图，去掉不想公开的内容。

## 🤝 感谢与开源

修复线索来自 [Codex #41513](https://github.com/openai/codex/issues/41513)，恢复方法参考了 [ChatGPT-Overlay-Fix](https://github.com/FoegiUpdate/ChatGPT-Overlay-Fix)，早期探索也借鉴了 [Codex Tweaks](https://github.com/codex-tweaks/codex-tweaks)。界面由 [Slint](https://github.com/slint-ui/slint) 提供，采用 Fluent 风格。

完整的组件、借鉴和图标来源见 **[致谢与许可说明](CREDITS.md)**。本项目自己的代码使用 [MIT 许可](LICENSE)，第三方组件和宠物形象的权利仍归各自权利人所有。

友情链接：[LINUX DO](https://linux.do)。

## ⭐ Star 趋势

<a href="https://www.star-history.com/#BaoZiFly-233/codex-pet-repair&Date">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=BaoZiFly-233/codex-pet-repair&amp;type=Date&amp;theme=dark">
<source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=BaoZiFly-233/codex-pet-repair&amp;type=Date">
<img src="https://api.star-history.com/svg?repos=BaoZiFly-233/codex-pet-repair&amp;type=Date" alt="Codex 宠物修复的 GitHub Star 趋势" width="800">
</picture>
</a>

---

<div align="center">
<sub>非 OpenAI 官方产品，与 OpenAI 没有隶属或授权关系。</sub>
</div>
