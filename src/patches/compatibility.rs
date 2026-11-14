use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use super::{ChildObjectScope, PatchRule, PatchSpec, PatchValue, PatchVariant, rule_revisions};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct WebOsMajor(pub u16);

impl WebOsMajor {
    pub const WEBOS_24: Self = Self(24);
    pub const WEBOS_25: Self = Self(25);
    pub const WEBOS_26: Self = Self(26);

    pub const SUPPORTED: [Self; 3] = [Self::WEBOS_24, Self::WEBOS_25, Self::WEBOS_26];

    pub const fn number(self) -> u16 {
        self.0
    }

    /// Normalize an explicit webOS release/major label without ever inferring
    /// a major version from numeric firmware strings such as `33.22.75`.
    pub fn parse_release_label(raw: &str) -> Result<Self, IdentityError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(IdentityError::MissingWebOsMajor);
        }
        if trimmed.contains('.') || trimmed.contains('/') || trimmed.contains(',') {
            return Err(IdentityError::UnsupportedReleaseLabel(trimmed.to_owned()));
        }

        let lower = trimmed.to_ascii_lowercase();
        if let Some(after_webos) = lower.strip_prefix("webos") {
            let after_tv = after_webos
                .trim_start_matches([' ', '-', '_'])
                .strip_prefix("tv")
                .unwrap_or(after_webos);
            let digits = after_tv.trim_start_matches([' ', '-', '_']);
            if digits.is_empty()
                || digits.starts_with('0')
                || !digits.bytes().all(|byte| byte.is_ascii_digit())
            {
                return Err(IdentityError::UnsupportedReleaseLabel(trimmed.to_owned()));
            }
            let major = digits
                .parse::<u16>()
                .map_err(|_| IdentityError::UnsupportedReleaseLabel(trimmed.to_owned()))?;
            if !(1..=99).contains(&major) {
                return Err(IdentityError::UnsupportedReleaseLabel(trimmed.to_owned()));
            }
            return Ok(Self(major));
        }

        if !trimmed.starts_with('0') && trimmed.bytes().all(|byte| byte.is_ascii_digit()) {
            let major = trimmed
                .parse::<u16>()
                .map_err(|_| IdentityError::UnsupportedReleaseLabel(trimmed.to_owned()))?;
            if !(22..=99).contains(&major) {
                return Err(IdentityError::UnsupportedReleaseLabel(trimmed.to_owned()));
            }
            return Ok(Self(major));
        }

        Err(IdentityError::UnsupportedReleaseLabel(trimmed.to_owned()))
    }
}

impl fmt::Display for WebOsMajor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "webOS {}", self.0)
    }
}

impl FromStr for WebOsMajor {
    type Err = IdentityError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse_release_label(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentitySource {
    LiveTv,
    ManualPreview,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawTvIdentity {
    /// Raw webOS release / platform label (e.g. `webOSTV 24`), if reported.
    pub webos_release: Option<String>,
    /// Raw `sdkVersion` from system properties, preserved separately.
    pub sdk_version: Option<String>,
    /// Exact TV-reported software/firmware version (e.g. `33.22.75`), never used to infer major.
    pub firmware_version: Option<String>,
    /// Raw TV model identifier (e.g. `OLED55C4PSA`), if reported.
    pub model: Option<String>,
    /// Raw machine/platform identifier (e.g. `O24`), if reported.
    pub machine: Option<String>,
    /// Raw TV region (e.g. `GLOBAL`), if reported.
    pub region: Option<String>,
    /// Raw internal OS release (e.g. starfish `10.2.2-57`), preserved for diagnostics.
    pub internal_release: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TvIdentity {
    pub major: WebOsMajor,
    pub source: IdentitySource,
    pub raw: RawTvIdentity,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum IdentityError {
    #[error("webOS major version is missing")]
    MissingWebOsMajor,
    #[error("unsupported or ambiguous webOS release label `{0}`")]
    UnsupportedReleaseLabel(String),
    #[error("conflicting webOS major versions detected: {first} vs {second}")]
    ConflictingWebOsMajor {
        first: WebOsMajor,
        second: WebOsMajor,
    },
}

/// Normalize detected TV identity values into a `TvIdentity` while preserving
/// all raw detected fields and never inferring `major` from `firmware_version`.
pub fn normalize_tv_identity(
    raw: RawTvIdentity,
    source: IdentitySource,
) -> Result<TvIdentity, IdentityError> {
    normalize_tv_identity_from_candidates(raw, source, &[])
}

/// Normalize `raw.webos_release` together with any additional explicit release-label
/// candidates, rejecting missing, invalid, or conflicting webOS major values.
pub fn normalize_tv_identity_from_candidates(
    raw: RawTvIdentity,
    source: IdentitySource,
    extra_release_candidates: &[&str],
) -> Result<TvIdentity, IdentityError> {
    let mut resolved: Option<WebOsMajor> = None;

    let raw_release = raw
        .webos_release
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    for candidate in raw_release.into_iter().chain(
        extra_release_candidates
            .iter()
            .copied()
            .map(str::trim)
            .filter(|value| !value.is_empty()),
    ) {
        let parsed = WebOsMajor::parse_release_label(candidate)?;
        match resolved {
            Some(existing) if existing != parsed => {
                return Err(IdentityError::ConflictingWebOsMajor {
                    first: existing,
                    second: parsed,
                });
            }
            Some(_) => {}
            None => resolved = Some(parsed),
        }
    }

    let major = resolved.ok_or(IdentityError::MissingWebOsMajor)?;
    Ok(TvIdentity { major, source, raw })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatchRuleKind {
    Scalar,
    ExpressionLiterals,
    ScalarGroup,
    FunctionStatements,
    IfCondition,
    Import,
    AppendMembers,
    MultiSite,
}

impl PatchRuleKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Scalar => "scalar",
            Self::ExpressionLiterals => "expression_literals",
            Self::ScalarGroup => "scalar_group",
            Self::FunctionStatements => "function_statements",
            Self::IfCondition => "if_condition",
            Self::Import => "import",
            Self::AppendMembers => "append_members",
            Self::MultiSite => "multi_site",
        }
    }

    pub const fn handler_revision(self, revisions: RuleRevisions) -> u32 {
        match self {
            Self::Scalar => revisions.scalar,
            Self::ExpressionLiterals => revisions.expression_literals,
            Self::ScalarGroup => revisions.scalar_group,
            Self::FunctionStatements => revisions.function_statements,
            Self::IfCondition => revisions.if_condition,
            Self::Import => revisions.import,
            Self::AppendMembers => revisions.append_members,
            Self::MultiSite => revisions.multi_site,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuleRevisions {
    pub scalar: u32,
    pub expression_literals: u32,
    pub scalar_group: u32,
    pub function_statements: u32,
    pub if_condition: u32,
    pub import: u32,
    pub append_members: u32,
    pub multi_site: u32,
}

impl RuleRevisions {
    pub const CURRENT: Self = Self {
        scalar: rule_revisions::SCALAR,
        expression_literals: rule_revisions::EXPRESSION_LITERALS,
        scalar_group: rule_revisions::SCALAR_GROUP,
        function_statements: rule_revisions::FUNCTION_STATEMENTS,
        if_condition: rule_revisions::IF_CONDITION,
        import: rule_revisions::IMPORT,
        append_members: rule_revisions::APPEND_MEMBERS,
        multi_site: rule_revisions::MULTI_SITE,
    };
}

impl PatchRule {
    pub const fn kind(self) -> PatchRuleKind {
        match self {
            Self::Scalar { .. } => PatchRuleKind::Scalar,
            Self::ExpressionLiterals { .. } => PatchRuleKind::ExpressionLiterals,
            Self::ScalarGroup { .. } => PatchRuleKind::ScalarGroup,
            Self::FunctionStatements { .. } => PatchRuleKind::FunctionStatements,
            Self::IfCondition { .. } => PatchRuleKind::IfCondition,
            Self::Import { .. } => PatchRuleKind::Import,
            Self::AppendMembers { .. } => PatchRuleKind::AppendMembers,
            Self::MultiSite { .. } => PatchRuleKind::MultiSite,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ImplementationRevision(pub String);

impl ImplementationRevision {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ImplementationRevision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

pub(crate) fn compute_implementation_revision(
    patch_id: &str,
    variant: &PatchVariant,
    revisions: RuleRevisions,
) -> ImplementationRevision {
    let mut hasher = Sha256::new();
    hash_str(&mut hasher, "midnight-ui-patcher:impl-rev:v1");
    hash_str(&mut hasher, patch_id);
    hash_u16(&mut hasher, variant.major.0);
    hash_str(&mut hasher, variant.target);
    hash_rule(&mut hasher, variant.rule, revisions);

    let digest = hasher.finalize();
    let mut hex = String::with_capacity(7 + digest.len() * 2);
    hex.push_str("sha256:");
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in digest {
        hex.push(HEX[(byte >> 4) as usize] as char);
        hex.push(HEX[(byte & 0x0f) as usize] as char);
    }
    ImplementationRevision(hex)
}

fn hash_u8(hasher: &mut Sha256, value: u8) {
    hasher.update([value]);
}

fn hash_u16(hasher: &mut Sha256, value: u16) {
    hasher.update(value.to_be_bytes());
}

fn hash_u32(hasher: &mut Sha256, value: u32) {
    hasher.update(value.to_be_bytes());
}

fn hash_usize(hasher: &mut Sha256, value: usize) {
    hasher.update((value as u64).to_be_bytes());
}

fn hash_str(hasher: &mut Sha256, value: &str) {
    hash_usize(hasher, value.len());
    hasher.update(value.as_bytes());
}

fn hash_scope(hasher: &mut Sha256, scope: Option<ChildObjectScope>) {
    match scope {
        Some(scope) => {
            hash_u8(hasher, 1);
            hash_str(hasher, scope.class);
            hash_str(hasher, scope.property);
            hash_str(hasher, scope.expected);
        }
        None => hash_u8(hasher, 0),
    }
}

fn hash_patch_value(hasher: &mut Sha256, value: PatchValue) {
    match value {
        PatchValue::Literal(text) => {
            hash_u8(hasher, 0);
            hash_str(hasher, text);
        }
        PatchValue::Option(option) => {
            hash_u8(hasher, 1);
            hash_str(hasher, option.canonical_id());
            hash_str(hasher, option.kind().as_str());
            hash_str(hasher, &option.qml_format_signature());
        }
    }
}

fn hash_rule(hasher: &mut Sha256, rule: PatchRule, revisions: RuleRevisions) {
    let kind = rule.kind();
    hash_str(hasher, kind.as_str());
    hash_u32(hasher, kind.handler_revision(revisions));

    match rule {
        PatchRule::Scalar {
            root_id,
            scope,
            property,
            expected,
            replacement,
        } => {
            hash_str(hasher, root_id);
            hash_scope(hasher, scope);
            hash_str(hasher, property);
            hash_str(hasher, expected);
            hash_patch_value(hasher, replacement);
        }
        PatchRule::ExpressionLiterals {
            root_id,
            property,
            expected_literal,
            expected_count,
            required_identifiers,
            required_operators,
            replacement,
        } => {
            hash_str(hasher, root_id);
            hash_str(hasher, property);
            hash_str(hasher, expected_literal);
            hash_usize(hasher, expected_count);
            hash_usize(hasher, required_identifiers.len());
            for identifier in required_identifiers {
                hash_str(hasher, identifier);
            }
            hash_usize(hasher, required_operators.len());
            for operator in required_operators {
                hash_str(hasher, operator);
            }
            hash_patch_value(hasher, replacement);
        }
        PatchRule::ScalarGroup { root_id, targets } => {
            hash_str(hasher, root_id);
            hash_usize(hasher, targets.len());
            for target in targets {
                hash_scope(hasher, target.scope);
                hash_str(hasher, target.property);
                hash_str(hasher, target.expected);
                hash_patch_value(hasher, target.replacement);
            }
        }
        PatchRule::FunctionStatements { root_id, targets } => {
            hash_str(hasher, root_id);
            hash_usize(hasher, targets.len());
            for target in targets {
                hash_str(hasher, target.function);
                hash_str(hasher, target.expected);
                hash_str(hasher, target.replacement);
                hash_usize(hasher, target.expected_count);
            }
        }
        PatchRule::IfCondition {
            root_id,
            function,
            expected,
            replacement,
            expected_count,
        } => {
            hash_str(hasher, root_id);
            hash_str(hasher, function);
            hash_str(hasher, expected);
            hash_str(hasher, replacement);
            hash_usize(hasher, expected_count);
        }
        PatchRule::Import { module, version } => {
            hash_str(hasher, module);
            hash_str(hasher, version);
        }
        PatchRule::AppendMembers {
            root_id,
            anchor_property,
            expected,
            child_types,
            member_template,
            replacements,
        } => {
            hash_str(hasher, root_id);
            hash_str(hasher, anchor_property);
            hash_str(hasher, expected);
            hash_usize(hasher, child_types.len());
            for child_type in child_types {
                hash_str(hasher, child_type);
            }
            hash_str(hasher, member_template);
            hash_usize(hasher, replacements.len());
            for replacement in replacements {
                hash_patch_value(hasher, *replacement);
            }
        }
        PatchRule::MultiSite { sites } => {
            hash_usize(hasher, sites.len());
            for site in sites {
                hash_str(hasher, site.target);
                hash_rule(hasher, site.rule, revisions);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VariantSelectionKind {
    Direct,
    UpwardFallback { origin_major: WebOsMajor },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompatibilityStatus {
    MaintainerVerified,
    CommunityConfirmed,
    Unverified,
    KnownIncompatible,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceStatus {
    MaintainerVerified,
    CommunityConfirmed,
    KnownIncompatible,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisualConfirmationScope {
    NormalVisualStateOnly,
    ExtendedStatesChecked,
}

impl VisualConfirmationScope {
    pub const fn summary(self) -> &'static str {
        match self {
            Self::NormalVisualStateOnly => {
                "normal visual state confirmed (focus, disabled, click, and transition states not certified)"
            }
            Self::ExtendedStatesChecked => "extended visual states checked",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvidenceRecord {
    pub patch_id: &'static str,
    pub implementation_revision: &'static str,
    pub major: WebOsMajor,
    pub firmware_version: &'static str,
    pub model: Option<&'static str>,
    pub machine: Option<&'static str>,
    pub region: &'static str,
    pub status: EvidenceStatus,
    pub visual_scope: VisualConfirmationScope,
    pub report_ref: &'static str,
    pub superseded: bool,
}

#[derive(Clone, Debug)]
pub struct ResolvedPatch {
    pub patch: PatchSpec,
    pub variant: Option<PatchVariant>,
    pub selection: Option<VariantSelectionKind>,
    pub implementation_revision: Option<ImplementationRevision>,
    pub status: CompatibilityStatus,
    pub matching_evidence: Option<EvidenceRecord>,
}

pub fn evidence_matches_scope(record: &EvidenceRecord, identity: &TvIdentity) -> bool {
    if record.major != identity.major {
        return false;
    }

    let Some(firmware_version) = identity
        .raw
        .firmware_version
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return false;
    };
    if firmware_version != record.firmware_version {
        return false;
    }

    let Some(region) = identity
        .raw
        .region
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return false;
    };
    if !region.eq_ignore_ascii_case(record.region) {
        return false;
    }

    let model = identity
        .raw
        .model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let machine = identity
        .raw
        .machine
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    match (record.model, model) {
        (Some(expected_model), Some(actual_model)) => {
            actual_model == expected_model
                && record
                    .machine
                    .is_none_or(|expected_machine| machine == Some(expected_machine))
        }
        (None, None) => match (record.machine, machine) {
            (Some(expected_machine), Some(actual_machine)) => actual_machine == expected_machine,
            _ => false,
        },
        (Some(_), None) | (None, Some(_)) => false,
    }
}

pub(crate) fn resolve_patch_with(
    patch: &PatchSpec,
    identity: &TvIdentity,
    evidence: &[EvidenceRecord],
    revisions: RuleRevisions,
) -> ResolvedPatch {
    let Some((variant, selection)) = patch.resolve_variant(identity.major) else {
        return ResolvedPatch {
            patch: *patch,
            variant: None,
            selection: None,
            implementation_revision: None,
            status: CompatibilityStatus::Unavailable,
            matching_evidence: None,
        };
    };

    let revision = variant.implementation_revision_with_revisions(patch.id, revisions);
    let mut best_match: Option<EvidenceRecord> = None;

    for record in evidence {
        if record.superseded
            || record.patch_id != patch.id
            || record.implementation_revision != revision.as_str()
            || !evidence_matches_scope(record, identity)
        {
            continue;
        }
        let replace = match best_match.as_ref().map(|existing| existing.status) {
            None => true,
            Some(EvidenceStatus::KnownIncompatible) => false,
            Some(EvidenceStatus::MaintainerVerified) => {
                record.status == EvidenceStatus::KnownIncompatible
            }
            Some(EvidenceStatus::CommunityConfirmed) => {
                matches!(
                    record.status,
                    EvidenceStatus::KnownIncompatible | EvidenceStatus::MaintainerVerified
                )
            }
        };
        if replace {
            best_match = Some(*record);
        }
    }

    let status = match best_match.as_ref().map(|record| record.status) {
        Some(EvidenceStatus::KnownIncompatible) => CompatibilityStatus::KnownIncompatible,
        Some(EvidenceStatus::MaintainerVerified) => CompatibilityStatus::MaintainerVerified,
        Some(EvidenceStatus::CommunityConfirmed) => CompatibilityStatus::CommunityConfirmed,
        None => CompatibilityStatus::Unverified,
    };

    ResolvedPatch {
        patch: *patch,
        variant: Some(*variant),
        selection: Some(selection),
        implementation_revision: Some(revision),
        status,
        matching_evidence: best_match,
    }
}
