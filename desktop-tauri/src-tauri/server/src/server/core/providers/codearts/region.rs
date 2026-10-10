//! CodeArts 的**区域**（国内版 / 国际版）：两套区域端点与两条 provider 身份。
//!
//! ── 这一家是什么 ────────────────────────────────────────────
//! CodeArts（华为云 AI 代码助手 / CodeArts Doer / snap-access）的官方扩展打的
//! 是**区域网关** `https://snap-access.<region>.myhuaweicloud.com`（参考实现
//! `cpa-codearts-plugin` 的 `base_url` 注释原文），网页登录门户则是 CodeArts
//! 自己的控制台域名。**数据面（snap-access）与门户按区域分区**：国内 `cn-north-4`
//! 签发的 AK/SK/STS 拿到国际站上必然 401，反之亦然 —— 所以区域不是「一个可选
//! 参数」，而是身份的一部分。
//!
//! 但**授权面（STS 令牌 / 身份端点）不分区**：官方扩展 `product.json` 里，
//! 国际版（`HKFramework.productDomain`）的 `iamStsOpenDomain` 仍是
//! `https://sts.cn-north-4.myhuaweicloud.com` —— 与国内版逐字相同。因此两地的
//! `token_url()` / `identity_url()` 是**同一个**端点，只有 `base_url()` /
//! `web_login_base()` 分区域。
//!
//! ```text
//!              区域网关（snap-access）                              STS（sts，两地同一台）                      网页登录门户
//!   国内版  https://snap-access.cn-north-4.myhuaweicloud.com   https://sts.cn-north-4.myhuaweicloud.com   https://codearts.huaweicloud.com
//!   国际版  https://snap-access.ap-southeast-1.myhuaweicloud.com  https://sts.cn-north-4.myhuaweicloud.com   https://codearts.ap-southeast-1.huaweicloud.com
//! ```
//!
//! ── 国际版区域码是 `ap-southeast-1`（依据）────────────────────
//! 官方扩展 `product.json` 的 `HKFramework.productDomain`：`snapApigDomain` =
//! `https://snap-access.ap-southeast-1.myhuaweicloud.com`、`codeartsWebNewUrl` =
//! `https://codearts.ap-southeast-1.huaweicloud.com`（实测分别回 `APIG.0301`
//! 未鉴权、200 门户页）。**区域码不是 `ap-southeast-3`**（那个 snap-access 无响应）。
//!
//! ── 为什么 STS 不能跟着区域化（踩过的坑）────────────────────
//! 曾把国际版 STS 也写成 `sts.ap-southeast-1` / `sts.ap-southeast-3`，换码必失败：
//! 这些主机上 `/v1/oauth2/tokens` 返回
//! `STS5.1007 this API is not supported in this region`。逐一探测 26 个区域后确认
//! 该 API **只在 `cn-north-4` 与 `cn-south-1` 上线**，国际区域一律 1007 —— 与
//! `product.json` 里 `iamStsOpenDomain` 不分区完全吻合。
//!
//! ── 福利网关：目录两地共用，但**国际版的福利模型要打中国网关**──
//! 官方扩展的福利后端（bundle 里的 `sue()`）对国内 / 国际返回同一组中国主机
//! （`opengw.developer.huaweicloud.com`；灰度走 `snapengine-apig.cn-north-7`），
//! 只用 `isGammaVersion` 区分 —— **目录**（`/api/v1/gateway/config`）两地都能取到，
//! 同样的 4 个福利模型（`glm-5.3-flash` / `deepseek-v4-*`）。
//!
//! 但**目录可调 ≠ 模型可调**（2026-10 实测）：
//!   * 国际区域上的 `/v1/benefit-gateway-config` 回的不是国内那种 `{"enabled":...}`
//!     开关，而是 **`{"vendors":[...]}`**（第三方模型目录）；
//!   * 照"拿到配置即启用"继续取目录能拿到 4 个福利模型，但**这些模型注册在
//!     中国的 InferHub**：拿国际数据面 `snap-access.ap-southeast-1` 调它们一律
//!     `InferHub.002002009.404 The model is not registered`。
//!
//! 所以国际版的福利模型**单独改打中国的区域网关**（`snap-access.cn-north-4`）——
//! 依据是国际账号的凭据本就是 cn-north-4 的 STS 签发的，同一套签名大概率被中国
//! 网关接受。路由在 [`Region::benefit_chat_base`]，**这是一条实测中的路由**：
//! 若中国网关拒了国际凭据，就把国际版福利源整体关掉（`default_benefit_gateway_url`
//! 回 `None`），别留一调就 404 的幽灵模型。
//!
//! ── 为什么是两个 provider 而不是「一家的一个字段」──────────
//! 与 Cline 的两个额度池、AutoClaw / Accio / ZCode 的两个地区同一思路：做成
//! 「一个 provider 上的 `region` 字段」会让区域变成**账号的属性**，界面上混在
//! 一起、「哪个账号走哪个站点」看不出来，账号记录也无法按区域隔离。按两个
//! provider 建模之后各自有独立的账号、清单、启停与映射。
//!
//! ── 硬约束 ──────────────────────────────────────────────────
//! release 是 `panic=abort`：本文件零 unwrap/expect/panic。

use crate::server::core::providers::{kind_id, ProviderKind};

/// CodeArts 的区域。
///
/// 顺序 = 注册表顺序（国内版在前）：`ALL` 的遍历顺序决定模型目录合并时同名模型
/// 先归谁家、以及界面上两家的先后。国内版在前是因为国内网络环境下它是更常被
/// 添加的那一个（与 AutoClaw / ZCode 的排序理由一致）。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Region {
    /// 国内版（`cn-north-4`；provider id 是 `codearts`）
    #[default]
    Cn,
    /// 国际版（数据面 `ap-southeast-1`，AP-Singapore；provider id 是 `codearts-intl`）
    Intl,
}

impl Region {
    /// 两个区域（注册表顺序：国内版在前）
    pub const ALL: [Region; 2] = [Region::Cn, Region::Intl];

    /// 本区域对应哪个 provider kind（区域 → 身份的**唯一**映射）
    pub const fn kind(self) -> ProviderKind {
        match self {
            Self::Cn => ProviderKind::CodeArts,
            Self::Intl => ProviderKind::CodeArtsIntl,
        }
    }

    /// 本区域的 provider id（`"codearts"` / `"codearts-intl"`）
    pub fn provider_id(self) -> &'static str {
        kind_id(self.kind())
    }

    /// provider id → 区域（`codearts` 系之外的 id 返回 None）
    pub fn from_provider_id(provider_id: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|region| region.provider_id() == provider_id)
    }

    /// 这个 kind 是不是 CodeArts 系（两家都算）—— 判据只在这里写一份
    pub fn from_kind(kind: ProviderKind) -> Option<Self> {
        Self::ALL.into_iter().find(|region| region.kind() == kind)
    }

    /// 界面与日志里的名字（跟在 `CodeArts` 后面的那一段）
    pub const fn label(self) -> &'static str {
        match self {
            Self::Cn => "国内版",
            Self::Intl => "国际版",
        }
    }

    /// 落进账号公开形态的 `edition` 值（与其它家同一套取值：`cn` / `intl`）
    pub const fn edition(self) -> &'static str {
        match self {
            Self::Cn => "cn",
            Self::Intl => "intl",
        }
    }

    /// 本区域的**默认**区域网关基址（不含尾斜杠）。
    ///
    /// 国内版与 [`super::models::DEFAULT_BASE_URL`] 是同一个值 —— 后者是既有
    /// 常量（测试与向量在用），这里作为区域化的**事实来源**，别处新代码取本函数。
    pub const fn default_base_url(self) -> &'static str {
        match self {
            Self::Cn => "https://snap-access.cn-north-4.myhuaweicloud.com",
            Self::Intl => "https://snap-access.ap-southeast-1.myhuaweicloud.com",
        }
    }

    /// 本区域 STS 的**默认**基址（不含尾斜杠；令牌与身份端点都挂在它下面）。
    ///
    /// 两地是**同一台**：官方扩展 `product.json` 的 `HKFramework` 里
    /// `iamStsOpenDomain` 仍是 `sts.cn-north-4`（授权面不分区，见模块头）。
    /// 别把国际版改成 `sts.ap-southeast-*` —— 那些主机上 `/v1/oauth2/tokens`
    /// 回 `STS5.1007 本区域不支持该 API`，登录换码会整个失败。
    pub const fn default_sts_base(self) -> &'static str {
        match self {
            Self::Cn => "https://sts.cn-north-4.myhuaweicloud.com",
            Self::Intl => "https://sts.cn-north-4.myhuaweicloud.com",
        }
    }

    /// 本区域网页登录门户的**默认**站点（授权页 `/portal/authorize` 挂在它下面）。
    pub const fn default_web_login_base(self) -> &'static str {
        match self {
            Self::Cn => "https://codearts.huaweicloud.com",
            // 国际站 CodeArts 控制台（区域化域名，与国内 `codearts.huaweicloud.com`
            // 不是一台）。实测 `/portal/authorize` 返回与国内同形的门户引导页。
            Self::Intl => "https://codearts.ap-southeast-1.huaweicloud.com",
        }
    }

    /// 本区域福利网关的**默认**基址（福利**目录**源；`None` = 该区域没有福利源）。
    ///
    /// **两地同一台** `opengw.developer.huaweicloud.com`：官方扩展的福利后端
    /// （`sue()`）对国内 / 国际是同一组中国主机，只用灰度标志 `isGammaVersion`
    /// 在 `opengw`（生产）与 `snapengine-apig.cn-north-7`（灰度）之间选，**没有按
    /// 地区的分支**。两地的福利**目录**都能取到同样的 4 个模型。
    ///
    /// 注意**目录可调 ≠ 模型可调**：国际版的福利模型注册在中国，推理要另走
    /// [`Self::benefit_chat_base`]（见模块头）。
    pub const fn default_benefit_gateway_url(self) -> Option<&'static str> {
        match self {
            Self::Cn => Some("https://opengw.developer.huaweicloud.com"),
            Self::Intl => Some("https://opengw.developer.huaweicloud.com"),
        }
    }

    /// 本区域目录缓存落在统一库 `kv` 里的 scope 键。
    pub fn catalog_scope(self) -> &'static str {
        use crate::server::core::providers::catalog_cache;
        match self {
            Self::Cn => catalog_cache::SCOPE_CODEARTS,
            Self::Intl => catalog_cache::SCOPE_CODEARTS_INTL,
        }
    }

    /// 本区域环境变量覆盖的前缀。
    ///
    /// 两地各带前缀：国内 `CODEARTS_`、国际 `CODEARTS_INTL_` —— 不能共用一个
    /// 名字，那会让「只想给国际版配代理 / 改端点」变成「两地一起改」
    /// （与 AutoClaw / ZCode 同一条理由）。私有化部署改端点走这些变量，别改常量。
    pub const fn env_prefix(self) -> &'static str {
        match self {
            Self::Cn => "CODEARTS_",
            Self::Intl => "CODEARTS_INTL_",
        }
    }

    /// 读本区域的环境变量覆盖（空值视为未设置，末尾斜杠去掉）。
    ///
    /// 返回 None 表示没有覆盖 —— 调用方回落到上面那几个默认常量。
    pub fn env_override(self, name: &str) -> Option<String> {
        let key = format!("{}{name}", self.env_prefix());
        std::env::var(key)
            .ok()
            .map(|value| value.trim().trim_end_matches('/').to_string())
            .filter(|value| !value.is_empty())
    }

    /// 区域网关基址（`{PREFIX}BASE_URL` 可覆盖；末尾不带斜杠）。
    pub fn base_url(self) -> String {
        self.env_override("BASE_URL")
            .unwrap_or_else(|| self.default_base_url().to_string())
    }

    /// STS 基址（`{PREFIX}STS_BASE` 可覆盖；末尾不带斜杠）。
    pub fn sts_base(self) -> String {
        self.env_override("STS_BASE")
            .unwrap_or_else(|| self.default_sts_base().to_string())
    }

    /// 令牌端点（授权码 / 续期共用）。
    pub fn token_url(self) -> String {
        format!("{}/v1/oauth2/tokens", self.sts_base())
    }

    /// 身份端点（签名后查询当前凭据的 domain / user）。
    pub fn identity_url(self) -> String {
        format!("{}/v5/caller-identity", self.sts_base())
    }

    /// 网页登录门户基址（`{PREFIX}WEB_LOGIN_BASE` 可覆盖；末尾不带斜杠）。
    pub fn web_login_base(self) -> String {
        self.env_override("WEB_LOGIN_BASE")
            .unwrap_or_else(|| self.default_web_login_base().to_string())
    }

    /// 福利网关基址（`{PREFIX}BENEFIT_GATEWAY_URL` 可覆盖；`None` = 该区域没有）。
    ///
    /// 两地默认都是 `opengw.developer.huaweicloud.com`（福利后端不分区，见模块头）；
    /// 环境变量可以把它指到另一个（预发 / 自建代理）地址。注意空串按「未设置」处理
    /// —— 想关掉这个源，把 `benefit_gateway_url` 传空串给 `models::discover`
    /// 即可（它按 `filter(|url| !url.trim().is_empty())` 跳过）。
    pub fn benefit_gateway_url(self) -> Option<String> {
        if let Some(value) = self.env_override("BENEFIT_GATEWAY_URL") {
            return Some(value);
        }
        self.default_benefit_gateway_url().map(str::to_string)
    }

    /// 福利模型的**推理**基址（不含尾斜杠；`{PREFIX}BENEFIT_CHAT_BASE` 可覆盖）。
    ///
    /// 福利模型（`glm-5.3-flash` / `deepseek-v4-*`）的**目录**来自 opengw，但**模型
    /// 注册在中国的 InferHub**：国内版打本区域网关即可（福利模型本就在 cn-north-4）；
    /// 国际版的数据面在 `ap-southeast-1`，调福利模型会 `InferHub.002002009.404`，
    /// 所以要单独打到中国的区域网关 —— 依据是国际账号的凭据本就是 cn-north-4 的
    /// STS 签发的（见模块头）。
    ///
    /// **这是实测中的路由**：若中国网关拒了国际凭据（401/403），就把国际版福利源
    /// 整体关掉（`default_benefit_gateway_url` 回 `None`），别留幽灵模型。
    pub fn benefit_chat_base(self) -> String {
        if let Some(value) = self.env_override("BENEFIT_CHAT_BASE") {
            return value;
        }
        match self {
            // 国内版：福利模型本就在 cn-north-4，跟本区域网关走（`CODEARTS_BASE_URL`
            // 这类私有化覆盖也要一并生效，所以取 `base_url()` 而不是常量）。
            Self::Cn => self.base_url(),
            Self::Intl => "https://snap-access.cn-north-4.myhuaweicloud.com".to_string(),
        }
    }

    /// 本区域账号记录 id 的**前缀**（id 生成用）。
    ///
    /// 国内版沿用既有的 `codearts-`（存量账号的 id 就是它，不能改）；国际版用
    /// `codearts-intl-` 让两地的记录 id 天然不相交 —— 同一个人在国内 / 国际
    /// 两套系统里的 userId 完全可能相同（与 ZCode / AutoClaw 国际版同一处境），
    /// 撞了的话存储层的撞 id 保护会拒绝写入，而「两地账号并存」正是两个 provider
    /// 建模的意义之一。
    pub const fn account_id_prefix(self) -> &'static str {
        match self {
            Self::Cn => "codearts-",
            Self::Intl => "codearts-intl-",
        }
    }
}

#[cfg(test)]
mod tests {
    //! 区域 ↔ provider 身份的互查与端点取值。纯函数，不联网、不碰磁盘。
    use super::*;
    use crate::server::core::providers::catalog_cache;

    #[test]
    fn kind_and_provider_id_round_trip() {
        for region in Region::ALL {
            assert_eq!(region, Region::from_provider_id(region.provider_id()).unwrap_or_else(|| panic!("{region:?} 的 id 要能反查")));
            assert_eq!(Some(region), Region::from_kind(region.kind()));
        }
        assert_eq!("codearts", Region::Cn.provider_id());
        assert_eq!("codearts-intl", Region::Intl.provider_id());
        // 别家的 id 不落进来
        assert!(Region::from_provider_id("zcode").is_none());
        assert!(Region::from_provider_id("codearts-cn").is_none());
    }

    #[test]
    fn the_two_regions_point_at_different_gateways() {
        let cn = Region::Cn;
        let intl = Region::Intl;
        // 数据面（区域网关与门户）两地必须不同（凭据是区域签发的，跨区域必然 401）
        assert_ne!(cn.default_base_url(), intl.default_base_url());
        assert_ne!(cn.default_web_login_base(), intl.default_web_login_base());
        // 但**授权面是同一台**：STS 令牌 / 身份端点两地共用 cn-north-4
        // （官方扩展 product.json 的 HKFramework.iamStsOpenDomain，见模块头）
        assert_eq!(cn.default_sts_base(), intl.default_sts_base());
        // 国际版的数据面落在 AP-Singapore（`ap-southeast-1`），国内版落在 cn-north-4
        assert!(cn.default_base_url().contains("cn-north-4"));
        assert!(intl.default_base_url().contains("ap-southeast-1"));
        assert!(intl.default_web_login_base().contains("ap-southeast-1"));
        // 两地的令牌 / 身份端点都挂在 cn-north-4 STS 下（不能是区域化的 sts）
        assert!(intl.token_url().contains("sts.cn-north-4"));
        assert!(cn.token_url().starts_with(cn.default_sts_base()));
        assert!(intl.identity_url().starts_with(intl.default_sts_base()));
        assert!(cn.token_url().ends_with("/v1/oauth2/tokens"));
        assert!(intl.identity_url().ends_with("/v5/caller-identity"));
    }

    #[test]
    fn both_regions_share_the_benefit_gateway() {
        // 福利**目录**不分区：官方扩展对国内 / 国际用同一组中国主机（见模块头）。
        assert_eq!(
            Some("https://opengw.developer.huaweicloud.com"),
            Region::Cn.default_benefit_gateway_url()
        );
        assert_eq!(
            Region::Cn.default_benefit_gateway_url(),
            Region::Intl.default_benefit_gateway_url()
        );
    }

    #[test]
    fn intl_benefit_models_are_routed_to_the_china_gateway() {
        // 福利模型注册在中国的 InferHub：国内版本就在中国，国际版要单独打过去。
        assert_eq!(Region::Cn.default_base_url(), Region::Cn.benefit_chat_base());
        assert_eq!(
            "https://snap-access.cn-north-4.myhuaweicloud.com",
            Region::Intl.benefit_chat_base()
        );
        // 普通模型的推理基址仍是本区域（国际版 = ap-southeast-1），不受影响。
        assert!(Region::Intl.base_url().contains("ap-southeast-1"));
        assert_ne!(Region::Intl.base_url(), Region::Intl.benefit_chat_base());
    }

    #[test]
    fn the_two_regions_have_distinct_identity_and_cache_slots() {
        assert_ne!(Region::Cn.account_id_prefix(), Region::Intl.account_id_prefix());
        assert_ne!(Region::Cn.catalog_scope(), Region::Intl.catalog_scope());
        // 两个 scope 都必须登记在 ALL_SCOPES 里，否则目录缓存不会按它恢复
        for region in Region::ALL {
            assert!(
                catalog_cache::ALL_SCOPES.contains(&region.catalog_scope()),
                "{region:?} 的 scope 没登记进 ALL_SCOPES"
            );
        }
        // 国内版的 scope 是既有值（存量缓存不能改名）
        assert_eq!(catalog_cache::SCOPE_CODEARTS, Region::Cn.catalog_scope());
    }

    #[test]
    fn edition_and_label_are_distinct() {
        assert_eq!("cn", Region::Cn.edition());
        assert_eq!("intl", Region::Intl.edition());
        assert_eq!("国内版", Region::Cn.label());
        assert_eq!("国际版", Region::Intl.label());
    }
}
