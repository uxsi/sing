# 技术架构

## 1. 总体结构

```
┌─────────────────────────────────────────┐
│                 App Shell                 │
│  UI (SFM-like) · 配置管理 · 模式预设       │
│  双 TUN / DNS 健康检测 · 日志视图          │
└─────────────────┬───────────────────────┘
                  │ 本地 API / 进程管控
┌─────────────────▼───────────────────────┐
│            Controller Service             │
│  启停 core · 热重载 · clash_api 适配       │
│  办公/出国模式 → 配置补丁生成               │
└─────────────────┬───────────────────────┘
                  │
┌─────────────────▼───────────────────────┐
│              sing-box core                │
│  TUN / mixed / DNS / route / outbound     │
└─────────────────────────────────────────┘
```

- **UI 与 core 解耦**：UI 不实现转发；一律通过配置 + `clash_api`（或等价本地控制口）操控。
- **模式 = 配置补丁**：办公/出国模式是对用户基线 JSON 的可逆补丁（规则插入、`strict_route`、DNS、默认出站等），可一键卸下。

## 2. 推荐技术选型（桌面 MVP）

| 层 | 建议 | 理由 |
|----|------|------|
| UI | Tauri 2 + Web 前端，或 Flutter Desktop | 跨 macOS/Windows（+ 后续 Linux）；易做 SFM 风格面板 |
| Core | 官方 `sing-box` 二进制 / 库，随安装包分发 | 协议与路由能力无需自研 |
| 控制面 | sing-box `experimental.clash_api` | 与 Clash 面板心智一致，便于模式切换与测速 |
| 配置 | sing-box JSON；支持从文件/URL 拉取 | 「配置复刻 sing-box」 |
| macOS 权限 | System Extension / Network Extension（对齐 SFM 路径） | TUN 必需 |
| Windows | WinTUN + 服务进程 | TUN 必需 |

> 实施阶段可再定 UI 框架；架构约束是：**core 可替换版本号，UI 不绑死某一发行版私有协议**。

## 3. 模块划分

1. **ConfigStore**：本地配置列表、订阅更新、校验、差分补丁。
2. **CoreSupervisor**：进程生命周期、崩溃拉起、版本检查。
3. **ModeEngine**：`office` / `abroad` / `manual` 三态；生成与回滚补丁。
4. **HealthProbe**：检测多 `utun`、默认路由归属、DNS 是否返回异常段（如公司污染的 `100.12.0.0/8` 一类）、关键域名连通性。
5. **PanelAPI**：封装 connections / proxies / configs，供 UI。
6. **PlatformVpn**：各 OS 的 TUN 安装与权限引导（独立模块，避免 UI 掺 OS 细节）。

## 4. 跨平台策略

| 阶段 | 平台 | 说明 |
|------|------|------|
| MVP | macOS（优先，贴合现网 iOA+SFM 场景） | 先打通 TUN + 模式引擎 + 面板 |
| MVP+ | Windows | 同一 UI，PlatformVpn 换实现 |
| 后期 | Linux desktop | 可选 |
| 后置 | iOS / Android | 权限与上架成本高，不挡桌面主线 |

## 5. 安全与隐私

- 配置中的节点密钥仅存本地；日志默认脱敏（密码、token）。
- 健康检测只读系统路由/DNS/进程列表，不做对抗式注入。
- 不收集用户流量内容；如需可选崩溃统计须明示。

## 6. 仓库布局（建议，实施时落地）

```
sing/
  docs/           # 本方案
  apps/desktop/   # UI 壳
  crates/ or packages/controller/  # 控制面
  third_party/sing-box/  # 版本钉扎说明（不强制 vendoring）
  configs/examples/      # 示例配置与模式补丁样例
```
