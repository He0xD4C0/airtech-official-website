# 公开站 V27 实施与验收记录

日期：2026-09-20。分支：`codex/public-site-frontend-v27`。
保留 `6a18d6d`、`eb2aedd`、`911ef21`，未推送、未部署远端。

## 结论

| 层次 | 结果 |
| --- | --- |
| 程序交付 | 串行检查、临时 PostgreSQL 契约测试、production-image 浏览器测试通过 |
| 隔离测试站就绪门禁 | 通过全部 8 组检查，无警告 |
| 统一业务库就绪门禁 | 失败，原因是正式站点壳层与核心页面尚未发布 |

不能将本次程序交付描述为业务公开站已恢复。业务阻断共 1 项：Critical / Content，正式 CMS 内容未准备完毕。未配置自定义图标按要求仅为警告。

## 实施范围

- 三 origin 统一为 `*.airtek.localhost:8088`，旧公开入口保留路径和查询做 308 跳转。没有降低 Secure、HttpOnly 或 SameSite=Strict Cookie 属性。
- `/healthz` 与 CMS 就绪分离；主库自动公共种子关闭；生产 API、Worker、Maintenance 拒绝开发种子配置。
- 产品列表、总数、排除自身筛选的分面在服务层共享只读一致性快照；公共搜索在数据库分页，按 CMS 类型展示，并修复 `/en` 游标。
- 目录 URL/历史恢复修复真实浏览器 `DataCloneError`；选型统一数字校验，缺乏结构化证据返回工程审核；Cookie 与比较栏共用底部布局。
- 四类 RFQ 完整传递上下文，Admin PII 权限、读取审计与禁缓存保持不变；错误返回对应步骤并聚焦，浏览器存储不可用不阻断提交。
- 修复到期清除尝试写入非法数据库状态的问题：数据库状态使用已有 `closed`，已清除标记保留在 JSON，并向 Admin 显示不可读取状态，无新迁移。
- 修复飞书未测试连接时的 NULL 布尔解码错误，不改变其 GUI/数据库配置架构。
- 图标、manifest、SEO 和部署就绪检查闭环；就绪检查只读，部署失败仅恢复应用镜像，不回滚数据库。

## 已运行检查

依次通过 `pnpm check:contracts`、`pnpm typecheck`、`pnpm lint`、`pnpm test:all`、`pnpm check:deployment`、`pnpm check:production`、`pnpm check:architecture`、`pnpm test:postgres`、`pnpm check:source-lines`，随后完整 `pnpm test:e2e:stack` 通过。最后再次通过 E2E 类型、行数与 diff 空白检查。

- Web：124 个测试通过；Admin：131 个测试通过；Rust library：191 个测试通过。
- 临时 PostgreSQL：35 个契约测试通过，使用独立容器/数据库，不使用业务库临时 schema。需要独立 S3 配置的 Rust 分支未在该运行器连接 S3；真实媒体上传、预览和原始下载另由 MinIO E2E 验证。
- 浏览器：41 个测试通过，包括真实表单登录、刷新会话、CSRF、登出、全部 15 个核心路由、四类 RFQ → Admin 上下文回读、目录历史、比较分享、空投影 503、图标上传/选择/清除/尺寸阻断、390/768/1280 视口、键盘与 axe。
- 受控曲线匹配/不匹配由测试数据验证；真实 711 条产品没有结构化曲线，不宣称已验证匹配。
- 隔离 production-image 门禁结果：`originContract`、`publishedDataClasses`、`siteShell`、`coreAndShellRoutes`、`cors`、`robotsAndSitemaps`、`siteIconAndManifest`、`browserBundleOrigins` 全部通过。
- 所有本轮创建的测试数据库容器、Compose 网络及数据卷已清理。既有业务库、历史验证库/schema、旧 devseed 栈未删除。

## 业务库实操结果与阻断

本地 `airtekpower` 应用容器已更新为 production feature 构建；数据库及全部加密密钥与更新前相同，PostgreSQL 和 MinIO 未重建。

1. 浏览器打开 `http://www.localhost:8088`，到达 `http://www.airtek.localhost:8088/en`。
2. 页面明确显示暂时不可用；点击 Try again 留在当前路径，无首页循环跳转。截图已附在本任务对话中。
3. Admin 新 origin 可独立到达登录页，未绕过 TOTP。两个页面读取的浏览器 console error 均为空。

| 实际探针 | 结果 |
| --- | --- |
| `/healthz` | 200 |
| `/en` 和未知公开路径 | 503，壳层故障优先 |
| `/robots.txt` | 200，`Disallow: /`，不宣告 sitemap |
| `/sitemap.xml` | 503 |
| `/site-icon`、`/site.webmanifest` | 200，中性 SVG/动态 manifest |
| 浏览器 origin 的 RFQ POST 预检 | 200，准确允许 origin 和请求头 |
| 公开产品 API | 200，total=711 |
| 真实产品选型 API | 200，`engineeringReviewRequired`，无候选 |
| 只读就绪命令 | exit 1，bootstrap 503 |

[主库只读盘点](inventory.json)记录了缺失实体和已有草稿字段问题：

- General Information：`/content/drafts/65b2f465-1a69-496b-8132-a804039d3ff4`，结构校验未发现额外字段错误，但仍为占位草稿，必须审核。
- Navigation：`/content/drafts/3ab1b318-bd50-4a2b-bf0b-12fd4aea06de`，同上。
- Footer 与 15 个核心页面不存在，未擅自创建或发布。现有文章与两个草稿保留。
- 本轮没有向业务库写入测试 RFQ 或合成产品；只读核对仍为 711 产品、0 RFQ、3 CMS 实体、1 已发布 CMS 记录。

继续准备缺失草稿需要用户完成 Admin 登录。已提供 `scripts/prepare-public-drafts.mjs`，仅用认证 CMS 创建接口创建缺失草稿，不覆盖旧草稿、不审批、不发布。正式公司/隐私/条款内容仍需审核；导航与页脚发布前需核验其目标。

## 与原计划的一处契约差异

核对最新 V27 源码与数据库测试后确认：媒体预览兼容入口是 308；原始下载入口已是 200 流式响应，包含下载文件名和原始字节。本轮保留 V27 已有行为，未回退下载实现。已向用户提出确认问题。图标端点自身仍为 308 到不可变公开原图。

## 未完成的业务验收

正式内容未审核发布，不能在主库上验收正常首页、导航与 15 条页面旅程；未获得主库已认证会话，因此缺失草稿创建仍待执行。远端 HTTPS、生产主机和自动部署未配置、未运行。

操作说明见 `infra/deploy/public-site-recovery.md`。本次 dogfood 实操仅验证主库入口/故障语义/Admin 可达性；完整内容旅程在隔离生产镜像栈验证，不以占位内容替代业务内容审批。
