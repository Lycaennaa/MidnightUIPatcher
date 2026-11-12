mod qml;
#[cfg(test)]
mod tests;

use self::qml::QmlDoc;
use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

use crate::patches::{PATCHES, PatchRule, PatchSpec, TARGETS};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PatchError {
    #[error("missing target file: {0}")]
    MissingTarget(String),
    #[error("unsupported QML structure in {file}: {detail}")]
    Unsupported { file: String, detail: String },
    #[error("unknown patch identifier: {0}")]
    UnknownPatch(String),
    #[error("select at least one patch")]
    EmptySelection,
}

#[derive(Debug, Clone)]
pub struct PatchOutput {
    pub files: BTreeMap<String, String>,
    pub applied: Vec<&'static PatchSpec>,
}

fn unsupported(file: &str, detail: &str) -> PatchError {
    PatchError::Unsupported {
        file: file.to_owned(),
        detail: detail.to_owned(),
    }
}

#[derive(Clone)]
struct Edit {
    start: usize,
    end: usize,
    replacement: String,
}

fn append_site_edits(
    edits: &mut BTreeMap<String, Vec<Edit>>,
    doc: &QmlDoc<'_>,
    rule: PatchRule,
    target: &str,
    selected: bool,
) -> Result<(), PatchError> {
    let mut patch_edits = locate_patch(doc, rule, target)?;
    if selected {
        edits
            .entry(target.to_owned())
            .or_default()
            .append(&mut patch_edits);
    }
    Ok(())
}

fn collect_patch_edits(
    docs: &BTreeMap<String, QmlDoc<'_>>,
    patches: &[PatchSpec],
    selected: &BTreeSet<String>,
) -> Result<BTreeMap<String, Vec<Edit>>, PatchError> {
    let mut edits: BTreeMap<String, Vec<Edit>> = BTreeMap::new();
    for patch in patches {
        let is_selected = selected.contains(patch.id);
        match patch.rule {
            PatchRule::MultiSite { sites } => {
                if sites.is_empty() {
                    return Err(unsupported(
                        patch.target,
                        "multi-site patch has no target sites",
                    ));
                }
                for site in sites {
                    append_site_edits(
                        &mut edits,
                        &docs[site.target],
                        site.rule,
                        site.target,
                        is_selected,
                    )?;
                }
            }
            rule => append_site_edits(
                &mut edits,
                &docs[patch.target],
                rule,
                patch.target,
                is_selected,
            )?,
        }
    }
    Ok(edits)
}

pub fn patch_qml(
    sources: &BTreeMap<String, String>,
    selected: &BTreeSet<String>,
) -> Result<PatchOutput, PatchError> {
    if selected.is_empty() {
        return Err(PatchError::EmptySelection);
    }
    for id in selected {
        if !PATCHES.iter().any(|patch| patch.id == id) {
            return Err(PatchError::UnknownPatch(id.clone()));
        }
    }
    let mut docs = BTreeMap::new();
    for path in TARGETS {
        let source = sources
            .get(path)
            .ok_or_else(|| PatchError::MissingTarget(path.to_owned()))?;
        docs.insert(path.to_owned(), QmlDoc::parse(source, path)?);
    }

    let mut edits = collect_patch_edits(&docs, &PATCHES, selected)?;

    let mut output = BTreeMap::new();
    for (path, doc) in docs {
        let mut text = doc.source().to_owned();
        if let Some(mut file_edits) = edits.remove(&path) {
            file_edits.sort_by_key(|edit| std::cmp::Reverse(edit.start));
            let mut previous_start = usize::MAX;
            for edit in file_edits {
                if edit.end > previous_start {
                    return Err(unsupported(&path, "overlapping patch edits"));
                }
                text.replace_range(edit.start..edit.end, &edit.replacement);
                previous_start = edit.start;
            }
        }
        output.insert(path, text);
    }
    Ok(PatchOutput {
        files: output,
        applied: PATCHES
            .iter()
            .filter(|patch| selected.contains(patch.id))
            .collect(),
    })
}

fn scalar_edit(
    doc: &QmlDoc<'_>,
    scope: usize,
    property: &str,
    expected: &str,
    replacement: &str,
    file: &str,
) -> Result<Vec<Edit>, PatchError> {
    let span = doc.scalar_span(scope, property, expected, file)?;
    Ok(vec![Edit {
        start: span.start,
        end: span.end,
        replacement: replacement.to_owned(),
    }])
}
fn condition_edits(
    doc: &QmlDoc<'_>,
    body_open: usize,
    body_close: usize,
    expected: &str,
    replacement: &str,
    expected_count: usize,
    file: &str,
) -> Result<Vec<Edit>, PatchError> {
    let spans = doc.if_condition_spans(body_open, body_close, expected, expected_count, file)?;
    Ok(spans
        .into_iter()
        .map(|span| Edit {
            start: span.start,
            end: span.end,
            replacement: replacement.to_owned(),
        })
        .collect())
}

fn locate_patch(doc: &QmlDoc<'_>, rule: PatchRule, file: &str) -> Result<Vec<Edit>, PatchError> {
    match rule {
        PatchRule::Scalar {
            root_id,
            scope,
            property,
            expected,
            replacement,
        } => {
            let root = doc.object_by_id(root_id, file)?;
            let scope = match scope {
                Some(scope) => doc.child_object_with_property(
                    root,
                    scope.class,
                    scope.property,
                    scope.expected,
                    file,
                )?,
                None => root,
            };
            scalar_edit(doc, scope, property, expected, replacement, file)
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
            let scope = doc.object_by_id(root_id, file)?;
            let spans = doc.expression_literal_spans(
                scope,
                property,
                expected_literal,
                expected_count,
                required_identifiers,
                required_operators,
                file,
            )?;
            Ok(spans
                .into_iter()
                .map(|span| Edit {
                    start: span.start,
                    end: span.end,
                    replacement: replacement.to_owned(),
                })
                .collect())
        }
        PatchRule::ScalarGroup { root_id, targets } => {
            if targets.is_empty() {
                return Err(unsupported(file, "scalar group has no targets"));
            }
            let root = doc.object_by_id(root_id, file)?;
            let mut edits = Vec::with_capacity(targets.len());
            for target in targets {
                let scope = match target.scope {
                    Some(scope) => doc.child_object_with_property(
                        root,
                        scope.class,
                        scope.property,
                        scope.expected,
                        file,
                    )?,
                    None => root,
                };
                edits.extend(scalar_edit(
                    doc,
                    scope,
                    target.property,
                    target.expected,
                    target.replacement,
                    file,
                )?);
            }
            Ok(edits)
        }
        PatchRule::FunctionStatements { root_id, targets } => {
            if targets.is_empty() {
                return Err(unsupported(file, "function statement group has no targets"));
            }
            let root = doc.object_by_id(root_id, file)?;
            let edit_capacity = targets
                .iter()
                .map(|target| target.expected_count)
                .sum::<usize>();
            let mut edits = Vec::with_capacity(edit_capacity);
            for target in targets {
                let (body_open, body_close) = doc.function_body(root, target.function, file)?;
                let spans = doc.statement_spans(
                    body_open,
                    body_close,
                    target.expected,
                    target.expected_count,
                    file,
                )?;
                edits.extend(spans.into_iter().map(|span| Edit {
                    start: span.start,
                    end: span.end,
                    replacement: target.replacement.to_owned(),
                }));
            }
            Ok(edits)
        }
        PatchRule::IfCondition {
            root_id,
            function,
            expected,
            replacement,
            expected_count,
        } => {
            let root = doc.object_by_id(root_id, file)?;
            let (body_open, body_close) = doc.function_body(root, function, file)?;
            condition_edits(
                doc,
                body_open,
                body_close,
                expected,
                replacement,
                expected_count,
                file,
            )
        }
        PatchRule::HandlerCondition {
            root_id,
            scope,
            handler,
            expected,
            replacement,
            expected_count,
        } => {
            let root = doc.object_by_id(root_id, file)?;
            let scope = doc.child_object_with_property(
                root,
                scope.class,
                scope.property,
                scope.expected,
                file,
            )?;
            let (body_open, body_close) = doc.handler_body(scope, handler, file)?;
            condition_edits(
                doc,
                body_open,
                body_close,
                expected,
                replacement,
                expected_count,
                file,
            )
        }
        PatchRule::MultiSite { .. } => Err(unsupported(
            file,
            "multi-site patch groups must be resolved before matching a QML file",
        )),
    }
}

pub fn selected_patch_ids<I, S>(ids: I) -> Result<BTreeSet<String>, PatchError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let selected: BTreeSet<String> = ids.into_iter().map(Into::into).collect();
    for id in &selected {
        if !PATCHES.iter().any(|patch| patch.id == id) {
            return Err(PatchError::UnknownPatch(id.clone()));
        }
    }
    if selected.is_empty() {
        return Err(PatchError::EmptySelection);
    }
    Ok(selected)
}
