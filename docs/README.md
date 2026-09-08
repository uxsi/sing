# sing 客户端 — 方案文档索引

本目录存放产品与技术方案，**实施前以本文档集为准**。

| 文档 | 说明 |
|------|------|
| [01-product-overview.md](./01-product-overview.md) | 产品定位、与 sing-box / Clash / SFM 的差异 |
| [02-architecture.md](./02-architecture.md) | 技术架构、模块划分、跨平台策略 |
| [03-features-and-modes.md](./03-features-and-modes.md) | 功能范围（对齐 Clash）与办公/出国模式 |
| [04-roadmap.md](./04-roadmap.md) | 分期实施与验收标准 |

相关背景：本机公司准入隧道（含公司 VPN / 公司 DNS 代理）与 SFM/sing-box 双 TUN 叠路由会导致 GitHub SSH/`git push` 间歇失败；办公模式即针对该场景设计。
