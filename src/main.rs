use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use midnight_ui_patcher::patcher::{patch_qml, selected_patch_ids};
use midnight_ui_patcher::patches::{PATCHES, TARGETS};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args
        .first()
        .is_some_and(|arg| arg == "--verify-local-preview")
    {
        let root = args
            .get(1)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        if let Err(error) = verify_local_preview(&root) {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }

    #[cfg(feature = "desktop")]
    midnight_ui_patcher::ui::run();

    #[cfg(not(feature = "desktop"))]
    eprintln!("Use --verify-local-preview [project-root], or build with the desktop feature.");
}

fn verify_local_preview(root: &Path) -> Result<(), String> {
    let mut sources = BTreeMap::new();
    for target in TARGETS {
        let path = root.join("preview/base").join(target);
        let source = fs::read_to_string(&path)
            .map_err(|_| format!("Cannot read local source file: {}", path.display()))?;
        sources.insert(target.to_owned(), source);
    }
    let selected = selected_patch_ids(PATCHES.iter().map(|patch| patch.id))
        .map_err(|error| error.to_string())?;
    let output = patch_qml(&sources, &selected).map_err(|error| error.to_string())?;
    for target in TARGETS {
        let overlay_path = root.join("preview/overlay").join(target);
        let expected = fs::read_to_string(&overlay_path).map_err(|_| {
            format!(
                "Cannot read local comparison file: {}",
                overlay_path.display()
            )
        })?;
        if output.files.get(target) != Some(&expected) {
            return Err(format!(
                "Structural patch output differs from the local overlay: {target}"
            ));
        }
    }
    println!(
        "Validated {} structural edits against all {} local target files.",
        output.applied.len(),
        TARGETS.len()
    );
    Ok(())
}
