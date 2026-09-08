# 分期路线与验收

## Phase 0 — 方案与仓库（当前）

- [x] 方案落入 `docs/`
- [ ] 初始化应用仓库结构与许可证/版本钉扎（sing-box 版本号）
- [ ] 示例配置 + 办公/出国补丁样例 JSON

**验收**：文档评审通过；目录可开工。

## Phase 1 — macOS 桌面 MVP

- 导入配置、启停 core、Rule/Global/Direct、节点选择、日志、mixed 端口
- TUN 连通（权限引导）
- ModeEngine：办公 / 出国 / 手动
- HealthProbe：双 TUN、脏 DNS 告警 + 办公保护补丁

**验收**：

1. 无 iOA 时：出国模式可稳定访问常用被墙站点。
2. 有 iOA + 办公模式：`git ls-remote` / `git push`（SSH `ssh.github.com:443`）连续成功；内网 OA 可用；指定代理域名出国。
3. 强制经本地 SOCKS 用**污染 IP**访问 GitHub 应仍失败（说明问题在路径），但 UI 已引导用户走直连保护或出国模式干净 DNS——行为与文档一致。

## Phase 2 — 体验与订阅

- URL 订阅更新、配置校验与错误定位
- 延迟测试、流量/连接列表（clash_api connections）
- Windows 适配启动

**验收**：Windows 上 Rule + 节点切换可用；订阅可更新。

## Phase 3 — 加固

- 模式补丁单元测试、探针规则可配置化
- 自动更新 core、崩溃还原
- （可选）Linux

## 明确后置

- iOS / Android 完整客户端
- 可视化规则编辑器
- 对抗公司安全软件或隐藏流量

## 实施原则

1. 先控产品边界，再刷 UI 细节。
2. 模式用补丁，不改用户原文件（或旁路保存，可一键还原）。
3. 每一期结束都在「纯净网」与「iOA 共存」两套环境做回归（若可用）。
