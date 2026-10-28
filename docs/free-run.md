# Windows 0.4.1 自由跑扩展

## 来源

主体完整基于 DemaciaForge/NekoSportsWorldTool 的 v0.4.1 标签，基线提交
989a8579ca88dbba11866a71cdc9793af1cba3bb。这是社区 Fork 的非官方 0.4 系列。
保留该版本的定位、地图选点、自定义高德路线、操场规格、路线颜色、海拔等功能。
在此基础上迁移 crusiLi 的自由跑扩展。路线统计和异步预览参考 GAN1112233131
Fork 的 stats.rs / preview_jobs.rs / school_track.rs（555c059e），遵循项目 LICENSE。

## 使用

解压后运行 nekosportsworldtool.exe。跑步页顶部选择“自由跑”。在设置中确认当前位置
为 BD-09 坐标。需要沿道路时，先在“路网”页导入覆盖该位置的 .osm / OSM XML 文件，
然后选择“沿真实道路”，等待后台预览完成，检查绿色起点和红色终点，再开始自由跑。
“换一版”会换路线种子。“重新规划道路路线”会重新检查当前输入。
自由跑不读取学校必经点或围栏。校园跑保留 0.4.1 的原有功能。

## 路线行为

先沿真实有向道路生成局部闭环，再按目标距离绕圈和走末圈。保留路口顶点，
不跨断开的路段、不使用 0.4 自动补出的连接线、不强行把最后一点拉回起点。
缺少路网、距路超过 150 米、单行路无法返回等情况会报错，停止提交。
预览和提交复用同一份规划结果；位置、路网、距离或种子变化后必须重新预览。
自由环线是本地几何圆环，不代表真实道路。GPX 转成的 OSM 仅包含该轨迹，不包含周边道路。

## 构建与本地校验

GitHub Actions → Windows Free Run Build → Run workflow。
Windows PowerShell 也可执行 ./windows/build.ps1（先安装 Rust MSVC 与 C++ Build Tools）。
脚本先运行全部测试，再构建并打包 dist/NekoSportsWorldTool-0.4.1-free-run-win-x64.zip。
包只含 exe、LICENSE、本文档，不含配置、会话或用户轨迹。

CLI 离线验证示例（不会提交）：
nekosportsworldtool.exe run --mode free --dry-run --route legacy --dist 2 --pace 400 --lat 39.9 --lon 116.4
--route road 使用配置中的 OSM 文件。--mode 默认为 campus，旧配置默认也为校园跑。

## 验证边界

自由跑沿用旧协议 sportType=4，策略也使用 runMode=4；body/OBS 不携带必经点。
服务端是否仍接受该类型，需要真实账号确认。收到提交响应、OBS 上传完成、
详情获取、类型匹配和达标是不同结果，界面分别显示；构建通过不代表学校认定有效。
