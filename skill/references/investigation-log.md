# 排查时间线 / 踩坑记录

这是实际做 3.0.2.44 时的过程摘要，按“线索 -> 结果”记录，方便未来 agent 少走弯路。

## 初始假设：主题资源

- 检查 WXZ 资源，尝试改 `Color_BG_03`、`choose_bg.png`
- 结果：候选窗口颜色不变
- 结论：候选框不读这些主题资源

## 继续：DLL/app.so 常量

- 在 `wetap_business.dll`、`app.so` 中找绿色字节
- 改 `app.so` 的 `#00B176` 常量
- 结果：候选不变；设置页也不是由这个常量主导
- 结论：候选是原生 renderer 绘制，不是 Flutter

## 窗口层级

- 找到候选父/子窗口：
  - parent `WeType.NativeCandidate.Decoration.v2`
  - child `WeType.NativeCandidate.Direct2D.v2`
- 用 `CandHide` 隐藏 child：
  - 绿色像素从 1624 -> 0
  - 恢复 child 后绿色复现
- 结论：绿色在 Direct2D 子窗口内，父窗口主要画阴影

## DrawBitmap 假线索

- 在绘制链里看到 `DrawBitmap`（`0x115167 -> 0x11526F`）
- 一度以为 bitmap 是绿色来源
- 实际检查 bitmap vector：
  - 198288 字节，49572 像素
  - green 分量 0
  - 第一像素 `00000000`
- 结论：DrawBitmap 只画阴影；绿色来自更早的 brush 构造

## brush 对象

- 扫描 D2D brush 对象，找到：
  - vtable = d2d1 基址 + `0x2D3CC0`
  - 颜色在对象 `+0x38`，RGBA float
- 绿色 brush：`0, 0.6941177, 0.4627451, 1` = `#00B176`
- 启动后未输入时 6 个 brush 全黑；首次候选显示后 7 个，其中 1 个绿色
- 直接改 brush 颜色不持久：每帧/下次绘制会重建

## 反查代码

- 反汇编 RVA `0x115900` 函数
- 在 `0x115A9E` 附近找到构造代码：
  - `mov dword [rbp-0x78], 0`（R float）
  - `mov eax, 0xB1`（G）
  - `mov r8d, 0xA8`（暗色 G）
  - `mov eax, 0x76`（B）
  - `mov r8d, 0x6F`（暗色 B）
- 文件偏移：
  - R：`0x114EAC`
  - G：`0x114EB1`
  - Gdark：`0x114EB7`
  - B：`0x114ED4`
  - Bdark：`0x114EDA`

## 第一次改错

- 一开始把暗色 G 的地址当成 `0x114EB6`
- 这个位置是 `41 B8` 的 opcode，不是立即数
- 结果：候选窗口无法正常启动/绘制
- 修正：改 `0x114EB7` 的 `A8`
- 教训：**只改立即数，不碰 opcode**

## 验证

- 只改 G/B：候选变蓝但 R 始终为 0
- 追加 R 的 float 位模式后：
  - `#007AFF` -> 候选 `#007AFF`
  - `#FF0000` -> 候选 `#FF0000`
- 重启服务/重启后重新输入仍生效
- 恢复原版 -> `#00B176`

## 设置界面

- 设置窗口 `wetype.flutter.setting`，Flutter
- 默认隐藏，需管理员 `SetWindowLongPtr` 去掉 `WS_CHILD` 才能截图分析
- WeUIResource WXZ 改色、wetype_update WXZ 改色、app.so 常量改色均未能稳定改变主界面
- 设置页绿色大量来自图片资源（如 `App_two_light.png` 的 `#23C891`）
- 结论：按用户要求放弃设置界面，只做候选框

## 从固定偏移到签名

- 固定 `TargetVersion` 在升级后直接失效
- 改成 exact + struct 签名扫描：
  - exact：3.0.2.44 原始字节
  - struct：颜色/栈偏移通配，保留 opcode 结构
- 两种签名当前都唯一
- 已知失败模式：
  - 版本变化但代码结构不变：struct 可命中
  - 编译器/绘制逻辑重写：可能全部失效，需要按 `reverse-engineering.md` 继续排查

## 最终产物

- 3.0.2.44 可用
- 候选颜色任意 `#RRGGBB`
- 备份/恢复/无备份拒绝
- 路径自动探测/手选/`--dir`
- 本 skill 将这些流程和证据交给其他 agent
