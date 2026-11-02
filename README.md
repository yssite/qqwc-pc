# 全球网测 · 桌面版（逆向还原 + Tauri 移植）

对 Android 应用 **全球网测 v4.4.8**（`com.cnspeedtest.globalspeed`，中国信通院 CAICT）做完整逆向，
并用 **Tauri 2 + Rust** 重写为跨平台桌面测速工具（无 Electron、无广告、无账号、免费）。
使用 DeepSeek Harness 逆向、开发。
<img width="2080" height="1402" alt="39bf6f8cd55a166091916833b66488c8" src="https://github.com/user-attachments/assets/1ebcc13c-1cbc-4418-aaa1-124cd662f067" />

---

## 成果一览

| 类别 | 内容 | 位置 |
|---|---|---|
| 逆向报告 | 加固分析、脱壳过程、包结构、native 层、算法还原 | [docs/01-逆向分析报告.md](docs/01-逆向分析报告.md) |
| 协议规格 | 全部接口、token 算法、节点表 DES 密钥、采样与统计口径 | [docs/02-协议规格.md](docs/02-协议规格.md) |
| 架构设计 | 原版 / 移植版架构图（Mermaid）、时序图、移植映射 | [docs/03-架构设计.md](docs/03-架构设计.md) |
| 逆向脚本 | 脱壳 dump 器、节点表解密、协议验证、ELF/JNI 解析 | [tools/re/](tools/re/) |
| 节点库 | 596 个测速节点（已内置进客户端） | [app/src-tauri/resources/nodes.json](app/src-tauri/resources/nodes.json) |
| PC 客户端 | Tauri 2 + Rust 工程（Windows / macOS / Linux） | [app/](app/) |

> 反编译得到的 Java 源码（2,576 文件）与 `libGSCore.so` 的 Ghidra 伪代码、原始 APK
> **不入库**：一是体积（~40MB），二是其中包含第三方版权代码。需要时按
> [docs/01-逆向分析报告.md](docs/01-逆向分析报告.md) 的步骤可完整复现。

---

## 关键逆向结论（速览）

1. **360 加固**：真 dex 加密追加在 `classes.dex` 尾部（0x4000 之后），少量方法被 native 化。
2. **脱壳**：自建 API36 x86_64 模拟器（带 ARM 翻译）→ `/proc/pid/mem` 扫描 → 72 个 dex 镜像，主 dex 5,529 类。
3. **协议**：
   - 节点匹配 `mobilematch_many.php`
   - 会话 `GET /speed/dovalid?…&token=…` → `1-<uuid>`
   - 下行 `GET /speed/File(1G).dl?r=…&key=<uuid>`
   - 上行 `POST /speed/doAnalsLoad.do`（multipart，随机数据流）
   - 结束 `POST /speed/dovalid?key=<uuid>`
4. **token 算法**（native `Jni.b` 逆向 + 实网校验）：

   ```
   token = md5( md5("model={model}&imei={imei}") + md5("stime={t}&band={bw}&rand=12345555") )
   ```
5. **节点表 DES 密钥**：`dw!@#$%^`（DES/ECB/PKCS5，hex 文本）。
6. **采样口径**：500ms 间隔、2s 预热、1s 滑动窗口；平均值为剔除预热后的全量平均（Android 15+ 行为）。

---

## 编译

前置：Rust（msvc）+ VS2022 C++ 生成工具 + WebView2 Runtime（Win10/11 自带）。

```powershell
cd app\src-tauri
cargo build --release
# 首次编译约 3-8 分钟
```

产物（命名统一带版本号，见 `tools/release.ps1`）：

- 安装包：`dist/GlobalSpeedTest_<ver>_x64-setup.exe`（NSIS）
- 便携版：`dist/GlobalSpeedTest_<ver>-portable.exe`（双击即用，需系统自带 WebView2）

### 变更记录

**v1.2.2**（当前版本）
- 修复「批量测延迟」结果不显示：Rust 回传键由 `ip:port` 改为**输入下标**，与前端行键对齐
- 新增延迟排序：默认 / 低→高 / 高→低，批量测完自动升序；超时与未测项排在末尾
- 测速结束后**仪表指针归零、中央显示 `--`**，顶部三张卡片保留最终平均值
- 新增 macOS arm64 构建（`.dmg`，ad-hoc 签名），并修正 `open` 分支、`~/Pictures` 输出目录等平台假设

**v1.2.1**
- 修复测速结束后指针又被拉回下行平均值的问题（收尾时不再切换指针方向）

**v1.2.0**
- 修复点「批量测延迟 / 网络诊断」界面卡死：阻塞型命令改 `async` + `spawn_blocking`，不再占用主线程
- 批量测延迟改为 Rust 侧 12 线程并发（36 节点 2.3s，原先串行约 36s），新增「停止测延迟」
- 延迟结果缓存渲染，切换选项卡不再丢失/中断
- 设置页省份/城市改为联动下拉（数据来自节点库，选省后才可选市）
- 去掉「签约带宽」界面项，内部固定 200（与原版 WiFi 分支 `setBandwidth(200)` 一致）
- 查看历史记录时左侧卡片加 50% 半透明 + 20px 高斯模糊遮罩，显示「正在查看历史纪录 + 时间」
- 新增「保存结果图」：导出 900×1240 结果长图，保存后自动用默认看图程序打开
- 首选节点会话失败时自动切换备选节点（最多 3 个）
- **修复 HTTPS 后端不可用**：`ureq` 关闭默认特性后未启用 TLS 后端，节点匹配/节点表刷新一直静默失败（此前靠内置表兜底）

**v1.1.0**
- 修复上下行速率**只有真实值一半**的采样窗口换算错误，改为真实时间戳滑动窗口（与网卡计数交叉校验）
- 指标卡片改显示**加权平均**（剔除预热 2s + 尾部 0.5s），不再显示结束瞬间的衰减值
- 仪表盘指针与数字改为 60fps 指数平滑插值
- 图表铺满整宽；仪表盘按可用空间自适应并封顶，圆弧不再被裁切
- 新增公网出口 IP / 归属地 / 运营商识别，默认据此做「就近 + 同运营商」节点匹配
- 修复 `tracert` 中文乱码；默认窗口调整为 1040×700

**v1.0.0**
- 首个可运行版本：节点匹配、多线程上下行测速、实时曲线、历史记录、网络诊断

打包安装程序（可选）：

```powershell
cd app
npx @tauri-apps/cli build
```

---

## macOS 构建 / 安装

本机构建（在你的 Mac 上）：

```bash
cd app
./build-macos.sh            # 当前架构
./build-macos.sh universal  # arm64 + x86_64 通用包
```

CI 构建：`.github/workflows/build-macos.yml`（macOS runner 出通用 dmg，作为 artifact 下载）。

无 GUI 的远程 Mac（SSH）构建注意：`bundle_dmg.sh` 依赖 Finder AppleScript，
无窗口会话下会 `AppleEvent 超时`。此时用：

```bash
# 1) 只编译，不打包
npx tauri build --target aarch64-apple-darwin --no-bundle
# 2) 手动 ad-hoc 签名（Apple Silicon 必须）
codesign --force --deep --sign - \
  src-tauri/target/aarch64-apple-darwin/release/bundle/macos/GlobalSpeedTest.app
# 3) hdiutil 封装
hdiutil create -volname "全球网测" -srcfolder <含 .app 与 /Applications 软链的目录> \
  -ov -format UDZO GlobalSpeedTest.dmg
```

安装：`.dmg` 里的 App 拖进 Applications。首次打开被 Gatekeeper 拦时，右键 → 打开，
或执行 `xattr -dr com.apple.quarantine /Applications/GlobalSpeedTest.app`。

---

## 使用

1. 打开程序 →「网络测速」页点 **开始测试**：
   - 自动调用 CAICT 节点匹配接口，再对候选节点做 TCP 探测，选最优；
   - 依次执行 Ping → 下行 → 上行 → 结束会话 → 评分入库。
2. 「测速节点」页：596 个节点，可按省份/运营商筛选、搜索、批量测延迟；点任意行即以其测速。
3. 「历史记录」页：查看/删除/导出 CSV。
4. 「网络诊断」页：Ping（TCP RTT）、DNS 解析、路由追踪。
5. 「设置」页：连接数、时长、带宽档位、节点偏好。

数据目录：`%APPDATA%\cn.caict.globalspeed.pc\`（`settings.json` / `history.json`）。
**不上传任何测速结果。**

---

## 目录

```
.
├─ app/                      Tauri 桌面客户端
│  ├─ src-tauri/
│  │  ├─ src/                Rust：main / api / speed / store / model
│  │  ├─ resources/nodes.json  596 个节点（内置）
│  │  └─ icons/              应用图标（png / ico / icns）
│  ├─ ui/                    前端：index.html / style.css / app.js（无构建步骤）
│  ├─ build-macos.sh         一键出 .dmg
│  └─ package.json           仅用于拉 @tauri-apps/cli
├─ docs/                     逆向报告 / 协议规格 / 架构设计
├─ tools/
│  ├─ re/                    逆向脚本（脱壳 dump、解密、协议验证）
│  ├─ cdp_drive.py 等        WebView2 远程调试驱动 / 截图（自动化测试用）
│  └─ mac_bootstrap.sh       远程 Mac 环境初始化
└─ .github/workflows/        macOS dmg 的 CI 构建
```

## 发布

[Releases](../../releases) 提供 Windows 安装包、便携版与 macOS `.dmg`：

| 平台 | 文件 |
|---|---|
| Windows x64 | `GlobalSpeedTest_*_x64-setup.exe`（NSIS）/ `GlobalSpeedTest_*-portable.exe` |
| macOS arm64 | `GlobalSpeedTest_*_aarch64.dmg`（ad-hoc 签名，首次需右键打开） |

---

## 说明

- 本项目用于互操作性与学习研究：把已在 Android 上公开提供的测速服务，做成桌面端更好用的客户端。
- 节点与接口均来自官方 App 的公开协议；节点表随官方下发更新。
- 未复刻：网页/视频/游戏体验测速（原版由服务端 VSP 脚本驱动）、结果上报接口。
