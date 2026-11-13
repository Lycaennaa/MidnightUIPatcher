#[path = "support/update_migration.rs"]
mod support;

use std::collections::BTreeSet;
use std::fs;

use midnight_ui_patcher::patches::TARGETS;
use support::{
    CURRENT_TARGETS, PREVIOUS_TARGETS, assert_original_backups, assert_original_backups_in,
    assert_overlay_targets, assert_preupdate_snapshot, assert_preupdate_snapshot_in,
    build_scenario, direct_backup_targets, expected_mount_targets, expected_original_targets,
    failed_state_dir, invalid_target_manifests, mount_targets,
    old_targets_for_added_targets_migration, replace_package_target_manifest, run_update,
    verify_backup_manifest, verify_backup_manifest_in,
};

#[test]
fn old_target_sets_migrate_to_current_targets_and_rollback_on_failure() {
    for old_targets in [&PREVIOUS_TARGETS[..], &CURRENT_TARGETS[..], &TARGETS[..]] {
        let success = build_scenario(old_targets, false);
        assert_eq!(
            mount_targets(&success),
            expected_mount_targets(&success, &success.old_targets)
        );
        assert_overlay_targets(&success, &success.old_targets, "old-overlay");
        let output = run_update(&success);
        assert!(
            output.status.success(),
            "stdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let state = success.root.join("webosbrew/black-theme");
        assert_eq!(
            fs::read_to_string(state.join("TARGET-SHA256SUMS")).unwrap(),
            success.current_manifest
        );
        assert!(
            success
                .root
                .join("run")
                .join("black-theme-overlay.active")
                .is_file()
        );
        assert_eq!(
            mount_targets(&success),
            expected_mount_targets(&success, &TARGETS)
        );
        assert_overlay_targets(&success, &TARGETS, "new-overlay");
        assert_original_backups(&success, &expected_original_targets(&success));
        assert_preupdate_snapshot(&success);

        let failed = build_scenario(old_targets, true);
        assert_eq!(
            mount_targets(&failed),
            expected_mount_targets(&failed, &failed.old_targets)
        );
        assert_overlay_targets(&failed, &failed.old_targets, "old-overlay");
        let output = run_update(&failed);
        assert!(
            !output.status.success(),
            "activation failure unexpectedly succeeded; stdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let state = failed.root.join("webosbrew/black-theme");
        assert_eq!(
            fs::read_to_string(state.join("TARGET-SHA256SUMS")).unwrap(),
            failed.old_manifest
        );
        assert!(
            failed
                .root
                .join("run")
                .join("black-theme-overlay.active")
                .is_file()
        );
        assert_eq!(
            mount_targets(&failed),
            expected_mount_targets(&failed, &failed.old_targets)
        );
        assert_overlay_targets(&failed, &failed.old_targets, "old-overlay");
        assert_original_backups(&failed, &failed.old_targets);
        verify_backup_manifest(&failed);
        let failed_recovery = failed_state_dir(&failed);
        let expected_originals = expected_original_targets(&failed);
        assert_original_backups_in(&failed_recovery, &expected_originals);
        assert_preupdate_snapshot_in(&failed, &failed_recovery);
        verify_backup_manifest_in(&failed, &failed_recovery);
    }
}

#[test]
fn migration_captures_added_targets_and_restores_them_on_failure() {
    let old_targets = old_targets_for_added_targets_migration();
    let added_targets = &TARGETS[TARGETS.len() - 2..];
    assert!(
        added_targets
            .iter()
            .all(|target| !old_targets.contains(target))
    );
    let scenario = build_scenario(&old_targets, false);
    assert_eq!(
        mount_targets(&scenario),
        expected_mount_targets(&scenario, &old_targets)
    );
    assert_overlay_targets(&scenario, &old_targets, "old-overlay");

    let initial_backup_targets = old_targets
        .iter()
        .map(|target| (*target).to_owned())
        .collect::<BTreeSet<_>>();
    assert_eq!(direct_backup_targets(&scenario), initial_backup_targets);
    for added_target in added_targets.iter().copied() {
        assert!(
            !mount_targets(&scenario).contains(&format!("{}/{added_target}", scenario.qml)),
            "newly added target must begin unmounted"
        );
        assert_eq!(
            fs::read_to_string(scenario.root.join("qml").join(added_target)).unwrap(),
            format!("original:{added_target}\n")
        );
    }

    let output = run_update(&scenario);
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let state = scenario.root.join("webosbrew/black-theme");
    assert_eq!(
        fs::read_to_string(state.join("TARGET-SHA256SUMS")).unwrap(),
        scenario.current_manifest
    );
    assert_eq!(
        mount_targets(&scenario),
        expected_mount_targets(&scenario, &TARGETS)
    );
    assert_overlay_targets(&scenario, &TARGETS, "new-overlay");
    assert_original_backups(&scenario, &expected_original_targets(&scenario));
    assert_preupdate_snapshot(&scenario);
    let failed = build_scenario(&old_targets, true);
    let output = run_update(&failed);
    assert!(
        !output.status.success(),
        "new-target activation failure unexpectedly succeeded; stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let state = failed.root.join("webosbrew/black-theme");
    assert_eq!(
        fs::read_to_string(state.join("TARGET-SHA256SUMS")).unwrap(),
        failed.old_manifest
    );
    assert_eq!(
        mount_targets(&failed),
        expected_mount_targets(&failed, &old_targets)
    );
    assert_overlay_targets(&failed, &old_targets, "old-overlay");
    assert_original_backups(&failed, &old_targets);
    verify_backup_manifest(&failed);
    let failed_recovery = failed_state_dir(&failed);
    let expected_originals = expected_original_targets(&failed);
    assert_original_backups_in(&failed_recovery, &expected_originals);
    assert_preupdate_snapshot_in(&failed, &failed_recovery);
    verify_backup_manifest_in(&failed, &failed_recovery);

    let retry = run_update(&failed);
    assert!(
        !retry.status.success(),
        "update unexpectedly proceeded with unresolved recovery backups"
    );
    assert!(
        String::from_utf8_lossy(&retry.stderr).contains("Unresolved update recovery path found")
    );
    assert_eq!(
        fs::read_to_string(state.join("TARGET-SHA256SUMS")).unwrap(),
        failed.old_manifest
    );
    assert_eq!(
        fs::read_to_string(state.join("BACKUP-SHA256SUMS")).unwrap(),
        failed.backup_manifest
    );
    assert_eq!(
        mount_targets(&failed),
        expected_mount_targets(&failed, &old_targets)
    );
    assert_original_backups(&failed, &old_targets);
    assert_original_backups_in(&failed_recovery, &expected_originals);
    assert_preupdate_snapshot_in(&failed, &failed_recovery);
    verify_backup_manifest(&failed);
    verify_backup_manifest_in(&failed, &failed_recovery);
}

#[test]
fn malformed_duplicate_and_unsafe_package_target_manifests_are_rejected_unchanged() {
    let old_targets = old_targets_for_added_targets_migration();
    let scenario = build_scenario(&old_targets, false);
    let invalid_manifests = invalid_target_manifests(&scenario);

    for (kind, manifest) in invalid_manifests {
        replace_package_target_manifest(&scenario, &manifest);
        let output = run_update(&scenario);
        assert!(
            !output.status.success(),
            "{kind} target manifest unexpectedly succeeded; stdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !String::from_utf8_lossy(&output.stderr)
                .contains("Update bundle checksum verification failed"),
            "{kind} manifest was not tested past bundle checksum verification"
        );

        let state = scenario.root.join("webosbrew/black-theme");
        assert_eq!(
            fs::read_to_string(state.join("TARGET-SHA256SUMS")).unwrap(),
            scenario.old_manifest
        );
        assert_eq!(
            fs::read_to_string(state.join("BACKUP-SHA256SUMS")).unwrap(),
            scenario.backup_manifest
        );
        assert_eq!(
            mount_targets(&scenario),
            expected_mount_targets(&scenario, &scenario.old_targets)
        );
        assert!(
            scenario
                .root
                .join("run/black-theme-overlay.active")
                .is_file()
        );
        assert_overlay_targets(&scenario, &scenario.old_targets, "old-overlay");
        let original_targets = scenario
            .old_targets
            .iter()
            .map(|target| (*target).to_owned())
            .collect::<BTreeSet<_>>();
        assert_eq!(direct_backup_targets(&scenario), original_targets);
        for target in &scenario.old_targets {
            assert_eq!(
                fs::read_to_string(state.join("backups").join(*target)).unwrap(),
                format!("original:{target}\n")
            );
        }
        verify_backup_manifest(&scenario);
        assert!(
            !fs::read_dir(state.join("backups"))
                .unwrap()
                .filter_map(Result::ok)
                .any(|entry| entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("pre-update-"))
        );
    }
}
