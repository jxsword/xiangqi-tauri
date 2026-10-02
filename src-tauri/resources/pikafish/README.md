# 皮卡鱼引擎资源目录

此目录用于存放皮卡鱼（Pikafish）官方发布版的二进制与 NNUE 权重文件，**二进制不纳入 git 仓库**（体积大、各平台不同），由脚本 `scripts/fetch-pikafish.sh` 获取或手动从官方发布页下载。

预期布局：

```
resources/pikafish/
├── README.md
├── windows-x64/
│   ├── pikafish.exe
│   └── pikafish.nnue
├── linux-x64/
│   ├── pikafish
│   └── pikafish.nnue
├── macos-x64/  (macos-arm64 可选)
│   ├── pikafish
│   └── pikafish.nnue
```

- 官方站点：https://www.pikafish.com / GitHub: https://github.com/official-pikafish/Pikafish
- 许可证：GPLv3（详见 `docs/THIRD_PARTY_LICENSES.md`，UI"关于"页亦有声明）
- 引擎仅以子进程 UCI 协议调用，不修改其源码
