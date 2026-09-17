# Skill trigger matrix

Last reviewed: 2026-09-14

Descriptions in each `SKILL.md` are the primary discovery surface. Root `AGENTS.md` reinforces routing. Load the smallest sufficient set and add another Skill only when the task actually crosses its domain.

## Primary routing cases

| Example intent | Expected Skill | Must not substitute |
|---|---|---|
| “检查 About 页公司名、邮箱、口号和品牌色” | `$airtek-brand` | Product or website strategy alone |
| “整理离心/轴流/横流风机分类与 EC 电机字段” | `$airtek-product-knowledge` | Brand or generic website Skill |
| “设计站点 sitemap、SEO 内链和 RFQ 漏斗” | `$airtek-website-growth` | Product knowledge alone |
| “阅读新 PDF，纠正旧知识并更新项目 Skill” | `$airtek-knowledge-governance` plus the affected domain Skill | Generic system `skill-creator` alone |
| “香港还是新加坡部署，采购表能否当预算” | `$airtek-website-growth` + `$airtek-knowledge-governance` | Brand or product Skill |

## Combined cases

| Example intent | Expected combination |
|---|---|
| Product detail page implementation | `$airtek-product-knowledge` + `$airtek-website-growth`; add `$airtek-brand` for public copy/visuals |
| Fan Selector | `$airtek-product-knowledge` + `$airtek-website-growth`; add `$airtek-brand` if designing the branded UI |
| Technical article or FAQ | `$airtek-product-knowledge` + `$airtek-website-growth` + `$airtek-brand` when publication-ready |
| Solution landing page | `$airtek-product-knowledge` + `$airtek-website-growth` + `$airtek-brand` |
| RFQ implementation with product prefill | `$airtek-website-growth` + `$airtek-product-knowledge` |
| Resolve the two values for model `B23E280H128-102-B0` | `$airtek-product-knowledge` + `$airtek-knowledge-governance` |
| Import a new approved brand manual | `$airtek-knowledge-governance` + `$airtek-brand` |

## Negative cases

No AIRTEK project Skill should load for a generic refactor, dependency upgrade, test-run request, Git operation, or generic Skill creation when the task does not require AIRTEK-specific knowledge. A simple CSS syntax fix does not need `$airtek-brand`; choosing colors or typography does. A generic form component does not need `$airtek-website-growth`; changing AIRTEK RFQ fields or analytics does.

## Regression prompts

Use these prompts after changing Skill descriptions:

1. Positive brand: `请把 About 页品牌名、公司英文名、颜色和邮箱改为 AIRTEK 的正确规范。`
2. Positive product: `为风机选型器设计产品字段和硬约束，不要采用 Demo 的型号参数。`
3. Positive website: `规划 AIRTEK 的 SEO、RFQ、埋点和数据 API。`
4. Positive governance: `阅读新增资料，判断冲突来源并固化到项目级 Skill。`
5. Combined: `实现带 PQ 曲线、下载、相关内容和询价预填的产品详情页。`
6. Conflict: `B23E280H128-102-B0 应该显示哪组风量、压力和功率？`
7. Negative: `升级测试框架并修复与 AIRTEK 业务无关的 lint 错误。`
8. Negative: `为另一个仓库创建通用 Skill。`

Expected safety outcome for prompt 6: identify both demo value sets as `DEPRECATED`, choose neither, and read exact values only from a published record or controlled snapshot matching the registered Product Master checksum and mapping. If that source is unavailable, return `Published data unavailable`; do not ask the user to choose between the demos.
