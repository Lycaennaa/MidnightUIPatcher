use super::{
    COMPATIBILITY_EVIDENCE, CompatibilityStatus, EvidenceRecord, EvidenceStatus, IdentityError,
    IdentitySource, PATCH_OPTION_SPECS, PATCHES, PatchArea, PatchRule, PatchRuleKind, PatchSite,
    PatchSpec, PatchValue, PatchVariant, RawTvIdentity, RuleRevisions, TARGETS, TvIdentity,
    VariantSelectionKind, VisualConfirmationScope, WebOsMajor, normalize_tv_identity,
    normalize_tv_identity_from_candidates, resolve_patch, resolve_patches, targets_for_major,
};

fn ids_for(area: PatchArea) -> Vec<&'static str> {
    PATCHES
        .iter()
        .filter(|patch| patch.area == area)
        .map(|patch| patch.id)
        .collect()
}

fn owner_baseline_identity() -> TvIdentity {
    normalize_tv_identity(
        RawTvIdentity {
            webos_release: Some("webOSTV 24".to_owned()),
            sdk_version: None,
            firmware_version: Some("33.22.75".to_owned()),
            model: None,
            machine: Some("O24".to_owned()),
            region: Some("GLOBAL".to_owned()),
            internal_release: Some("10.2.2-57".to_owned()),
        },
        IdentitySource::LiveTv,
    )
    .unwrap()
}

#[test]
fn tweaks_are_grouped_by_affected_area() {
    assert_eq!(
        ids_for(PatchArea::QuickSettings),
        vec![
            "main-panel-color",
            "main-panel-opacity",
            "main-panel-visibility",
            "menu-popup-color",
            "button-default-color",
            "button-idle-fill",
        ],
    );
    assert_eq!(
        ids_for(PatchArea::InputPicker),
        vec![
            "input-screen-backdrop",
            "input-tile-shadow",
            "input-home-hub",
            "input-live-tv",
            "input-active-signal",
            "input-category-text",
            "input-universal-control",
            "input-tile-background",
            "input-tile-icons",
            "input-tile-red-text",
            "input-tile-border",
        ],
    );
    assert_eq!(
        ids_for(PatchArea::InputBanner),
        vec![
            "input-banner-background",
            "input-banner-text-color",
            "input-banner-divider",
            "input-banner-menu-color",
        ],
    );
    assert_eq!(
        ids_for(PatchArea::NoSignal),
        vec![
            "no-signal-animation",
            "no-signal-movement",
            "no-signal-instructions",
            "no-signal-text-color",
        ],
    );
}

#[test]
fn patch_option_canonical_ids_match_specs() {
    for spec in PATCH_OPTION_SPECS {
        let variant = PATCHES
            .iter()
            .find(|patch| patch.id == spec.patch_id)
            .expect("option spec must reference a valid patch");
        assert!(!variant.primary_target().is_empty());
        assert_eq!(spec.id.is_empty(), false);
    }
    assert_eq!(targets_for_major(WebOsMajor::WEBOS_24), TARGETS.to_vec());
}

#[test]
fn baseline_evidence_covers_all_patches_and_matches_current_revisions() {
    let mut expected_pairs = Vec::new();
    for patch in &PATCHES {
        let variant = patch
            .variant(WebOsMajor::WEBOS_24)
            .expect("every baseline tweak must define a webOS 24 variant");
        let rev = variant.implementation_revision(patch.id);
        expected_pairs.push((patch.id, rev));
    }

    assert_eq!(COMPATIBILITY_EVIDENCE.len(), PATCHES.len());
    let mismatches = expected_pairs
        .iter()
        .filter_map(|(id, rev)| {
            let record = COMPATIBILITY_EVIDENCE
                .iter()
                .find(|record| record.patch_id == *id);
            match record {
                Some(record) if record.implementation_revision == rev.as_str() => None,
                _ => Some(format!(
                    "    baseline_evidence(\"{id}\", \"{}\"),",
                    rev.as_str()
                )),
            }
        })
        .collect::<Vec<_>>();
    assert!(
        mismatches.is_empty(),
        "baseline evidence revisions out of sync:\n{}",
        mismatches.join("\n")
    );

    let baseline = owner_baseline_identity();
    let resolved = resolve_patches(&baseline);
    assert_eq!(resolved.len(), PATCHES.len());
    for item in resolved {
        assert_eq!(item.selection, Some(VariantSelectionKind::Direct));
        assert_eq!(item.status, CompatibilityStatus::MaintainerVerified);
        let evidence = item
            .matching_evidence
            .expect("baseline evidence must match");
        assert_eq!(
            evidence.visual_scope,
            VisualConfirmationScope::NormalVisualStateOnly
        );
        assert_eq!(evidence.firmware_version, "33.22.75");
        assert_eq!(evidence.region, "GLOBAL");
    }
}

#[test]
fn identity_normalization_never_infers_major_from_numeric_firmware() {
    for (label, expected) in [
        ("webOSTV 24", WebOsMajor::WEBOS_24),
        ("webOS TV 24", WebOsMajor::WEBOS_24),
        ("webOS 24", WebOsMajor::WEBOS_24),
        ("webOS24", WebOsMajor::WEBOS_24),
        ("webostv-25", WebOsMajor::WEBOS_25),
        ("26", WebOsMajor::WEBOS_26),
        ("27", WebOsMajor(27)),
    ] {
        assert_eq!(WebOsMajor::parse_release_label(label), Ok(expected));
    }

    for invalid in [
        "33.22.75",
        "33.25.73",
        "10.2.2-57",
        "9.0.0",
        "24.0.0",
        "O24",
        "GLOBAL",
        "webOS 0",
        "webOS 024",
        "024",
        "webOS 24 extra",
    ] {
        assert!(
            matches!(
                WebOsMajor::parse_release_label(invalid),
                Err(IdentityError::UnsupportedReleaseLabel(_))
            ),
            "unexpectedly parsed `{invalid}` as a webOS major"
        );
    }

    let missing_major = normalize_tv_identity(
        RawTvIdentity {
            webos_release: None,
            sdk_version: Some("24.0.0".to_owned()),
            firmware_version: Some("33.22.75".to_owned()),
            model: Some("OLED55".to_owned()),
            machine: Some("O24".to_owned()),
            region: Some("GLOBAL".to_owned()),
            internal_release: Some("10.2.2-57".to_owned()),
        },
        IdentitySource::LiveTv,
    );
    assert_eq!(missing_major, Err(IdentityError::MissingWebOsMajor));

    let id_a = normalize_tv_identity(
        RawTvIdentity {
            webos_release: Some("webOSTV 24".to_owned()),
            firmware_version: Some("33.22.75".to_owned()),
            ..RawTvIdentity::default()
        },
        IdentitySource::LiveTv,
    )
    .unwrap();
    let id_b = normalize_tv_identity(
        RawTvIdentity {
            webos_release: Some("webOSTV 24".to_owned()),
            firmware_version: Some("99.99.99".to_owned()),
            ..RawTvIdentity::default()
        },
        IdentitySource::LiveTv,
    )
    .unwrap();
    assert_eq!(id_a.major, WebOsMajor::WEBOS_24);
    assert_eq!(id_b.major, WebOsMajor::WEBOS_24);
    assert_eq!(id_a.raw.firmware_version.as_deref(), Some("33.22.75"));
    assert_eq!(id_b.raw.firmware_version.as_deref(), Some("99.99.99"));

    let conflict = normalize_tv_identity_from_candidates(
        RawTvIdentity {
            webos_release: Some("webOSTV 24".to_owned()),
            ..RawTvIdentity::default()
        },
        IdentitySource::LiveTv,
        &["webOS 25"],
    );
    assert_eq!(
        conflict,
        Err(IdentityError::ConflictingWebOsMajor {
            first: WebOsMajor::WEBOS_24,
            second: WebOsMajor::WEBOS_25,
        })
    );
}

#[test]
fn implementation_revision_invalidates_only_changed_rule_or_handler_not_labels_or_options() {
    let baseline = owner_baseline_identity();
    let original_patch = PATCHES
        .iter()
        .find(|patch| patch.id == "main-panel-color")
        .unwrap();
    let original_variant = original_patch.variant(WebOsMajor::WEBOS_24).unwrap();
    let original_rev = original_variant.implementation_revision(original_patch.id);

    // Label/area-only change preserves the exact implementation revision and evidence.
    let relabeled = PatchSpec {
        area: PatchArea::NoSignal,
        id: original_patch.id,
        label: "Completely different UI label",
        variants: original_patch.variants,
    };
    assert_eq!(
        relabeled
            .variant(WebOsMajor::WEBOS_24)
            .unwrap()
            .implementation_revision(relabeled.id),
        original_rev
    );
    assert_eq!(
        relabeled.resolve(&baseline).status,
        CompatibilityStatus::MaintainerVerified
    );

    // Rule change invalidates prior evidence for the changed patch.
    const MODIFIED_VARIANTS: [PatchVariant; 1] = [PatchVariant::new(
        WebOsMajor::WEBOS_24,
        TARGETS[0],
        PatchRule::Scalar {
            root_id: "mainLayout",
            scope: None,
            property: "color",
            expected: "\"#000000\"",
            replacement: PatchValue::Literal("\"#111111\""),
        },
    )];
    let modified_patch = PatchSpec {
        area: original_patch.area,
        id: original_patch.id,
        label: original_patch.label,
        variants: &MODIFIED_VARIANTS,
    };
    assert_ne!(
        modified_patch
            .variant(WebOsMajor::WEBOS_24)
            .unwrap()
            .implementation_revision(modified_patch.id),
        original_rev
    );
    assert_eq!(
        modified_patch.resolve(&baseline).status,
        CompatibilityStatus::Unverified
    );

    // Bumping shared Scalar handler invalidates Scalar patches, but leaves
    // ExpressionLiterals / ScalarGroup / FunctionStatements / IfCondition patches verified.
    let mut bumped_scalar = RuleRevisions::CURRENT;
    bumped_scalar.scalar += 1;

    let scalar_patch = PATCHES.iter().find(|p| p.id == "main-panel-color").unwrap();
    let expr_patch = PATCHES.iter().find(|p| p.id == "button-idle-fill").unwrap();
    let group_patch = PATCHES.iter().find(|p| p.id == "input-tile-icons").unwrap();
    let fn_patch = PATCHES.iter().find(|p| p.id == "input-live-tv").unwrap();
    let if_patch = PATCHES
        .iter()
        .find(|p| p.id == "input-active-signal")
        .unwrap();
    let multi_fn_patch = PATCHES.iter().find(|p| p.id == "input-home-hub").unwrap();
    let multi_scalar_patch = PATCHES
        .iter()
        .find(|p| p.id == "input-banner-text-color")
        .unwrap();

    assert_eq!(
        scalar_patch
            .resolve_with(&baseline, &COMPATIBILITY_EVIDENCE, bumped_scalar)
            .status,
        CompatibilityStatus::Unverified
    );
    assert_eq!(
        multi_scalar_patch
            .resolve_with(&baseline, &COMPATIBILITY_EVIDENCE, bumped_scalar)
            .status,
        CompatibilityStatus::Unverified
    );
    for unaffected in [expr_patch, group_patch, fn_patch, if_patch, multi_fn_patch] {
        assert_eq!(
            unaffected
                .resolve_with(&baseline, &COMPATIBILITY_EVIDENCE, bumped_scalar)
                .status,
            CompatibilityStatus::MaintainerVerified,
            "patch `{}` should not be invalidated by Scalar handler bump",
            unaffected.id
        );
    }

    // Bumping FunctionStatements invalidates MultiSite patches containing FunctionStatements.
    let mut bumped_fn = RuleRevisions::CURRENT;
    bumped_fn.function_statements += 1;
    assert_eq!(
        multi_fn_patch
            .resolve_with(&baseline, &COMPATIBILITY_EVIDENCE, bumped_fn)
            .status,
        CompatibilityStatus::Unverified
    );
    assert_eq!(
        scalar_patch
            .resolve_with(&baseline, &COMPATIBILITY_EVIDENCE, bumped_fn)
            .status,
        CompatibilityStatus::MaintainerVerified
    );
}

#[test]
fn variant_resolution_supports_direct_upward_fallback_and_forbids_downward() {
    const MULTI_VARIANTS: [PatchVariant; 2] = [
        PatchVariant::new(
            WebOsMajor::WEBOS_24,
            TARGETS[0],
            PatchRule::Import {
                module: "Test",
                version: "1.0",
            },
        ),
        PatchVariant::new(
            WebOsMajor::WEBOS_26,
            TARGETS[0],
            PatchRule::Import {
                module: "Test",
                version: "2.0",
            },
        ),
    ];
    let multi_patch = PatchSpec {
        area: PatchArea::QuickSettings,
        id: "synthetic-multi-major",
        label: "Synthetic multi-major tweak",
        variants: &MULTI_VARIANTS,
    };

    const ONLY_26_VARIANTS: [PatchVariant; 1] = [PatchVariant::new(
        WebOsMajor::WEBOS_26,
        TARGETS[0],
        PatchRule::Import {
            module: "Only26",
            version: "1.0",
        },
    )];
    let only_26_patch = PatchSpec {
        area: PatchArea::QuickSettings,
        id: "synthetic-26-only",
        label: "Synthetic 26-only tweak",
        variants: &ONLY_26_VARIANTS,
    };

    assert_eq!(
        multi_patch
            .resolve_variant(WebOsMajor(23))
            .map(|(v, k)| (v.major, k)),
        None
    );
    assert_eq!(
        multi_patch
            .resolve_variant(WebOsMajor::WEBOS_24)
            .map(|(v, k)| (v.major, k)),
        Some((WebOsMajor::WEBOS_24, VariantSelectionKind::Direct))
    );
    assert_eq!(
        multi_patch
            .resolve_variant(WebOsMajor::WEBOS_25)
            .map(|(v, k)| (v.major, k)),
        Some((
            WebOsMajor::WEBOS_24,
            VariantSelectionKind::UpwardFallback {
                origin_major: WebOsMajor::WEBOS_24,
            }
        ))
    );
    assert_eq!(
        multi_patch
            .resolve_variant(WebOsMajor::WEBOS_26)
            .map(|(v, k)| (v.major, k)),
        Some((WebOsMajor::WEBOS_26, VariantSelectionKind::Direct))
    );
    assert_eq!(
        multi_patch
            .resolve_variant(WebOsMajor(27))
            .map(|(v, k)| (v.major, k)),
        Some((
            WebOsMajor::WEBOS_26,
            VariantSelectionKind::UpwardFallback {
                origin_major: WebOsMajor::WEBOS_26,
            }
        ))
    );

    // 26-only feature is unavailable on 24 and 25 (no downward fallback).
    assert!(
        only_26_patch
            .resolve_variant(WebOsMajor::WEBOS_24)
            .is_none()
    );
    assert!(
        only_26_patch
            .resolve_variant(WebOsMajor::WEBOS_25)
            .is_none()
    );
    assert_eq!(
        only_26_patch
            .resolve_variant(WebOsMajor::WEBOS_26)
            .map(|(v, k)| (v.major, k)),
        Some((WebOsMajor::WEBOS_26, VariantSelectionKind::Direct))
    );
}

#[test]
fn evidence_scoping_and_conflict_resolution_enforce_exact_scope() {
    let other_model = normalize_tv_identity(
        RawTvIdentity {
            webos_release: Some("webOSTV 24".to_owned()),
            firmware_version: Some("33.22.75".to_owned()),
            model: Some("OLED65G4".to_owned()),
            machine: Some("O24".to_owned()),
            region: Some("GLOBAL".to_owned()),
            ..RawTvIdentity::default()
        },
        IdentitySource::LiveTv,
    )
    .unwrap();
    let other_firmware = normalize_tv_identity(
        RawTvIdentity {
            webos_release: Some("webOSTV 24".to_owned()),
            firmware_version: Some("33.31.68".to_owned()),
            machine: Some("O24".to_owned()),
            region: Some("GLOBAL".to_owned()),
            ..RawTvIdentity::default()
        },
        IdentitySource::LiveTv,
    )
    .unwrap();
    let missing_firmware = normalize_tv_identity(
        RawTvIdentity {
            webos_release: Some("webOSTV 24".to_owned()),
            firmware_version: None,
            machine: Some("O24".to_owned()),
            region: Some("GLOBAL".to_owned()),
            ..RawTvIdentity::default()
        },
        IdentitySource::LiveTv,
    )
    .unwrap();
    let other_region = normalize_tv_identity(
        RawTvIdentity {
            webos_release: Some("webOSTV 24".to_owned()),
            firmware_version: Some("33.22.75".to_owned()),
            machine: Some("O24".to_owned()),
            region: Some("EU".to_owned()),
            ..RawTvIdentity::default()
        },
        IdentitySource::LiveTv,
    )
    .unwrap();

    for unverified_identity in [
        &other_model,
        &other_firmware,
        &missing_firmware,
        &other_region,
    ] {
        let resolved = resolve_patch("main-panel-color", unverified_identity).unwrap();
        assert_eq!(resolved.status, CompatibilityStatus::Unverified);
    }

    // Conflict resolution: KnownIncompatible supersedes CommunityConfirmed/MaintainerVerified.
    let baseline = owner_baseline_identity();
    let patch = PATCHES.iter().find(|p| p.id == "main-panel-color").unwrap();
    let rev_str = COMPATIBILITY_EVIDENCE
        .iter()
        .find(|record| record.patch_id == patch.id)
        .unwrap()
        .implementation_revision;
    let conflicting_records = [
        EvidenceRecord {
            patch_id: patch.id,
            implementation_revision: rev_str,
            major: WebOsMajor::WEBOS_24,
            firmware_version: "33.22.75",
            model: None,
            machine: Some("O24"),
            region: "GLOBAL",
            status: EvidenceStatus::CommunityConfirmed,
            visual_scope: VisualConfirmationScope::NormalVisualStateOnly,
            report_ref: "report-success",
            superseded: false,
        },
        EvidenceRecord {
            patch_id: patch.id,
            implementation_revision: rev_str,
            major: WebOsMajor::WEBOS_24,
            firmware_version: "33.22.75",
            model: None,
            machine: Some("O24"),
            region: "GLOBAL",
            status: EvidenceStatus::KnownIncompatible,
            visual_scope: VisualConfirmationScope::NormalVisualStateOnly,
            report_ref: "report-core-failure",
            superseded: false,
        },
    ];
    let resolved = patch.resolve_with(&baseline, &conflicting_records, RuleRevisions::CURRENT);
    assert_eq!(resolved.status, CompatibilityStatus::KnownIncompatible);
    assert_eq!(
        resolved.matching_evidence.unwrap().report_ref,
        "report-core-failure"
    );

    // Future-major fallback community confirmation preserves fallback origin.
    let webos_27 = normalize_tv_identity(
        RawTvIdentity {
            webos_release: Some("webOSTV 27".to_owned()),
            firmware_version: Some("40.00.01".to_owned()),
            model: Some("OLED55C7".to_owned()),
            machine: Some("O27".to_owned()),
            region: Some("GLOBAL".to_owned()),
            ..RawTvIdentity::default()
        },
        IdentitySource::LiveTv,
    )
    .unwrap();
    let fallback_confirmed = [EvidenceRecord {
        patch_id: patch.id,
        implementation_revision: rev_str,
        major: WebOsMajor(27),
        firmware_version: "40.00.01",
        model: Some("OLED55C7"),
        machine: Some("O27"),
        region: "GLOBAL",
        status: EvidenceStatus::CommunityConfirmed,
        visual_scope: VisualConfirmationScope::NormalVisualStateOnly,
        report_ref: "community-27-report",
        superseded: false,
    }];
    let resolved_27 = patch.resolve_with(&webos_27, &fallback_confirmed, RuleRevisions::CURRENT);
    assert_eq!(
        resolved_27.selection,
        Some(VariantSelectionKind::UpwardFallback {
            origin_major: WebOsMajor::WEBOS_24
        })
    );
    assert_eq!(resolved_27.status, CompatibilityStatus::CommunityConfirmed);
    assert_eq!(
        resolved_27.variant.unwrap().rule_kind(),
        PatchRuleKind::Scalar
    );

    // Partial identity omitting `machine` must not match an evidence record requiring `machine`.
    let webos_27_missing_machine = normalize_tv_identity(
        RawTvIdentity {
            webos_release: Some("webOSTV 27".to_owned()),
            firmware_version: Some("40.00.01".to_owned()),
            model: Some("OLED55C7".to_owned()),
            machine: None,
            region: Some("GLOBAL".to_owned()),
            ..RawTvIdentity::default()
        },
        IdentitySource::LiveTv,
    )
    .unwrap();
    let resolved_partial = patch.resolve_with(
        &webos_27_missing_machine,
        &fallback_confirmed,
        RuleRevisions::CURRENT,
    );
    assert_eq!(resolved_partial.status, CompatibilityStatus::Unverified);
}

#[test]
fn import_append_members_and_superseded_evidence_work_as_specified() {
    let baseline = owner_baseline_identity();
    let menu_patch = PATCHES
        .iter()
        .find(|patch| patch.id == "input-banner-menu-color")
        .unwrap();
    let scalar_patch = PATCHES
        .iter()
        .find(|patch| patch.id == "main-panel-color")
        .unwrap();

    let mut bumped_import = RuleRevisions::CURRENT;
    bumped_import.import += 1;
    assert_eq!(
        menu_patch
            .resolve_with(&baseline, &COMPATIBILITY_EVIDENCE, bumped_import)
            .status,
        CompatibilityStatus::Unverified
    );
    assert_eq!(
        scalar_patch
            .resolve_with(&baseline, &COMPATIBILITY_EVIDENCE, bumped_import)
            .status,
        CompatibilityStatus::MaintainerVerified
    );

    let mut bumped_append = RuleRevisions::CURRENT;
    bumped_append.append_members += 1;
    assert_eq!(
        menu_patch
            .resolve_with(&baseline, &COMPATIBILITY_EVIDENCE, bumped_append)
            .status,
        CompatibilityStatus::Unverified
    );
    assert_eq!(
        scalar_patch
            .resolve_with(&baseline, &COMPATIBILITY_EVIDENCE, bumped_append)
            .status,
        CompatibilityStatus::MaintainerVerified
    );

    // Superseded records are ignored when resolving active status.
    let rev_str = COMPATIBILITY_EVIDENCE
        .iter()
        .find(|record| record.patch_id == scalar_patch.id)
        .unwrap()
        .implementation_revision;
    let superseded_records = [
        EvidenceRecord {
            patch_id: scalar_patch.id,
            implementation_revision: rev_str,
            major: WebOsMajor::WEBOS_24,
            firmware_version: "33.22.75",
            model: None,
            machine: Some("O24"),
            region: "GLOBAL",
            status: EvidenceStatus::KnownIncompatible,
            visual_scope: VisualConfirmationScope::NormalVisualStateOnly,
            report_ref: "retracted-failure",
            superseded: true,
        },
        EvidenceRecord {
            patch_id: scalar_patch.id,
            implementation_revision: rev_str,
            major: WebOsMajor::WEBOS_24,
            firmware_version: "33.22.75",
            model: None,
            machine: Some("O24"),
            region: "GLOBAL",
            status: EvidenceStatus::CommunityConfirmed,
            visual_scope: VisualConfirmationScope::NormalVisualStateOnly,
            report_ref: "active-success",
            superseded: false,
        },
    ];
    let resolved =
        scalar_patch.resolve_with(&baseline, &superseded_records, RuleRevisions::CURRENT);
    assert_eq!(resolved.status, CompatibilityStatus::CommunityConfirmed);
    assert_eq!(
        resolved.matching_evidence.unwrap().report_ref,
        "active-success"
    );
}

#[test]
fn legacy_user_config_and_patch_options_deserialize_without_version_fields() {
    let legacy_toml = r##"
ssh_target = "root@tv.local"
model = "OLED55"
firmware_label = "webOS 24"

[patch_options]
main_panel_color = "#112233"
input_tile_text_color = "#445566"
"##;
    let loaded: crate::config::UserConfig = toml::from_str(legacy_toml).unwrap();
    assert_eq!(loaded.patch_options.main_panel_color, "#112233");
    assert_eq!(loaded.patch_options.input_tile_text_color, "#445566");
    loaded.patch_options.validate().unwrap();
}

#[test]
fn multisite_variant_affected_targets_deduplicates_sites_and_catalog_invariants_hold() {
    const SITES: [PatchSite; 3] = [
        PatchSite {
            target: TARGETS[9],
            rule: PatchRule::Import {
                module: "A",
                version: "1.0",
            },
        },
        PatchSite {
            target: TARGETS[9],
            rule: PatchRule::Import {
                module: "B",
                version: "1.0",
            },
        },
        PatchSite {
            target: TARGETS[10],
            rule: PatchRule::Import {
                module: "C",
                version: "1.0",
            },
        },
    ];
    let variant = PatchVariant::new(
        WebOsMajor::WEBOS_24,
        TARGETS[8],
        PatchRule::MultiSite { sites: &SITES },
    );
    assert_eq!(
        variant.affected_targets().collect::<Vec<_>>(),
        vec![TARGETS[9], TARGETS[10]]
    );

    for patch in &PATCHES {
        assert!(
            !patch.variants.is_empty(),
            "patch `{}` must declare at least one variant",
            patch.id
        );
        let mut majors = Vec::new();
        for variant in patch.variants {
            assert!(
                !majors.contains(&variant.major),
                "patch `{}` has duplicate variant major {:?}",
                patch.id,
                variant.major
            );
            majors.push(variant.major);
            if let PatchRule::MultiSite { sites } = variant.rule {
                assert!(
                    !sites.is_empty(),
                    "patch `{}` MultiSite variant must declare at least one site",
                    patch.id
                );
                assert!(
                    sites.iter().any(|site| site.target == variant.target),
                    "patch `{}` MultiSite primary target must match one of its sites",
                    patch.id
                );
            }
        }
    }
}
