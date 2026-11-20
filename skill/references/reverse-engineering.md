# 微信输入法候选框改色 — 逆向记录

本文记录 2026-11 对微信输入法 PC 3.0.2.44 的定位过程。  
目标不是“记住五个偏移”这么简单，而是让后来的人/agent 在版本变化时知道**该看什么、哪些是假线索、怎样验证唯一性**。

## 1. 目标与边界

- 目标软件：微信输入法 PC 3.0.2.44
- 目标：候选窗口选中项/强调色，默认 `#00B176`
- 结果：直接修改 `wetype_renderer.exe` 的 brush 构造立即数，持久生效，不需要常驻
- 不涉及：设置界面、安装包、云端配置、输入法其他功能

## 2. 候选窗口结构

候选不是 Flutter 设置页，而是原生 Direct2D：

- 子窗口 class：`WeType.NativeCandidate.Direct2D.v2`
- 父窗口 class：`WeType.NativeCandidate.Decoration.v2`（主要画阴影）
- 绿色实际在**子窗口**内；隐藏子窗口后绿色像素归零，父窗口仍在
- 候选窗口过程 RVA 约 `0x118C80`
- `WM_PAINT` 附近 `0x118DDB`，绘制链：
  `0x116FF0 -> 0x114760 -> 0x115167 DrawBitmap -> 返回 0x11526F`

重要假线索：

- `0x115167` 的 `DrawBitmap` 只画阴影 bitmap，不是绿色来源
- 该 bitmap vector 约 198288 字节，green 分量为 0
- 不要因为它“在绘制函数里”就去改它

## 3. 绿色来源：D2D brush

### 3.1 运行中观察

使用静态内存扫描找到 D2D brush 对象：

- d2d1 基址每次启动不同（示例 `0x7FF854AC0000`）
- `ID2D1SolidColorBrush` vtable = d2d1 基址 + `0x2D3CC0`
- 颜色布局：对象 `+0x38` 开始是 RGBA 四个 float
- 默认绿色 brush：`rgba = 0, 0.6941177, 0.4627451, 1`
  - `G = 0.6941177 * 255 ≈ 0xB1`
  - `B = 0.4627451 * 255 ≈ 0x76`
  - 即 `#00B176`

启动后未输入时约 6 个 brush，都是黑色；第一次显示候选后变成 7 个，其中 1 个是绿色。绿色 brush 是**首次候选渲染时创建**的。

直接改这个 brush 对象的颜色不能持久：渲染路径会重新构造/重写颜色，所以必须改构造代码。

### 3.2 反查代码

反汇编 RVA `0x115900` 函数，在 `0x115A9E` 附近找到构造选中项 brush 的代码：

```asm
; 精简示意
mov dword [rbp-0x78], 0     ; R = 0.0f
mov eax, 0xB1               ; G = 177
mov r8d, 0xA8               ; 暗色 G = 168
...
mov eax, 0x76               ; B = 118
mov r8d, 0x6F               ; 暗色 B = 111
```

原版文件位置（3.0.2.44）：

| 字段 | 文件偏移 | 原值 | 说明 |
|---|---|---|---|
| R float 立即数 | `0x114EAC..0x114EAF` | `00 00 00 00` | 属于 `C7 45 88 imm32` |
| 普通 G | `0x114EB1` | `B1` | `B8 imm32` |
| 暗色 G | `0x114EB7` | `A8` | `41 B8 imm32` |
| 普通 B | `0x114ED4` | `76` | `B8 imm32` |
| 暗色 B | `0x114EDA` | `6F` | `41 B8 imm32` |

**不要修改 `41 B8` opcode。**  
早期误改 `0x114EB6` 破坏了 `41 B8`，候选窗口直接无法正常绘制；正确目标是它后面的立即数字节。

## 4. 当前签名

签名起点为文件 `0x114EA5`（RVA `0x115A9E` 附近，具体以文件为准）。

精确特征（原版绿色）：

```text
0F B6 53 78 C7 45 88 00 00 00 00 B8 B1 00 00 00 41 B8 A8 00 00 00
84 D2 41 0F 45 C0 0F 57 C0 F3 48 0F 2A C0 F3 41 0F 5E C4 F3 0F 11 45 8C
B8 76 00 00 00 41 B8 6F 00 00 00
```

结构特征（颜色和栈偏移通配）：

```text
0F B6 53 78 C7 45 ?? ?? ?? ?? ?? B8 ?? ?? 00 00 41 B8 ?? ?? 00 00
84 D2 41 0F 45 C0 0F 57 C0 F3 48 0F 2A C0 F3 41 0F 5E C4 F3 0F 11 45 ??
B8 ?? ?? 00 00 41 B8 ?? ?? 00 00 41 0F 45 C0
```

相对偏移：

```text
R     = start + 7   (4 字节 float)
G     = start + 12
Gdark = start + 18
B     = start + 47
Bdark = start + 53
```

在当前 3.0.2.44 上，两种特征都只匹配一次，起点 `0x114EA5`。

## 5. 为什么固定版本号会失效

旧工具用：

```csharp
const string TargetVersion = "3.0.2.44";
```

版本一变就拒绝，或者偏移移动后改错。  
正确方向是：

1. 先按已收集的 signature 扫描；
2. 命中且唯一才改；
3. 不命中时按本文档继续定位；
4. 把新签名写回数据库。

## 6. 新版本排查决策树

### 第一步：确认目标仍是候选窗口

- 用 Win32 `EnumWindows` 找 class 为 `WeType.NativeCandidate.Direct2D.v2` 的可见子窗口
- 隐藏该窗口，绿色应该消失；恢复显示，绿色返回
- 如果 class 或绘制技术变了，先记录新窗口结构，不要直接改旧偏移

### 第二步：确认仍是 D2D brush

- 在 renderer 进程中扫描已提交可读内存
- 目标对象特征：
  - vtable 指向 `d2d1.dll + 0x2D3CC0`（不同 d2d1 版本偏移可能变）
  - 对象 `+0x38` 是 RGBA float
  - 默认候选绿色分量为 `G≈0.694`、`B≈0.463`
- 找候选首次显示前后 brush 数量变化，定位新创建的绿色 brush

### 第三步：从 brush 反查构造代码

- 对 `CreateSolidColorBrush` 下断点/挂钩，观察调用栈
- 或对绿色 brush 对象下硬件写断点，找写入 `+0x38` 的代码
- 往调用者回溯，找 `mov` 立即数/float 构造位置
- 目标通常是同一个函数里连续构造 R、G/B（普通/暗色）两套分支

### 第四步：只改立即数

- G/B 是 `mov eax, imm32` / `mov r8d, imm32` 的立即数低位
- R 可能是 `mov dword [rbp+disp], imm32` 的 float 位模式
- 修改前校验原值；修改后必须在临时副本上验证
- 不要改 opcode、寄存器、分支结构

### 第五步：生成并验证新签名

- 从命中位置截取足够长的字节序列
- 颜色/栈偏移用 `??` 通配，但保留 opcode 和调用顺序
- 要求：
  - 原版唯一
  - 已修改文件仍唯一
  - 不匹配资源区/其他函数
- 写入 `signatures.json`，更新 `references/signatures.md`

## 7. 验证方法

- 读回目标字节，确认 R/G/B 已写入
- 解码 `#RRGGBB` 与用户输入一致
- 交互式桌面：输入文字触发候选，截图统计主色
- 候选不出现时不要误判失败；需要重新输入/等待
- 修改失败或候选异常：恢复备份并重启服务

## 8. 已证伪/不要走的路线

- WXZ 主题里的 `Color_BG_03`：候选不读取
- `choose_bg.png`：不是候选强调色来源
- `wetap_business.dll` 的绿色字节：误报/指针
- `app.so` 里的 `#00B176`：候选不读
- 设置界面：绿色很多来自 Flutter 图片资源（如 `App_two_light.png` 的 `#23C891`），不是单一主题常量；本 skill 不做
- 覆盖层/常驻程序：用户明确不要

## 9. 调试工具记录

当时使用的辅助程序（没有随 skill 分发二进制）：

- `FindBrush`：扫描 D2D brush 对象
- `D2DBrushLayout`：验证 brush 布局/颜色偏移
- `MemScan`：进程内存扫描/读写
- `MiniDbg` / `MultiDbg`：软件断点/单步，定位绘制调用链
- `CandHide`：隐藏候选子窗口，验证绿色归属
- `SnapWin` / `SnapAllSettings`：截图和颜色统计

这些工具的作用是**定位**；最终 skill 只做签名扫描和文件修改，不常驻、不注入。

## 10. 相关文件

- `references/investigation-log.md`：时间线和踩坑记录
- `references/signatures.md`：签名格式和追加流程
- `references/troubleshooting.md`：常见故障
- `signatures.json`：当前签名数据库
