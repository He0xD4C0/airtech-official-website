#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TemplateResolutionError {
    Missing,
    Unsupported,
    Incompatible,
}

pub(super) fn resolve_template(
    legacy_kind: &str,
    template_key: Option<&str>,
) -> Result<(Kind, Template), TemplateResolutionError> {
    if legacy_kind == "navigation" {
        return match template_key {
            None | Some("navigation") => registered(Kind::Navigation, Template::Navigation),
            Some(key) if known_template(key) => Err(TemplateResolutionError::Incompatible),
            Some(_) => Err(TemplateResolutionError::Unsupported),
        };
    }
    if legacy_kind == "footer" {
        return match template_key {
            None | Some("footer") => registered(Kind::Footer, Template::Footer),
            Some(key) if known_template(key) => Err(TemplateResolutionError::Incompatible),
            Some(_) => Err(TemplateResolutionError::Unsupported),
        };
    }
    let template_key = template_key.ok_or(TemplateResolutionError::Missing)?;
    let result = match (legacy_kind, template_key) {
        ("home", "home") => (Kind::Home, Template::Home),
        ("home", "productIndex") => (Kind::Page, Template::ProductIndex),
        ("home", "productFamily") => (Kind::Page, Template::ProductFamily),
        ("home", "selector") => (Kind::Page, Template::Selector),
        ("home", "compare") => (Kind::Page, Template::Compare),
        ("home", "newsIndex") => (Kind::Page, Template::NewsIndex),
        ("home", "rfqRouter") => (Kind::Page, Template::RfqRouter),
        ("home", "rfqForm") => (Kind::Page, Template::RfqForm),
        ("home", "search") => (Kind::Page, Template::Search),
        ("solution", "solutionIndex") => (Kind::Page, Template::SolutionIndex),
        ("solution", "solutionDetail") => (Kind::Solution, Template::SolutionDetail),
        ("technology", "technologyIndex") => (Kind::Page, Template::TechnologyIndex),
        ("technology", "technologyDetail") => (Kind::Technology, Template::TechnologyDetail),
        ("article", "articleIndex") => (Kind::Page, Template::ArticleIndex),
        ("article", "articleDetail") => (Kind::Article, Template::ArticleDetail),
        ("news", "newsDetail") => (Kind::News, Template::NewsDetail),
        ("faq", "faqIndex") => (Kind::Page, Template::FaqIndex),
        ("faq", "faqDetail") => (Kind::Faq, Template::FaqDetail),
        ("caseStudy", "caseStudyIndex") => (Kind::Page, Template::CaseStudyIndex),
        ("caseStudy", "caseStudyDetail") => (Kind::CaseStudy, Template::CaseStudyDetail),
        ("download", "downloadIndex") => (Kind::Page, Template::DownloadIndex),
        ("download", "downloadDetail") => (Kind::Download, Template::DownloadDetail),
        ("company", "about") => (Kind::Company, Template::About),
        ("company", "contact") => (Kind::Company, Template::Contact),
        ("legal", "legal") => (Kind::Legal, Template::Legal),
        _ if known_template(template_key) => return Err(TemplateResolutionError::Incompatible),
        _ => return Err(TemplateResolutionError::Unsupported),
    };
    registered(result.0, result.1)
}

fn registered(kind: Kind, template: Template) -> Result<(Kind, Template), TemplateResolutionError> {
    let definition = template_definition(template).ok_or(TemplateResolutionError::Unsupported)?;
    if definition.content_kind != kind {
        return Err(TemplateResolutionError::Incompatible);
    }
    Ok((kind, template))
}

fn known_template(value: &str) -> bool {
    matches!(
        value,
        "home"
            | "productIndex"
            | "productFamily"
            | "selector"
            | "compare"
            | "solutionIndex"
            | "solutionDetail"
            | "technologyIndex"
            | "technologyDetail"
            | "articleIndex"
            | "articleDetail"
            | "newsIndex"
            | "newsDetail"
            | "faqIndex"
            | "faqDetail"
            | "caseStudyIndex"
            | "caseStudyDetail"
            | "downloadIndex"
            | "downloadDetail"
            | "about"
            | "contact"
            | "company"
            | "legal"
            | "rfqRouter"
            | "rfqForm"
            | "search"
            | "navigation"
            | "footer"
    )
}
use crate::{
    models::{CmsContentKind as Kind, ContentTemplateKey as Template},
    services::cms_templates::template_definition,
};
