# 分期路线与验收

> 状态以 **main 已合并** 为准。挂起未合的 PR 不勾选。

## Phase 0 — 方案与仓库

- [x] 方案落入 `docs/`
- [x] 初始化应用仓库结构（monorepo：`apps/desktop`、`packages/controller`、`configs/examples`）
- [x] 许可证/版本钉扎（sing-box 版本号）— `third_party/sing-box/VERSION` + `scripts/fetch-sing-box.sh`
- [x] 示例配置 + 办公/出国补丁样例 JSON（`configs/examples/`）

**验收**：文档评审通过；目录可开工。

## Phase 1 — macOS 桌面 MVP

> 桌面壳 + 控制面桥见 `docs/05-macos-tauri.md`（Tauri 2 脚手架 + Node bridge）。

- [x] ModeEngine：办公 / 出国 / 手动（`packages/controller` patch + merge + CLI）
- [x] HealthProbe：双 TUN、脏 DNS 告警分类（fixture + CLI）
- [x] 桌面 SFM-like 壳：模式切换、状态、日志 stub（`apps/desktop`）
- [x] 启停 core 桌面桥（Tauri Rust + Node packages/bridge；UI Connect / Status）
- [x] mixed 端口实机联动 — macOS **System Proxy** On/Off → `127.0.0.1:1080`；Disconnect/Off 恢复
- [x] 内网域名保护 — System Proxy bypass + Office 直连（`*.woa.com` / `woa.com` / `*.oa.com` / `oa.com`）
- [x] macOS dataplane 默认 **mixed-only**（无 Network Extension 前不白试 utun；`SING_TRY_TUN=1` 可试 TUN）
- [x] Abroad 活节点 — Connect 后 delay 探测并选第一个活节点，`final→proxy`（Office 仍 `final→direct`）
- [ ] 导入配置、Rule/Global/Direct（仍待）
- [ ] 节点选择 UI（Proxies 成员一键 Select）— PR #19 挂起未合
- [ ] TUN 完整连通（Network Extension / 权限引导）— **后置**；当前用 System Proxy + mixed-only
- [ ] 按钮即时反馈 / 一键办公（Connect+System Proxy、loading、重活后台）— 待用户点头

**验收**：

1. 无公司隧道时：出国模式可稳定访问常用被墙站点（依赖活节点）。
2. 有公司隧道 + 办公模式：`git ls-remote` / `git push`（SSH `ssh.github.com:443`）连续成功；内网 OA 可用；指定代理域名出国。
3. 强制经本地 SOCKS 用**污染 IP**访问 GitHub 应仍失败（说明问题在路径），但 UI 已引导用户走直连保护或出国模式干净 DNS——行为与文档一致。

## Phase 2 — 订阅与面板（进行中；macOS 优先）

- [x] URL 订阅更新（`subscribe`：fetch + 落盘 + CLI；桌面 Fetch 预览）
- [x] 配置校验与错误定位（`validate`：结构检查 + CLI + UI）
- [x] 延迟测试、连接列表（`clashApi`：proxies / connections / delay + CLI + UI）
- [x] 桌面 ↔ core 桥：Tauri invoke + Node HTTP bridge（:8787）；clash_api 经桥避免 CORS
- [x] macOS 实机联调（Connect / Status / System Proxy / proxies·delay / Office+woa+Google）— 已多次本机验证；正式「签字」仍可再走一轮回归
- [ ] Windows 适配启动 — **已后置**（先完成 macOS 端到端）

**验收（macOS）**：订阅可更新；校验与 clash_api 面板可用。Windows 验收延后。

## Phase 3 — 加固（进行中）

- [x] 探针规则可配置化（`loadHealthRules` + dirty CIDRs / GitHub hints / 公司隧道 iface 名模式；示例 `configs/examples/health-rules.json`）
- [x] Core supervisor：检测 sing-box、`core start|stop|status`、崩溃重启（max restarts + backoff）；缺二进制 soft-fail
- [x] 模式补丁单元测试（既有 `patch.test.ts`）
- [x] 捆绑 sing-box sidecar（`externalBin` + fetch 脚本，桌面优先用自带二进制）
- [ ] 自动更新 core（下载/校验）— 未做
- [ ] （可选）Linux
- [ ] Windows 文档与服务骨架 — **已后置**

## 明确后置

- Windows 适配与 WinTUN / 服务进程说明
- iOS / Android 完整客户端
- 可视化规则编辑器
- 对抗公司安全软件或隐藏流量
- macOS Network Extension 真 TUN

## 实施原则

1. 先控产品边界，再刷 UI 细节；**macOS 端到端优先**。
2. 模式用补丁，不改用户原文件（或旁路保存，可一键还原）。
3. 每一期结束都在「纯净网」与「公司隧道共存」两套环境做回归（若可用）。
4. 代码 / 文档 / UI 不出现具体公司产品名；仅用「公司 / 内网 / 公司隧道」等泛称。
5. 改动路径：本地改 → 先验再 PR；远端改 → 先 PR 再本地验。
