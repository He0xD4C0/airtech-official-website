use airtek_domain::models::{ContentTypeFields, FooterColumn, NavigationItem};

use super::{
    invalid, optional_media, optional_string, string, valid_slug, validate_action, validate_seo,
    validate_target, ContractIssue,
};

pub(super) fn validate(fields: &ContentTypeFields, issues: &mut Vec<ContractIssue>) {
    match fields {
        ContentTypeFields::Solution(value) | ContentTypeFields::Technology(value) => {
            optional_string(value.key.as_deref(), 120, "typeFields.key", issues);
        }
        ContentTypeFields::Article(value) | ContentTypeFields::News(value) => {
            optional_string(
                value.category.as_deref(),
                120,
                "typeFields.category",
                issues,
            );
            optional_string(
                value.author_display_name.as_deref(),
                200,
                "typeFields.authorDisplayName",
                issues,
            );
            optional_media(value.cover.as_ref(), "typeFields.cover", issues);
        }
        ContentTypeFields::Faq(value) => {
            for (index, item) in value.items.iter().enumerate() {
                string(
                    &item.question,
                    500,
                    &format!("typeFields.items.{index}.question"),
                    issues,
                );
            }
        }
        ContentTypeFields::CaseStudy(value) => {
            optional_string(
                value.industry.as_deref(),
                200,
                "typeFields.industry",
                issues,
            );
            optional_string(
                value.location.as_deref(),
                200,
                "typeFields.location",
                issues,
            );
        }
        ContentTypeFields::Download(value) => {
            optional_string(
                value.version_label.as_deref(),
                200,
                "typeFields.versionLabel",
                issues,
            );
            optional_string(
                value.resource_type.as_deref(),
                120,
                "typeFields.resourceType",
                issues,
            );
            optional_string(
                value.version_notes.as_deref(),
                2_000,
                "typeFields.versionNotes",
                issues,
            );
        }
        ContentTypeFields::GeneralInformation(value) => {
            validate_general_information(value, issues);
        }
        ContentTypeFields::Navigation(value) => {
            validate_navigation(&value.items, "typeFields.items", issues);
        }
        ContentTypeFields::Footer(value) => {
            for (index, column) in value.columns.iter().enumerate() {
                validate_footer_column(column, &format!("typeFields.columns.{index}"), issues);
            }
            validate_navigation(&value.legal_links, "typeFields.legalLinks", issues);
        }
        ContentTypeFields::Home
        | ContentTypeFields::Page
        | ContentTypeFields::Company
        | ContentTypeFields::Legal(_) => {}
    }
}

fn validate_general_information(
    value: &airtek_domain::models::GeneralInformationTypeFields,
    issues: &mut Vec<ContractIssue>,
) {
    optional_string(
        value.organization_name.as_deref(),
        200,
        "typeFields.organizationName",
        issues,
    );
    optional_string(
        value.brand_line.as_deref(),
        500,
        "typeFields.brandLine",
        issues,
    );
    optional_string(
        value.home_path.as_deref(),
        2_048,
        "typeFields.homePath",
        issues,
    );
    optional_string(
        value.footer_statement.as_deref(),
        1_000,
        "typeFields.footerStatement",
        issues,
    );
    optional_string(
        value.copyright_template.as_deref(),
        500,
        "typeFields.copyrightTemplate",
        issues,
    );
    validate_contact(&value.contact, issues);
    for (index, link) in value.social_links.iter().enumerate() {
        string(
            &link.service,
            120,
            &format!("typeFields.socialLinks.{index}.service"),
            issues,
        );
        string(
            &link.url,
            2_048,
            &format!("typeFields.socialLinks.{index}.url"),
            issues,
        );
    }
    validate_seo(&value.default_seo, "typeFields.defaultSeo", issues);
    for (index, category) in value.product_categories.iter().enumerate() {
        let path = format!("typeFields.productCategories.{index}");
        string(&category.slug, 180, &format!("{path}.slug"), issues);
        if !valid_slug(&category.slug) {
            invalid(
                issues,
                &format!("{path}.slug"),
                "Slug must contain lowercase ASCII words separated by single hyphens.",
            );
        }
        string(&category.name, 120, &format!("{path}.name"), issues);
        string(
            &category.description,
            2_000,
            &format!("{path}.description"),
            issues,
        );
    }
    if let Some(action) = &value.navigation_cta {
        validate_action(action, "typeFields.navigationCta", issues);
    }
}

fn validate_contact(
    contact: &airtek_domain::models::ContactInformationInput,
    issues: &mut Vec<ContractIssue>,
) {
    optional_string(
        contact.email.as_deref(),
        254,
        "typeFields.contact.email",
        issues,
    );
    optional_string(
        contact.phone.as_deref(),
        50,
        "typeFields.contact.phone",
        issues,
    );
    for (index, line) in contact.address_lines.iter().enumerate() {
        string(
            line,
            300,
            &format!("typeFields.contact.addressLines.{index}"),
            issues,
        );
    }
    optional_string(
        contact.locality.as_deref(),
        200,
        "typeFields.contact.locality",
        issues,
    );
    optional_string(
        contact.region.as_deref(),
        200,
        "typeFields.contact.region",
        issues,
    );
    optional_string(
        contact.postal_code.as_deref(),
        50,
        "typeFields.contact.postalCode",
        issues,
    );
    optional_string(
        contact.country_code.as_deref(),
        2,
        "typeFields.contact.countryCode",
        issues,
    );
}

fn validate_footer_column(column: &FooterColumn, path: &str, issues: &mut Vec<ContractIssue>) {
    string(&column.title, 120, &format!("{path}.title"), issues);
    validate_navigation(&column.links, &format!("{path}.links"), issues);
}

fn validate_navigation(items: &[NavigationItem], path: &str, issues: &mut Vec<ContractIssue>) {
    for (index, item) in items.iter().enumerate() {
        let item_path = format!("{path}.{index}");
        string(&item.label, 120, &format!("{item_path}.label"), issues);
        if let Some(target) = &item.target {
            validate_target(target, &format!("{item_path}.target"), issues);
        }
        validate_navigation(&item.children, &format!("{item_path}.children"), issues);
    }
}
