---
name: rainbow-wetype
description: Change or repair the WeType (微信输入法) candidate-window accent color on Windows, including after an update resets it to green or an older patch stops working. Use when the user wants a custom #RRGGBB candidate color, a failed color patch needs diagnosis, or an agent must locate and verify the renderer brush signature. The skill detects the installation, scans exact/structural signatures, backs up, patches, restarts the input service, verifies, and restores on failure.
license: MIT
compatibility: Windows 10/11; Windows PowerShell 5.1+; administrator required for apply/restore; WeType installed
metadata:
  version: "1.0.0"
  platform: windows-x64
  target: WeType 3.0.2.44 originally, signature-based for nearby builds
---

# rainbow-wetype

把微信输入法候选框的强调色改成任意 `#RRGGBB`，并在微信输入法升级导致旧补丁失效时，用签名扫描 + 排查文档定位新版本。

## 先看边界

- 本 skill **不安装微信输入法**。用户机器必须已经安装。
- 只改候选框，不改设置界面。
- `apply` / `restore` 需要管理员权限。没有管理员权限时脚本返回 `administrator_required`，不要假装成功。
- 不要上传或分发微信输入法原版文件、备份文件、原版图标。
- 这不是腾讯官方工具，使用前必须备份并自担风险。

## 标准流程

所有命令在 skill 目录下执行，脚本输出 JSON。

### 1. 检测

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/detect.ps1
```

检查：

- `ok` 是否为 `true`
- `selected` 是否是 `wetype_renderer.exe` 所在目录
- `renderer.version` / `renderer.sha256`
- `backup` / `backupExists`
- `isAdmin`

如果 `candidates` 有多个，优先选版本匹配、路径正确的那个，再用 `-Dir` 明确指定。

### 2. 扫描签名

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/scan.ps1 -Dir "C:\Program Files\Tencent\WeType\3.0.2.44"
```

检查：

- `supported` 是否为 `true`
- `signature` 是 `exact` 还是 `struct`
- `matches[].matchCount` 是否等于 `1`
- `currentColor`

规则：

- **只有唯一匹配才能改。**
- `matchCount > 1` 说明特征太宽，必须拒绝。
- `supported=false` 时阅读 `references/reverse-engineering.md`，不要盲改。

### 3. 应用颜色

必须使用管理员 PowerShell：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/apply.ps1 -Color "#007AFF" -Dir "C:\Program Files\Tencent\WeType\3.0.2.44"
```

脚本会调用同目录的 `tools/wetype-color.exe`：

- 备份原版 renderer
- 停止 WeType 服务并清理进程
- 扫描特征
- 写入颜色
- 重启服务
- 读回并验证

`ok=true` 且 `verify.matchesExpected=true` 才算成功。

### 4. 验证

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify.ps1 -Expect "#007AFF"
```

`verify.ps1` 不依赖截图，适合 headless agent。  
如果当前是交互式桌面，用户可以再输入文字确认候选框。

### 5. 恢复

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/restore.ps1
```

只在存在原版备份时工作。恢复失败时保留日志，不要删除备份。

## 微信输入法升级后的处理

先按顺序做：

1. `detect.ps1`：确认新版本、路径、备份。
2. `scan.ps1`：先试 exact，再试 struct。
3. 如果 struct 唯一匹配：可以继续 apply（已有备份时）。
4. 如果都失败：不要扩大通配符乱试；阅读 `references/reverse-engineering.md`，按里面的 D2D/brush 排查流程定位，再把新签名追加到 `signatures.json`。
5. 新签名必须满足：
   - 只在 renderer 中唯一
   - 能同时在原版和已修改文件上匹配
   - 相对偏移与 R/G/Gdark/B/Bdark 对应
6. 验证通过后，把新签名、版本、hash、验证结果写进 `references/signatures.md` 或 `signatures.json`。
7. Rust CLI 的 `apply` 会直接读取 `signatures.json`，所以新增签名不需要重新编译工具；只需重新跑 scan/apply/verify/restore。
8. 如果改的是结构/偏移格式，先跑完整流程确认唯一性，再提交。

## 文件说明

```text
SKILL.md
README.md
LICENSE
signatures.json
scripts/
  lib.ps1
  detect.ps1
  scan.ps1
  apply.ps1
  restore.ps1
  verify.ps1
tools/
  wetype-color.exe          # Rust CLI；动态读取 signatures.json
references/
  reverse-engineering.md   # 当时怎么定位的、为什么
  investigation-log.md     # 时间线、失败路线
  signatures.md            # 签名格式与追加方法
  troubleshooting.md       # 常见故障
```

## 重要原则

- 不要因为版本号变了就修改旧偏移硬写。
- 不要修改 `41 B8` 这类 opcode；只改立即数/浮点字节。
- 不要在未知字节、多个匹配、没有备份时操作。
- 文件被占用时循环终止对应安装目录下的 renderer，并重试写入。
- 服务停止/启动失败时明确报告，不要吞掉错误。
- 不常驻、不做覆盖层、不改设置界面。

详细证据和排查树见 `references/reverse-engineering.md`。
