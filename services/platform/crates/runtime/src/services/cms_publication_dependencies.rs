//! CMS V2 publication dependency extraction and exact-target validation.
//!
//! Callers should extract first, lock [`ExtractedPublicationDependencies::lock_targets`]
//! in the returned order inside their SERIALIZABLE transaction, then validate.
//! No Product Master facts are copied into CMS rows: product dependencies retain
//! only the stable product ID and its exact published revision.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::postgres::PgConnection;
use uuid::Uuid;

use airtek_domain::models::ContentDraftV2;

#[path = "cms_publication_dependencies/extraction.rs"]
mod extraction;
#[path = "cms_publication_dependencies/reverse.rs"]
mod reverse;
#[path = "cms_publication_dependencies/validation.rs"]
mod validation;

pub use reverse::{active_content_dependents, ActiveContentDependent};
pub const CMS_DEPENDENCY_EXTRACTOR_VERSION: &str = "cms-v2-dependencies-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PublicationDependencyKind {
    ContentLink,
    RelationContent,
    RelationProduct,
    MediaInline,
    MediaDownload,
}

impl PublicationDependencyKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::ContentLink => "contentLink",
            Self::RelationContent => "relationContent",
            Self::RelationProduct => "relationProduct",
            Self::MediaInline => "mediaInline",
            Self::MediaDownload => "mediaDownload",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DependencyIssueCode {
    InvalidSchemaVersion,
    InvalidType,
    InvalidUuid,
    InvalidTargetType,
    NavigationDepthExceeded,
    TargetMissing,
    ContentNotPublished,
    ContentLocaleMismatch,
    ContentSchemaMismatch,
    ContentRouteMissing,
    ProductNotPublished,
    ProductRevisionMissing,
    MediaMissing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DependencyGate {
    DocumentShape,
    TargetIdentity,
    ContentPublished,
    ContentLocale,
    ContentSchemaVersion,
    ContentPublicRoute,
    ProductPublished,
    ProductRevision,
    MediaExists,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyBlockingIssue {
    pub code: DependencyIssueCode,
    pub path: String,
    pub failed_gate: DependencyGate,
    pub target_id: Option<Uuid>,
    pub detail: String,
}

impl DependencyBlockingIssue {
    pub(crate) fn new(
        code: DependencyIssueCode,
        path: impl Into<String>,
        failed_gate: DependencyGate,
        target_id: Option<Uuid>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            code,
            path: path.into(),
            failed_gate,
            target_id,
            detail: detail.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExtractedDependencyTarget {
    Content(Uuid),
    Product(Uuid),
    Media(Uuid),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtractedDependency {
    pub reference_path: String,
    pub kind: PublicationDependencyKind,
    pub target: ExtractedDependencyTarget,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PublicationLockTargetKind {
    Content,
    Product,
    Media,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PublicationLockTarget {
    pub kind: PublicationLockTargetKind,
    pub id: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtractedPublicationDependencies {
    pub source_locale: Option<String>,
    pub references: Vec<ExtractedDependency>,
    pub blocking_issues: Vec<DependencyBlockingIssue>,
    pub lock_targets: Vec<PublicationLockTarget>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PublicationDependencyTarget {
    Content(Uuid),
    Product {
        product_id: Uuid,
        product_revision: i64,
    },
    Media(Uuid),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicationDependencyRow {
    pub reference_path: String,
    pub kind: PublicationDependencyKind,
    pub target: PublicationDependencyTarget,
}

impl PublicationDependencyRow {
    pub fn stable_id(&self, source_content_id: Uuid, source_revision: i64) -> Uuid {
        let mut digest = Sha256::new();
        digest.update(b"airtek:cms-publication-dependency:v1\0");
        digest.update(source_content_id.as_bytes());
        digest.update(source_revision.to_be_bytes());
        digest.update(self.kind.label().as_bytes());
        digest.update([0]);
        digest.update(self.reference_path.as_bytes());
        match &self.target {
            PublicationDependencyTarget::Content(content_id) => {
                digest.update(content_id.as_bytes());
            }
            PublicationDependencyTarget::Product {
                product_id,
                product_revision,
            } => {
                digest.update(product_id.as_bytes());
                digest.update(product_revision.to_be_bytes());
            }
            PublicationDependencyTarget::Media(asset_id) => {
                digest.update(asset_id.as_bytes());
            }
        }
        let digest = digest.finalize();
        let mut bytes = [0_u8; 16];
        bytes.copy_from_slice(&digest[..16]);
        bytes[6] = (bytes[6] & 0x0f) | 0x80;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        Uuid::from_bytes(bytes)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DependencySnapshotStatus {
    Complete,
    Blocked,
}

impl DependencySnapshotStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Blocked => "blocked",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedPublicationDependencies {
    pub dependencies: Vec<PublicationDependencyRow>,
    pub blocking_issues: Vec<DependencyBlockingIssue>,
    pub lock_targets: Vec<PublicationLockTarget>,
}

impl ValidatedPublicationDependencies {
    pub fn status(&self) -> DependencySnapshotStatus {
        if self.blocking_issues.is_empty() {
            DependencySnapshotStatus::Complete
        } else {
            DependencySnapshotStatus::Blocked
        }
    }

    pub fn is_complete(&self) -> bool {
        self.status() == DependencySnapshotStatus::Complete
    }
}

/// Extracts dependency-bearing fields from stored JSON and reports malformed
/// dependency containers as blocking issues. This is not a replacement for
/// the existing full `ContentDraftV2` and template publication validation.
pub fn extract_document(document: &Value) -> ExtractedPublicationDependencies {
    extraction::extract(document)
}

/// Preferred live-publication entry point after the typed draft has passed the
/// existing CMS template publish validation.
pub fn extract_draft(draft: &ContentDraftV2) -> ExtractedPublicationDependencies {
    let document = serde_json::to_value(draft).expect("ContentDraftV2 must serialize");
    extract_document(&document)
}

pub async fn lock_targets(
    connection: &mut PgConnection,
    targets: &[PublicationLockTarget],
) -> Result<(), sqlx::Error> {
    let mut targets = targets.to_vec();
    targets.sort_unstable();
    targets.dedup();
    for target in targets {
        let sql = match target.kind {
            PublicationLockTargetKind::Content => {
                "SELECT id FROM content_entries WHERE id=$1 FOR UPDATE"
            }
            PublicationLockTargetKind::Product => "SELECT id FROM products WHERE id=$1 FOR UPDATE",
            PublicationLockTargetKind::Media => {
                "SELECT id FROM media_assets WHERE id=$1 FOR UPDATE"
            }
        };
        let _ = sqlx::query_scalar::<_, Uuid>(sql)
            .bind(target.id)
            .fetch_optional(&mut *connection)
            .await?;
    }
    Ok(())
}

pub async fn validate_extracted(
    connection: &mut PgConnection,
    extracted: ExtractedPublicationDependencies,
) -> Result<ValidatedPublicationDependencies, sqlx::Error> {
    validation::validate(connection, extracted).await
}

pub async fn validate_document(
    connection: &mut PgConnection,
    document: &Value,
) -> Result<ValidatedPublicationDependencies, sqlx::Error> {
    validate_extracted(connection, extract_document(document)).await
}

pub async fn replace_current(
    connection: &mut PgConnection,
    source_content_id: Uuid,
    source_revision: i64,
    _created_by: &str,
    plan: &ValidatedPublicationDependencies,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM cms_current_publication_dependencies WHERE source_content_id=$1")
        .bind(source_content_id)
        .execute(&mut *connection)
        .await?;

    for dependency in &plan.dependencies {
        insert_dependency_row(connection, source_content_id, source_revision, dependency).await?;
    }
    Ok(())
}

async fn insert_dependency_row(
    connection: &mut PgConnection,
    source_content_id: Uuid,
    source_revision: i64,
    dependency: &PublicationDependencyRow,
) -> Result<(), sqlx::Error> {
    let (content_id, product_id, product_revision, asset_id) = match &dependency.target {
        PublicationDependencyTarget::Content(content_id) => (Some(*content_id), None, None, None),
        PublicationDependencyTarget::Product {
            product_id,
            product_revision,
        } => (None, Some(*product_id), Some(*product_revision), None),
        PublicationDependencyTarget::Media(asset_id) => (None, None, None, Some(*asset_id)),
    };
    sqlx::query(
        r#"INSERT INTO cms_current_publication_dependencies
           (id,source_content_id,reference_path,dependency_kind,target_content_id,
            target_product_id,target_product_revision,target_media_asset_id)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8)"#,
    )
    .bind(dependency.stable_id(source_content_id, source_revision))
    .bind(source_content_id)
    .bind(&dependency.reference_path)
    .bind(dependency.kind.label())
    .bind(content_id)
    .bind(product_id)
    .bind(product_revision)
    .bind(asset_id)
    .execute(connection)
    .await?;
    Ok(())
}

pub(crate) fn sort_issues(issues: &mut [DependencyBlockingIssue]) {
    issues.sort_by(|left, right| {
        (&left.path, left.code, left.failed_gate, left.target_id).cmp(&(
            &right.path,
            right.code,
            right.failed_gate,
            right.target_id,
        ))
    });
}
