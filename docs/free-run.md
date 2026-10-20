# Windows 自由跑增量版

本次交付为源码与构建入口，不含已验证的 EXE。基线是上游 main 的 `69856c91ce54fcb409f995e8300220d7bef5d917`。

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

将这份源码放入你有写权限的 GitHub 仓库，在 Actions 中选择 **Windows Free Run Build**，手动运行。成功后下载同名构建产物并解压；产物里是应用 ZIP。

该入口只测试和生成构建产物，不发布 Release，不使用账号，不提交运动记录。本次没有替你创建仓库、推送源码或触发 Actions。

## 协议与验证边界

- 自由跑暂按 Inverx 7.3.40 参考实现及 Neko Issue #8 的 `sportType=4` 线索实现；规则查询也传递所选模式。类型 6 属于另一个参考版本，不混入这条路径。目标学校是否接受类型 4、是否接受空点位表示，以及规则接口的模式映射，均没有在本次得到服务端验证。
- 提交签名包含所选运动类型；自由跑的提交点位字段与 OBS 点位字段均为空数组。校园跑的非空点位校验仍保留。
- 新增测试覆盖签名与模式的一致性、自由跑空点位、校园跑校验保留、OBS 解压内容、位置有效性、配置兼容及预览状态。测试已编写，但本次未运行。
- 本机没有 Rust 工具链。官方工具链下载两次因自动审批模型容量不足而未获执行；用户选择交付源码与 Windows 构建入口。因此没有声称编译通过、Windows UI 验收通过或业务提交有效。
- 本地 `git diff --check` 与静态检查不替代编译和真机测试。

Neko 的上游自动更新不会包含这份本地修改；测试此版本时可在“关于”中关闭启动更新检查，避免换回上游 EXE 后丢失自由跑选项。

## 待验收

- Windows 构建脚本和所有测试通过，打开窗口后能切换两种模式。
- 自由跑不需要点位缓存或 OSM 文件；改变设备位置后预览更新；重启后恢复所选模式。
- 校园跑的原有点位约束和路线选项仍生效。
- 真实业务另行核对记录类别、轨迹、详情状态；本次没有进行账号登录或在线提交。

本工程保留上游 CC BY-NC 4.0 许可证及署名。
