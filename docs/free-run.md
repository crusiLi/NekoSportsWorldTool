# Windows 自由跑增量版

2026-10-20 已通过 GitHub Actions 的 Windows x64 构建，工作区 105 项测试通过，发布版 EXE 和应用 ZIP 已生成。Windows 窗口操作与真实业务提交尚未验收。

源码位于 [crusiLi/NekoSportsWorldTool 的 free-run 分支](https://github.com/crusiLi/NekoSportsWorldTool/tree/free-run)，基线是上游的 `69856c91ce54fcb409f995e8300220d7bef5d917`，未合入之后的上游修改。

[成功构建与下载入口](https://github.com/crusiLi/NekoSportsWorldTool/actions/runs/39392097117)。

## 使用流程

1. 启动后在设备页设置跑步所在城市和位置（沿用原工程的 BD-09 坐标约定）。
2. 跑步页选择“自由跑”，设置距离和配速，查看自由环线预览。
3. 登录后点击“开始自由跑”。该分支不请求学校必经点，不读取校园跑的围栏或点位缓存，也不将本地路线的几何控制点上报为必经打卡点。
4. 查看结果中的记录号、OBS 上传数量、详情查询、服务端达标状态与模式匹配情况。缺少结果时显示“尚未核验”；取得记录号不代表已计入学校考核。

本版自由跑使用围绕配置位置生成的本地环线，不包含地图手绘、沿道路规划或避开建筑物功能。校园跑保留原有经典/路网模式。旧配置默认校园跑，GUI 会保存所选模式；CLI 不带模式参数时仍为校园跑。

## Windows 本地构建

需要 64 位 Windows、Rust MSVC 工具链，以及 Visual Studio Build Tools 的“使用 C++ 的桌面开发”组件和 Windows SDK。

在项目根目录打开 PowerShell：

```powershell
rustup target add x86_64-pc-windows-msvc
./windows/build.ps1
```

脚本先运行工作区测试，再编译 GUI，最后生成：

```text
dist/NekoSportsWorldTool-free-run-win-x64.zip
```

解压后运行 `nekosportsworldtool.exe`。PowerShell 若限制脚本执行，也可以在已配置开发环境的终端手动执行：

```powershell
cargo test --workspace --all-targets --locked --target x86_64-pc-windows-msvc
cargo build --release --locked --target x86_64-pc-windows-msvc
```

手动构建的 EXE 在 `target/x86_64-pc-windows-msvc/release/`。

## GitHub Actions 构建

打开仓库的 Actions，选择 **Windows Free Run Build → Run workflow → Run workflow**。工作流入口在默认分支 main，实际 checkout 固定为 free-run 分支。成功后下载同名构建产物并解压；产物里是应用 ZIP，再次解压后运行 EXE。

如果将源码复制到另一个仓库，需要保留 free-run 分支，或将工作流中的 checkout ref 改为实际源码分支。

该入口只测试和生成构建产物，不发布 Release，不使用账号，不提交运动记录。已创建用户个人 Fork、上传源码，并手动触发上述构建。

## 协议与验证边界

- 自由跑暂按 Inverx 7.3.40 参考实现及 Neko Issue #8 的 `sportType=4` 线索实现；规则查询也传递所选模式。类型 6 属于另一个参考版本，不混入这条路径。目标学校是否接受类型 4、是否接受空点位表示，以及规则接口的模式映射，均没有在本次得到服务端验证。
- 提交签名包含所选运动类型；自由跑的提交点位字段与 OBS 点位字段均为空数组。校园跑的非空点位校验仍保留。
- 新增测试覆盖签名与模式的一致性、自由跑空点位、校园跑校验保留、OBS 解压内容、位置有效性、配置兼容及预览状态。新增测试已随本次 Windows 工作区测试通过；合计 105 项测试通过，0 项失败。
- 本机没有安装 Rust；编译与测试在 GitHub 的 Windows runner 上完成。ZIP 完整性及 EXE 的 Windows x64 PE 格式已检查。未在真实 Windows 桌面打开应用，也没有登录运动账号或提交运动记录。
- 本地 `git diff --check` 与静态检查不替代编译和真机测试。

Neko 的上游自动更新不会包含这份本地修改；测试此版本时可在“关于”中关闭启动更新检查，避免换回上游 EXE 后丢失自由跑选项。

## 待验收

- Windows 构建脚本和 105 项测试已通过；仍需打开窗口验收两种模式切换。
- 自由跑不需要点位缓存或 OSM 文件；改变设备位置后预览更新；重启后恢复所选模式。
- 校园跑的原有点位约束和路线选项仍生效。
- 真实业务另行核对记录类别、轨迹、详情状态；本次没有进行账号登录或在线提交。

本工程保留上游 CC BY-NC 4.0 许可证及署名。
