# rainbow-wetype

一个面向 agent 的 Windows skill：修改微信输入法（WeType）候选框强调色，并内置签名扫描、备份、恢复和逆向排查资料。

不是腾讯官方工具，与腾讯/微信输入法无关联。

## 能力

- 检测本机微信输入法安装目录、版本、renderer SHA256、服务状态、备份状态
- 用签名扫描定位候选 brush 构造代码，不依赖单一固定版本偏移
- 支持任意 `#RRGGBB` 颜色
- 自动备份原版 renderer
- 停服务、清理进程、写文件重试、启动服务
- 改完读回验证
- 一键恢复
- 提供完整逆向记录，升级导致签名失效时可继续排查

## 环境

- Windows 10/11
- Windows PowerShell 5.1+
- 管理员权限仅用于 `apply` / `restore`
- 已安装微信输入法
- skill 自带 `tools/wetype-color.exe`，不需要用户另外下载工具

## 使用

在 skill 目录中：

```powershell
# 1. 检测
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/detect.ps1

# 2. 扫描，确认唯一匹配
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/scan.ps1

# 3. 管理员 PowerShell 中应用
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/apply.ps1 -Color "#007AFF"

# 4. 验证
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify.ps1 -Expect "#007AFF"

# 5. 恢复
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/restore.ps1
```

脚本都输出 JSON，方便其他 agent 解析。

## 作为 Agent Skill 使用

目录名和 `SKILL.md` 的 `name` 都是 `rainbow-wetype`，符合 Agent Skills 规范。

- Pi：放到 `~/.agents/skills/rainbow-wetype` 或项目的 `.agents/skills/` 下
- 其他 agent：把整个目录交给它，读取 `SKILL.md` 后按流程执行
- 也可以直接用 `/skill:rainbow-wetype` 调用（在支持 skill 命令的 agent 中）

## 目录

```text
rainbow-wetype/
├── SKILL.md
├── README.md
├── LICENSE
├── signatures.json
├── scripts/        # detect / scan / apply / restore / verify
├── tools/          # Rust CLI 工具（无腾讯素材）
└── references/     # 逆向证据、时间线、签名说明
```

## 安全与限制

- 只改候选框，不改设置界面。
- 遇到未知特征、多个匹配、没有备份时拒绝操作。
- 修改第三方软件可能违反其许可协议；只在用户授权、已备份的前提下使用。
- 微信输入法大版本重写绘制逻辑后，签名可能失效；skill 提供排查流程，但不能保证自动适配所有未来版本。
- 不包含腾讯二进制、原版图标或备份文件。

## License

MIT，仅覆盖本仓库的脚本、文档和自有代码。微信输入法及相关商标、资源归腾讯所有。
