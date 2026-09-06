use std::sync::LazyLock;

use crate::models::{
    CmsBodyPolicy, CmsContentKind, ContentBlockKind, ContentTemplateDefinition, ContentTemplateKey,
};

mod validation;

pub use validation::{
    validate_content_draft, CmsTemplateValidationCode, CmsTemplateValidationIssue,
    CmsTemplateValidationPhase,
};

const EMPTY: &[ContentBlockKind] = &[];
const PAGE_ALLOWED: &[ContentBlockKind] = &[
    ContentBlockKind::Hero,
    ContentBlockKind::Body,
    ContentBlockKind::Media,
    ContentBlockKind::FeatureGrid,
    ContentBlockKind::Evidence,
    ContentBlockKind::Cta,
    ContentBlockKind::RelationCollection,
];
const EDITORIAL_ALLOWED: &[ContentBlockKind] = &[
    ContentBlockKind::Hero,
    ContentBlockKind::Body,
    ContentBlockKind::Media,
    ContentBlockKind::FeatureGrid,
    ContentBlockKind::Evidence,
    ContentBlockKind::Cta,
    ContentBlockKind::RelationCollection,
    ContentBlockKind::FaqCollection,
    ContentBlockKind::DownloadAsset,
];
const HERO_REQUIRED: &[ContentBlockKind] = &[ContentBlockKind::Hero];
const HERO_BODY_REQUIRED: &[ContentBlockKind] = &[ContentBlockKind::Hero, ContentBlockKind::Body];
const FAQ_REQUIRED: &[ContentBlockKind] =
    &[ContentBlockKind::Hero, ContentBlockKind::FaqCollection];
const DOWNLOAD_REQUIRED: &[ContentBlockKind] =
    &[ContentBlockKind::Hero, ContentBlockKind::DownloadAsset];
const CONTACT_REQUIRED: &[ContentBlockKind] =
    &[ContentBlockKind::Hero, ContentBlockKind::ContactBlock];

static TEMPLATE_REGISTRY: LazyLock<Vec<ContentTemplateDefinition>> = LazyLock::new(build_registry);

pub fn template_registry() -> &'static [ContentTemplateDefinition] {
    TEMPLATE_REGISTRY.as_slice()
}

pub fn template_definition(key: ContentTemplateKey) -> Option<&'static ContentTemplateDefinition> {
    template_registry().iter().find(|entry| entry.key == key)
}

fn definition(
    key: ContentTemplateKey,
    content_kind: CmsContentKind,
    body_policy: CmsBodyPolicy,
    required_blocks: &[ContentBlockKind],
    allowed_blocks: &[ContentBlockKind],
    routable: bool,
    singleton_per_locale: bool,
) -> ContentTemplateDefinition {
    ContentTemplateDefinition {
        key,
        content_kind,
        body_policy,
        required_blocks: required_blocks.to_vec(),
        allowed_blocks: allowed_blocks.to_vec(),
        routable,
        singleton_per_locale,
    }
}

fn page(key: ContentTemplateKey, singleton_per_locale: bool) -> ContentTemplateDefinition {
    definition(
        key,
        CmsContentKind::Page,
        CmsBodyPolicy::Optional,
        HERO_REQUIRED,
        PAGE_ALLOWED,
        true,
        singleton_per_locale,
    )
}

fn editorial(key: ContentTemplateKey, kind: CmsContentKind) -> ContentTemplateDefinition {
    definition(
        key,
        kind,
        CmsBodyPolicy::Required,
        HERO_BODY_REQUIRED,
        EDITORIAL_ALLOWED,
        true,
        false,
    )
}

fn build_registry() -> Vec<ContentTemplateDefinition> {
    use CmsBodyPolicy::{Forbidden, Optional, Required};
    use CmsContentKind as Kind;
    use ContentBlockKind as Block;
    use ContentTemplateKey as Template;

    vec![
        definition(
            Template::Home,
            Kind::Home,
            Optional,
            HERO_REQUIRED,
            PAGE_ALLOWED,
            true,
            true,
        ),
        page(Template::ProductIndex, true),
        page(Template::ProductFamily, false),
        page(Template::Selector, true),
        page(Template::Compare, true),
        page(Template::SolutionIndex, true),
        editorial(Template::SolutionDetail, Kind::Solution),
        page(Template::TechnologyIndex, true),
        editorial(Template::TechnologyDetail, Kind::Technology),
        page(Template::ArticleIndex, true),
        editorial(Template::ArticleDetail, Kind::Article),
        page(Template::NewsIndex, true),
        editorial(Template::NewsDetail, Kind::News),
        definition(
            Template::FaqIndex,
            Kind::Page,
            Optional,
            HERO_REQUIRED,
            &[
                Block::Hero,
                Block::Body,
                Block::Cta,
                Block::RelationCollection,
                Block::FaqCollection,
            ],
            true,
            true,
        ),
        definition(
            Template::FaqDetail,
            Kind::Faq,
            Forbidden,
            FAQ_REQUIRED,
            &[
                Block::Hero,
                Block::Cta,
                Block::RelationCollection,
                Block::FaqCollection,
            ],
            true,
            false,
        ),
        page(Template::CaseStudyIndex, true),
        editorial(Template::CaseStudyDetail, Kind::CaseStudy),
        page(Template::DownloadIndex, true),
        definition(
            Template::DownloadDetail,
            Kind::Download,
            Optional,
            DOWNLOAD_REQUIRED,
            EDITORIAL_ALLOWED,
            true,
            false,
        ),
        editorial(Template::About, Kind::Company),
        definition(
            Template::Contact,
            Kind::Company,
            Optional,
            CONTACT_REQUIRED,
            &[
                Block::Hero,
                Block::Body,
                Block::Media,
                Block::Cta,
                Block::ContactBlock,
            ],
            true,
            true,
        ),
        page(Template::RfqRouter, true),
        page(Template::RfqForm, false),
        definition(
            Template::Search,
            Kind::Page,
            Forbidden,
            HERO_REQUIRED,
            &[Block::Hero],
            true,
            true,
        ),
        definition(
            Template::Legal,
            Kind::Legal,
            Required,
            &[Block::Body],
            &[Block::Hero, Block::Body],
            true,
            false,
        ),
        definition(
            Template::Navigation,
            Kind::Navigation,
            Forbidden,
            EMPTY,
            EMPTY,
            false,
            true,
        ),
        definition(
            Template::Footer,
            Kind::Footer,
            Forbidden,
            EMPTY,
            EMPTY,
            false,
            true,
        ),
        definition(
            Template::GeneralInformation,
            Kind::GeneralInformation,
            Forbidden,
            EMPTY,
            EMPTY,
            false,
            true,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    const ALL_KEYS: &[ContentTemplateKey] = &[
        ContentTemplateKey::Home,
        ContentTemplateKey::ProductIndex,
        ContentTemplateKey::ProductFamily,
        ContentTemplateKey::Selector,
        ContentTemplateKey::Compare,
        ContentTemplateKey::SolutionIndex,
        ContentTemplateKey::SolutionDetail,
        ContentTemplateKey::TechnologyIndex,
        ContentTemplateKey::TechnologyDetail,
        ContentTemplateKey::ArticleIndex,
        ContentTemplateKey::ArticleDetail,
        ContentTemplateKey::NewsIndex,
        ContentTemplateKey::NewsDetail,
        ContentTemplateKey::FaqIndex,
        ContentTemplateKey::FaqDetail,
        ContentTemplateKey::CaseStudyIndex,
        ContentTemplateKey::CaseStudyDetail,
        ContentTemplateKey::DownloadIndex,
        ContentTemplateKey::DownloadDetail,
        ContentTemplateKey::About,
        ContentTemplateKey::Contact,
        ContentTemplateKey::RfqRouter,
        ContentTemplateKey::RfqForm,
        ContentTemplateKey::Search,
        ContentTemplateKey::Legal,
        ContentTemplateKey::Navigation,
        ContentTemplateKey::Footer,
        ContentTemplateKey::GeneralInformation,
    ];

    #[test]
    fn registry_contains_each_template_once() {
        let keys = template_registry()
            .iter()
            .map(|definition| definition.key)
            .collect::<HashSet<_>>();
        assert_eq!(keys.len(), template_registry().len());
        assert_eq!(keys, ALL_KEYS.iter().copied().collect());
    }

    #[test]
    fn every_required_block_is_allowed_and_body_policy_is_coherent() {
        for definition in template_registry() {
            assert!(definition
                .required_blocks
                .iter()
                .all(|kind| definition.allowed_blocks.contains(kind)));
            if definition.body_policy == CmsBodyPolicy::Required {
                assert!(definition.required_blocks.contains(&ContentBlockKind::Body));
            }
            if definition.body_policy == CmsBodyPolicy::Forbidden {
                assert!(!definition.allowed_blocks.contains(&ContentBlockKind::Body));
            }
        }
    }

    #[test]
    fn non_routable_templates_are_singleton_configuration_documents() {
        for key in [
            ContentTemplateKey::Navigation,
            ContentTemplateKey::Footer,
            ContentTemplateKey::GeneralInformation,
        ] {
            let definition = template_definition(key).expect("registered template");
            assert!(!definition.routable);
            assert!(definition.singleton_per_locale);
            assert_eq!(definition.body_policy, CmsBodyPolicy::Forbidden);
        }
    }
}
