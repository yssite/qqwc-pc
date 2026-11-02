# 逆向工具脚本

分析 `全球网测 v4.4.8` 时用的自研脚本，均可独立运行。

| 文件 | 作用 |
|---|---|
| `decrypt_serverlist.py` | 拉取配置清单 → 下载加密节点表 → DES/ECB/PKCS5 解密（密钥 `dw!@#$%^`）→ 导出 JSON/CSV，可选做 TCP 可达性探测 |
| `protocol_probe.py` | 测速协议最小实现：生成 token → 建立会话 → 下行 → 上行 → 结束会话，可直接对任意节点跑通并打印速率 |
| `dexmem.c` | Android 进程内存 dump 器（arm64，NDK 交叉编译）。扫描 `/proc/<pid>/maps` + `/proc/<pid>/mem`，按 `dex\n03x` 魔数与合法 `file_size`/`endian_tag` 落盘，用于 360 加固脱壳 |
| `elf_vaddr.py` | 极简 ELF 读取器：vaddr → 文件偏移，可直接解出 `.so` 里的 `JNINativeMethod` 注册表 |

## 用法示例

```bash
# 1) 节点表
pip install pycryptodome
python decrypt_serverlist.py --check

# 2) 协议验证（任意节点）
python protocol_probe.py --host 182.151.203.2 --port 65499 --secs 5

# 3) 脱壳（需要 Android NDK + adb root）
aarch64-linux-android24-clang -O2 -o dexmem dexmem.c      # 注意：不能 -static
adb push dexmem /data/local/tmp/ && adb shell "chmod 755 /data/local/tmp/dexmem"
adb shell "/data/local/tmp/dexmem $(adb shell pidof com.cnspeedtest.globalspeed) /data/local/tmp/dexdump"

# 4) 读取 .so 的 JNI 注册表（Ghidra 里镜像基址通常偏移 0x100000）
python elf_vaddr.py libGSCore.so 0x10d040 11
```

## 说明

- `dexmem.c` 必须是**动态链接**的 arm64 可执行文件；在其 ARM 翻译层下，静态链接版本会段错误。
- 密钥、token 算法等细节见 [`docs/02-协议规格.md`](../../docs/02-协议规格.md)。
