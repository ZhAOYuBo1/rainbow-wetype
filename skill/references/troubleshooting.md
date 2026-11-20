# 常见故障

## `ok=false` / `administrator_required`

`apply.ps1` 和 `restore.ps1` 需要管理员权限。  
让用户用管理员 PowerShell 运行，或者让 agent 在已提升的会话中调用。不要自动绕过 UAC。

## 检测不到微信输入法

检查：

- 是否安装的是 PC 版微信输入法
- 安装目录是否包含 `wetype_renderer.exe`
- 是否安装在非标准路径
- 是否被杀毒软件隔离

可以手动指定：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/scan.ps1 -Dir "D:\SomeDir\WeType\3.0.2.44"
```

## `supported=false`

`scan.ps1` 找不到唯一匹配：

1. 看 `matches[].matchCount`
2. 如果全部为 0：按 `references/reverse-engineering.md` 重新定位
3. 如果某个签名 >1：说明通配太宽或代码重复，不能改，先收紧签名
4. 如果版本升级：先记录新版本/hash，不要沿用旧偏移

## 修改后候选没变色

- 候选窗口需要重新输入/重新显示；旧窗口不会自动刷新
- 确认服务已经重启
- 用 `verify.ps1 -Expect "#RRGGBB"` 检查文件字节
- 如果字节正确但界面不对，检查是否改的是另一份安装/另一个 renderer

## 文件被占用

脚本/工具会终止选中安装目录下的 `wetype_renderer` 并重试写文件。  
如果仍失败：

- 检查是否有安全软件拦截
- 检查服务是否自动拉起 renderer
- 手动停止 `WeType Management Service` 后重试
- 不要直接删除被占用的文件

## 服务停不下来 / 起不来

- 检查服务名：`Get-Service *WeType*`
- 检查服务 ImagePath 是否指向当前安装目录
- 不要强杀系统服务；失败时报告并保留备份
- 修改失败后至少尝试把服务拉起来

## 候选窗口无法显示/输入法异常

立即恢复：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/restore.ps1
```

然后重启输入法或重新登录。  
如果备份也损坏，重新安装微信输入法；不要继续在未知字节上试。

## 没有备份

`restore.ps1` 会拒绝。  
工具在第一次 apply 时自动备份到：

```text
%ProgramData%\WeTypeColor\backup\<版本>\wetype_renderer.exe.orig
```

如果备份丢失，只能重装微信输入法恢复。

## 路径检测到多个版本

`detect.ps1` 的 `candidates` 会列出多个目录。  
用 `-Dir` 明确指定目标，不要依赖自动选择。

## 360/杀毒拦截

- 不要关闭用户的安全软件来“强行成功”
- 让用户把工具/脚本加入信任，或手动在管理员终端运行
- 说明这是修改第三方软件的工具，存在误报可能
- 不要隐藏行为、不要静默提权

## 设置界面颜色

本 skill 不支持设置界面。  
设置页绿色大量来自 Flutter 图片资源，不是候选 brush 的同一套逻辑；不要用候选签名去改设置资源。
