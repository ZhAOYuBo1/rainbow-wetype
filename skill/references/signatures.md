# 签名数据库说明

`signatures.json` 是定位候选 brush 构造代码的唯一依据。  
`scripts/scan.ps1` 会按 `priority` 从小到大扫描，返回每个签名的命中数量；只有 `matchCount == 1` 才可用于修改。

## 结构

```json
{
  "schema": 1,
  "knownBuild": "3.0.2.44",
  "offsets": {
    "r": 7,
    "g": 12,
    "gDark": 18,
    "b": 47,
    "bDark": 53
  },
  "patterns": [
    {
      "name": "wetype-candidate-brush-exact-30244",
      "kind": "exact",
      "priority": 10,
      "requiresUnique": true,
      "pattern": "0F B6 ...",
      "notes": "..."
    }
  ]
}
```

- `pattern`：空格分隔的十六进制字节；`??` 表示任意一个字节
- `offsets`：相对于签名起点的偏移
  - `r`：4 字节 float（小端）
  - `g`：普通 G 立即数
  - `gDark`：暗色 G 立即数
  - `b`：普通 B
  - `bDark`：暗色 B
- `kind`：`exact` 优先，`struct` 用于已修改或布局轻微变化
- `priority`：数字越小越先扫描

## 添加新版本签名

1. 按 `reverse-engineering.md` 在新版本上找到 brush 构造位置
2. 从相同起点截取至少覆盖以下内容的字节：
   - R 构造
   - G / 暗色 G 选择
   - B / 暗色 B 选择
3. 颜色立即数、栈偏移用 `??`，但保持 opcode/调用顺序
4. 在**原版**和**已修改** renderer 上分别测试：
   - `scripts/scan.ps1` 必须只命中一次
   - 命中的 `currentColor` 应与实际颜色一致
5. 追加 JSON 条目，写清版本、hash、备注
6. 用 `scripts/apply.ps1` 做一次临时改色，再 `restore.ps1` 恢复
7. 在 `investigation-log.md` 记录新版本的定位过程

## 禁止事项

- 不要用只包含 `??` 的宽泛模式
- 不要因为“先跑通”就忽略多匹配
- 不要把资源文件里的绿色字符串当签名
- 不要在未备份时试改
- 不要修改 opcode；只改立即数/浮点字节

## 当前指纹

- 已知构建：`3.0.2.44`
- 原版 renderer SHA256：
  `18564AD61BD01DDA0ED4830A28D3508CD4C39B123BD0DA49C786982EC447995E`
- 签名起点：文件 `0x114EA5`
- 当前 exact/struct 都唯一匹配

## 注意：scan 与 apply 的签名来源

- `scripts/scan.ps1` 读取 `signatures.json`
- `scripts/apply.ps1` 调用的 Rust `tools/wetype-color.exe` 也动态读取同一个 `signatures.json`
- 因此新增签名后**不需要重新编译**工具；重新跑 scan/apply/verify/restore 即可
- 只有修改签名文件格式或扫描逻辑时，才需要改 Rust 源码并重新构建
