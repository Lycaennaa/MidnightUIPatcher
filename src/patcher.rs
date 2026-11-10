use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use thiserror::Error;

pub const TARGETS: [&str; 5] = [
    "usr/lib/qml/QuickSettings/Utils/Style.qml",
    "usr/lib/qml/QuickSettings/Containers/QuickSettingsMainView.qml",
    "usr/lib/qml/SystemUI/Notification/NotificationStyle.qml",
    "usr/lib/qml/SystemUI/Notification/Toast.qml",
    "usr/lib/qml/QuickSettings/Component/BaseControlPanelButton.qml",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PatchKind {
    MainPanelColor,
    MainPanelOpacity,
    MainPanelVisibility,
    MenuPopupColor,
    ButtonDefaultColor,
    ButtonIdleFill,
    CandidateBackgroundColor,
    CandidateBackgroundOpacity,
    MainLayoutColor,
    MainLayoutSecondaryColor,
    RecentHistoryColor,
    EditPopupColor,
    MenuPopupAreaColor,
    MenuPopupButtonColor,
    SliderPopupColor,
    ToastActivatedColor,
    ToastBackgroundColor,
    ToastLightOpacity,
    ToastLinkColor,
}

#[derive(Clone, Copy, Debug)]
pub struct PatchSpec {
    pub kind: PatchKind,
    pub id: &'static str,
    pub label: &'static str,
    pub target: &'static str,
}

pub const PATCHES: [PatchSpec; 19] = [
    PatchSpec {
        kind: PatchKind::MainPanelColor,
        id: "main-panel-color",
        label: "Set panel backdrop color to black",
        target: TARGETS[1],
    },
    PatchSpec {
        kind: PatchKind::MainPanelOpacity,
        id: "main-panel-opacity",
        label: "Make panel backdrop fully opaque",
        target: TARGETS[1],
    },
    PatchSpec {
        kind: PatchKind::MainPanelVisibility,
        id: "main-panel-visibility",
        label: "Hide the full-panel backdrop",
        target: TARGETS[1],
    },
    PatchSpec {
        kind: PatchKind::MenuPopupColor,
        id: "menu-popup-color",
        label: "Set the menu popup background to black",
        target: TARGETS[1],
    },
    PatchSpec {
        kind: PatchKind::ButtonDefaultColor,
        id: "button-default-color",
        label: "Set the tile default fill to black",
        target: TARGETS[4],
    },
    PatchSpec {
        kind: PatchKind::ButtonIdleFill,
        id: "button-idle-fill",
        label: "Set idle tile fills to black",
        target: TARGETS[4],
    },
    PatchSpec {
        kind: PatchKind::CandidateBackgroundColor,
        id: "candidate-background-color",
        label: "Set candidate-list background to black",
        target: TARGETS[0],
    },
    PatchSpec {
        kind: PatchKind::CandidateBackgroundOpacity,
        id: "candidate-background-opacity",
        label: "Make candidate-list background opaque",
        target: TARGETS[0],
    },
    PatchSpec {
        kind: PatchKind::MainLayoutColor,
        id: "main-layout-color",
        label: "Make the main layout background transparent",
        target: TARGETS[0],
    },
    PatchSpec {
        kind: PatchKind::MainLayoutSecondaryColor,
        id: "main-layout-secondary-color",
        label: "Make the alternate main-layout background transparent",
        target: TARGETS[0],
    },
    PatchSpec {
        kind: PatchKind::RecentHistoryColor,
        id: "recent-history-color",
        label: "Set recent-history background to black",
        target: TARGETS[0],
    },
    PatchSpec {
        kind: PatchKind::EditPopupColor,
        id: "edit-popup-color",
        label: "Set edit-popup background to black",
        target: TARGETS[0],
    },
    PatchSpec {
        kind: PatchKind::MenuPopupAreaColor,
        id: "menu-popup-area-color",
        label: "Set the menu popup area to black",
        target: TARGETS[0],
    },
    PatchSpec {
        kind: PatchKind::MenuPopupButtonColor,
        id: "menu-popup-button-color",
        label: "Set menu popup buttons to black",
        target: TARGETS[0],
    },
    PatchSpec {
        kind: PatchKind::SliderPopupColor,
        id: "slider-popup-color",
        label: "Set slider popup background to black",
        target: TARGETS[0],
    },
    PatchSpec {
        kind: PatchKind::ToastActivatedColor,
        id: "toast-activated-color",
        label: "Set activated toast color to white",
        target: TARGETS[2],
    },
    PatchSpec {
        kind: PatchKind::ToastBackgroundColor,
        id: "toast-background-color",
        label: "Set toast background to black",
        target: TARGETS[2],
    },
    PatchSpec {
        kind: PatchKind::ToastLightOpacity,
        id: "toast-light-opacity",
        label: "Make light-toast background opaque",
        target: TARGETS[2],
    },
    PatchSpec {
        kind: PatchKind::ToastLinkColor,
        id: "toast-link-color",
        label: "Keep activated toast links white on black",
        target: TARGETS[3],
    },
];

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

#[derive(Clone, Debug)]
struct Token {
    start: usize,
    end: usize,
    line: usize,
}

struct QmlDoc<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    matching: Vec<Option<usize>>,
    container: Vec<Option<usize>>,
}

impl<'a> QmlDoc<'a> {
    fn parse(source: &'a str, file: &str) -> Result<Self, PatchError> {
        let mut tokens = Vec::new();
        let bytes = source.as_bytes();
        let mut i = 0;
        let mut line = 1;
        while i < bytes.len() {
            match bytes[i] {
                b' ' | b'\t' | b'\r' => i += 1,
                b'\n' => {
                    line += 1;
                    i += 1;
                }
                b'/' if bytes.get(i + 1) == Some(&b'/') => {
                    i += 2;
                    while i < bytes.len() && bytes[i] != b'\n' {
                        i += 1;
                    }
                }
                b'/' if bytes.get(i + 1) == Some(&b'*') => {
                    i += 2;
                    let mut closed = false;
                    while i + 1 < bytes.len() {
                        if bytes[i] == b'\n' {
                            line += 1;
                        }
                        if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                            i += 2;
                            closed = true;
                            break;
                        }
                        i += 1;
                    }
                    if !closed {
                        return Err(unsupported(file, "unterminated block comment"));
                    }
                }
                quote @ (b'\'' | b'"' | b'`') => {
                    let start = i;
                    i += 1;
                    let mut closed = false;
                    while i < bytes.len() {
                        if bytes[i] == b'\\' {
                            i = (i + 2).min(bytes.len());
                        } else if bytes[i] == quote {
                            i += 1;
                            closed = true;
                            break;
                        } else {
                            if bytes[i] == b'\n' {
                                line += 1;
                            }
                            i += 1;
                        }
                    }
                    if !closed {
                        return Err(unsupported(file, "unterminated string literal"));
                    }
                    tokens.push(Token {
                        start,
                        end: i,
                        line,
                    });
                }
                b if is_ident_start(b) => {
                    let start = i;
                    i += 1;
                    while i < bytes.len() && is_ident_continue(bytes[i]) {
                        i += 1;
                    }
                    tokens.push(Token {
                        start,
                        end: i,
                        line,
                    });
                }
                b if b.is_ascii_digit() => {
                    let start = i;
                    i += 1;
                    while i < bytes.len()
                        && (bytes[i].is_ascii_alphanumeric()
                            || bytes[i] == b'.'
                            || bytes[i] == b'_')
                    {
                        i += 1;
                    }
                    tokens.push(Token {
                        start,
                        end: i,
                        line,
                    });
                }
                _ => {
                    let start = i;
                    i += 1;
                    tokens.push(Token {
                        start,
                        end: i,
                        line,
                    });
                }
            }
        }

        let mut matching = vec![None; tokens.len()];
        let mut container = vec![None; tokens.len()];
        let mut stack: Vec<(usize, u8)> = Vec::new();
        for index in 0..tokens.len() {
            let text = &source[tokens[index].start..tokens[index].end];
            let byte = text.as_bytes()[0];
            if is_close(byte) {
                let Some((open, open_byte)) = stack.pop() else {
                    return Err(unsupported(file, "unbalanced delimiters"));
                };
                if !same_delimiter(open_byte, byte) {
                    return Err(unsupported(file, "mismatched delimiters"));
                }
                container[index] = Some(open);
                matching[index] = Some(open);
                matching[open] = Some(index);
            } else {
                container[index] = stack.last().map(|(open, _)| *open);
                if is_open(byte) {
                    stack.push((index, byte));
                }
            }
        }
        if !stack.is_empty() {
            return Err(unsupported(file, "unbalanced delimiters"));
        }
        Ok(Self {
            source,
            tokens,
            matching,
            container,
        })
    }

    fn text(&self, index: usize) -> &str {
        &self.source[self.tokens[index].start..self.tokens[index].end]
    }

    fn token_eq(&self, index: usize, value: &str) -> bool {
        self.text(index) == value
    }

    fn token_sequence(&self, start: usize, end: usize) -> Vec<&str> {
        (start..end).map(|i| self.text(i)).collect()
    }

    fn direct_member(&self, scope: usize, name: &str) -> Result<(usize, usize), PatchError> {
        let Some(close) = self.matching[scope] else {
            return Err(PatchError::Unsupported {
                file: "<qml>".into(),
                detail: "missing object end".into(),
            });
        };
        let mut matches = Vec::new();
        for index in scope + 1..close {
            if self.container[index] == Some(scope)
                && self.token_eq(index, name)
                && self.tokens.get(index + 1).is_some()
                && self.token_eq(index + 1, ":")
            {
                matches.push((index, index + 2));
            }
        }
        match matches.as_slice() {
            [one] => Ok(*one),
            [] => Err(PatchError::Unsupported {
                file: "<qml>".into(),
                detail: format!("property {name} not found"),
            }),
            _ => Err(PatchError::Unsupported {
                file: "<qml>".into(),
                detail: format!("property {name} is ambiguous"),
            }),
        }
    }

    fn property_object(&self, scope: usize, name: &str, file: &str) -> Result<usize, PatchError> {
        let Some(close) = self.matching[scope] else {
            return Err(unsupported(file, "missing object end"));
        };
        let mut found = Vec::new();
        for i in scope + 1..close {
            if self.container[i] != Some(scope)
                || !self.token_eq(i, name)
                || !self.token_eq(i + 1, ":")
            {
                continue;
            }
            if i < 2 || !self.token_eq(i - 2, "property") {
                continue;
            }
            let mut value = i + 2;
            while value < close && self.token_eq(value, "(") {
                value += 1;
            }
            if self.token_eq(value, "QtObject") && self.token_eq(value + 1, "{") {
                value += 1;
            }
            if self.token_eq(value, "{") {
                found.push(value);
            }
        }
        match found.as_slice() {
            [one] => Ok(*one),
            [] => Err(unsupported(
                file,
                &format!("QML object property {name} not found"),
            )),
            _ => Err(unsupported(
                file,
                &format!("QML object property {name} is ambiguous"),
            )),
        }
    }

    fn object_member(&self, scope: usize, name: &str, file: &str) -> Result<usize, PatchError> {
        let (_, value) = self
            .direct_member(scope, name)
            .map_err(|e| refile(e, file))?;
        let mut index = value;
        while self.token_eq(index, "(") {
            index += 1;
        }
        if !self.token_eq(index, "{") {
            return Err(unsupported(file, &format!("{name} is not an object")));
        }
        Ok(index)
    }

    fn object_by_id(&self, id: &str, file: &str) -> Result<usize, PatchError> {
        let mut found = Vec::new();
        for i in 0..self.tokens.len().saturating_sub(2) {
            if self.container[i].is_some()
                && self.token_eq(i, "id")
                && self.token_eq(i + 1, ":")
                && self.token_eq(i + 2, id)
            {
                if let Some(scope) = self.container[i] {
                    found.push(scope);
                }
            }
        }
        found.sort_unstable();
        found.dedup();
        match found.as_slice() {
            [one] => Ok(*one),
            [] => Err(unsupported(file, &format!("object id {id} not found"))),
            _ => Err(unsupported(file, &format!("object id {id} is ambiguous"))),
        }
    }

    fn child_object_with_property(
        &self,
        parent: usize,
        class: &str,
        property: &str,
        expected: &str,
        file: &str,
    ) -> Result<usize, PatchError> {
        let Some(close) = self.matching[parent] else {
            return Err(unsupported(file, "missing parent object end"));
        };
        let expected_tokens = lex_fragment(expected);
        let mut found = Vec::new();
        for i in parent + 1..close {
            if self.container[i] != Some(parent)
                || !self.token_eq(i, class)
                || !self.token_eq(i + 1, "{")
            {
                continue;
            }
            let child = i + 1;
            if let Ok((_, value_start)) = self.direct_member(child, property) {
                let range = self.value_line_range(child, value_start);
                if self.token_sequence(range.start, range.end)
                    == expected_tokens
                        .iter()
                        .map(String::as_str)
                        .collect::<Vec<_>>()
                {
                    found.push(child);
                }
            }
        }
        match found.as_slice() {
            [one] => Ok(*one),
            [] => Err(unsupported(
                file,
                &format!("no unique {class} child has expected {property}"),
            )),
            _ => Err(unsupported(
                file,
                &format!("multiple {class} children have expected {property}"),
            )),
        }
    }

    fn value_line_range(&self, scope: usize, start: usize) -> Range<usize> {
        if start >= self.tokens.len() {
            return start..start;
        }
        let line = self.tokens[start].line;
        let mut end = start;
        while end < self.tokens.len() && self.tokens[end].line == line {
            if self.container[end] == Some(scope)
                && (self.token_eq(end, ",") || self.token_eq(end, ";") || self.token_eq(end, "}"))
            {
                break;
            }
            end += 1;
        }
        start..end
    }

    fn scalar_span(
        &self,
        scope: usize,
        property: &str,
        expected: &str,
        file: &str,
    ) -> Result<Range<usize>, PatchError> {
        let (_, value_start) = self
            .direct_member(scope, property)
            .map_err(|e| refile(e, file))?;
        let range = self.value_line_range(scope, value_start);
        let expected_tokens = lex_fragment(expected);
        let actual = self.token_sequence(range.start, range.end);
        if actual
            != expected_tokens
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
        {
            return Err(unsupported(
                file,
                &format!("unexpected value for {property}"),
            ));
        }
        if expected_tokens.is_empty() {
            return Err(unsupported(file, "empty expected value"));
        }
        let first = &self.tokens[range.start];
        let last = &self.tokens[range.end - 1];
        Ok(first.start..last.end)
    }

    fn expression_literal_spans(
        &self,
        scope: usize,
        property: &str,
        expected_literal: &str,
        expected_count: usize,
        required_identifiers: &[&str],
        required_operators: &[&str],
        file: &str,
    ) -> Result<Vec<Range<usize>>, PatchError> {
        let (_, value_start) = self
            .direct_member(scope, property)
            .map_err(|e| refile(e, file))?;
        let range = self.value_line_range(scope, value_start);
        let mut spans = Vec::new();
        let mut id_counts = BTreeMap::new();
        let mut op_counts = BTreeMap::new();
        for index in range.clone() {
            let token = self.text(index);
            if token == expected_literal {
                spans.push(self.tokens[index].start..self.tokens[index].end);
            }
            if required_identifiers.contains(&token) {
                *id_counts.entry(token).or_insert(0usize) += 1;
            }
            if required_operators.contains(&token) {
                *op_counts.entry(token).or_insert(0usize) += 1;
            }
        }
        if spans.len() != expected_count {
            return Err(unsupported(
                file,
                &format!(
                    "{property} has {} matching old values; expected {expected_count}",
                    spans.len()
                ),
            ));
        }
        for identifier in required_identifiers {
            if id_counts.get(identifier).copied().unwrap_or_default() == 0 {
                return Err(unsupported(
                    file,
                    &format!("{property} lacks expected condition {identifier}"),
                ));
            }
        }
        for operator in required_operators {
            if op_counts.get(operator).copied().unwrap_or_default() == 0 {
                return Err(unsupported(
                    file,
                    &format!("{property} lacks expected operator {operator}"),
                ));
            }
        }
        Ok(spans)
    }
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$'
}
fn is_ident_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
}
fn is_open(byte: u8) -> bool {
    matches!(byte, b'{' | b'(' | b'[')
}
fn is_close(byte: u8) -> bool {
    matches!(byte, b'}' | b')' | b']')
}
fn same_delimiter(open: u8, close: u8) -> bool {
    matches!((open, close), (b'{', b'}') | (b'(', b')') | (b'[', b']'))
}

fn unsupported(file: &str, detail: &str) -> PatchError {
    PatchError::Unsupported {
        file: file.to_owned(),
        detail: detail.to_owned(),
    }
}

fn refile(error: PatchError, file: &str) -> PatchError {
    match error {
        PatchError::Unsupported { detail, .. } => PatchError::Unsupported {
            file: file.to_owned(),
            detail,
        },
        other => other,
    }
}

fn lex_fragment(fragment: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let bytes = fragment.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        match bytes[i] {
            b'\'' | b'"' | b'`' => {
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i = (i + 2).min(bytes.len());
                    } else if bytes[i] == quote {
                        i += 1;
                        break;
                    } else {
                        i += 1;
                    }
                }
            }
            b if is_ident_start(b) => {
                i += 1;
                while i < bytes.len() && is_ident_continue(bytes[i]) {
                    i += 1;
                }
            }
            b if b.is_ascii_digit() => {
                i += 1;
                while i < bytes.len()
                    && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'.' || bytes[i] == b'_')
                {
                    i += 1;
                }
            }
            _ => i += 1,
        }
        tokens.push(fragment[start..i].to_owned());
    }
    tokens
}

#[derive(Clone)]
struct Edit {
    start: usize,
    end: usize,
    replacement: String,
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

    let mut edits: BTreeMap<String, Vec<Edit>> = BTreeMap::new();
    for patch in &PATCHES {
        let doc = &docs[patch.target];
        let mut patch_edits = locate_patch(doc, patch.kind, patch.target)?;
        if selected.contains(patch.id) {
            edits
                .entry(patch.target.to_owned())
                .or_default()
                .append(&mut patch_edits);
        }
    }

    let mut output = BTreeMap::new();
    for (path, doc) in docs {
        let mut text = doc.source.to_owned();
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

fn locate_patch(doc: &QmlDoc<'_>, kind: PatchKind, file: &str) -> Result<Vec<Edit>, PatchError> {
    use PatchKind::*;
    match kind {
        MainPanelColor | MainPanelOpacity | MainPanelVisibility => {
            let parent = doc.object_by_id("mainLayout", file)?;
            let scope = doc.child_object_with_property(
                parent,
                "Rectangle",
                "visible",
                "interfaces.systemInfo.checkBackGround",
                file,
            )?;
            let (property, expected, replacement) = match kind {
                MainPanelColor => ("color", "\"#323941\"", "\"#000000\""),
                MainPanelOpacity => ("opacity", "0.95", "1.0"),
                MainPanelVisibility => {
                    ("visible", "interfaces.systemInfo.checkBackGround", "false")
                }
                _ => unreachable!(),
            };
            scalar_edit(doc, scope, property, expected, replacement, file)
        }
        MenuPopupColor => {
            let scope = doc.object_by_id("menuPopupBg", file)?;
            scalar_edit(doc, scope, "color", "\"#2E3741\"", "\"#000000\"", file)
        }
        ButtonDefaultColor => {
            let scope = doc.object_by_id("root", file)?;
            scalar_edit(doc, scope, "bgColor", "\"#575E66\"", "\"#000000\"", file)
        }
        ButtonIdleFill => {
            let scope = doc.object_by_id("background", file)?;
            let spans = doc.expression_literal_spans(
                scope,
                "color",
                "\"#575E66\"",
                2,
                &["objDeactive", "hasFocus"],
                &["?", ":"],
                file,
            )?;
            Ok(spans
                .into_iter()
                .map(|span| Edit {
                    start: span.start,
                    end: span.end,
                    replacement: "\"#000000\"".to_owned(),
                })
                .collect())
        }
        CandidateBackgroundColor | CandidateBackgroundOpacity => {
            let root = doc.object_by_id("styleRoot", file)?;
            let scope = doc.object_member(
                doc.property_object(root, "candidateListView", file)?,
                "BG_AREA",
                file,
            )?;
            if kind == CandidateBackgroundColor {
                scalar_edit(doc, scope, "BG_COLOR", "\"#575E66\"", "\"#000000\"", file)
            } else {
                scalar_edit(doc, scope, "BG_OPACITY", "0.9", "1.0", file)
            }
        }
        MainLayoutColor | MainLayoutSecondaryColor => {
            let root = doc.object_by_id("styleRoot", file)?;
            let scope = doc.object_member(
                doc.property_object(root, "quickSettingMainView", file)?,
                "MAINLAYOUT",
                file,
            )?;
            if kind == MainLayoutColor {
                scalar_edit(doc, scope, "COLOR", "\"#575E66\"", "\"transparent\"", file)
            } else {
                scalar_edit(doc, scope, "COLOR1", "\"#2E3741\"", "\"transparent\"", file)
            }
        }
        RecentHistoryColor | EditPopupColor | MenuPopupAreaColor | MenuPopupButtonColor => {
            let root = doc.object_by_id("styleRoot", file)?;
            let quick = doc.property_object(root, "quickSettingMainView", file)?;
            match kind {
                RecentHistoryColor => scalar_edit(
                    doc,
                    doc.object_member(quick, "RECENT_AREA", file)?,
                    "COLOR",
                    "\"#3E444D\"",
                    "\"#000000\"",
                    file,
                ),
                EditPopupColor => {
                    let editable = doc.object_member(quick, "EDITABLE_AREA", file)?;
                    scalar_edit(
                        doc,
                        doc.object_member(editable, "BG_EDITBOX", file)?,
                        "COLOR",
                        "\"#2E3741\"",
                        "\"#000000\"",
                        file,
                    )
                }
                MenuPopupAreaColor => scalar_edit(
                    doc,
                    doc.object_member(quick, "MENU_AREA", file)?,
                    "BG_COLOR",
                    "\"#404040\"",
                    "\"#000000\"",
                    file,
                ),
                MenuPopupButtonColor => {
                    let menu = doc.object_member(quick, "MENU_AREA", file)?;
                    scalar_edit(
                        doc,
                        doc.object_member(menu, "BUTTONS", file)?,
                        "SETTING1_BG_COLOR",
                        "\"#575E66\"",
                        "\"#000000\"",
                        file,
                    )
                }
                _ => unreachable!(),
            }
        }
        SliderPopupColor => {
            let root = doc.object_by_id("styleRoot", file)?;
            let slider = doc.property_object(root, "barSliderPopup", file)?;
            scalar_edit(
                doc,
                doc.object_member(slider, "BG_AREA", file)?,
                "COLOR",
                "\"#575E66\"",
                "\"#000000\"",
                file,
            )
        }
        ToastActivatedColor | ToastBackgroundColor | ToastLightOpacity => {
            let root = doc.object_by_id("root", file)?;
            let toast = doc.property_object(root, "toast", file)?;
            let (property, expected, replacement) = match kind {
                ToastActivatedColor => ("activatedColor", "\"#000000\"", "\"#FFFFFF\""),
                ToastBackgroundColor => ("backgroundColor", "\"#787F86\"", "\"#000000\""),
                ToastLightOpacity => ("lightOpacity", "0.5", "1.0"),
                _ => unreachable!(),
            };
            scalar_edit(doc, toast, property, expected, replacement, file)
        }
        ToastLinkColor => {
            let expected = "return style.toast.textColor";
            let replacement = "return style.toast.backgroundColor";
            let expected_tokens = lex_fragment(expected);
            let mut matches = Vec::new();
            for start in 0..doc.tokens.len() {
                let end = start + expected_tokens.len();
                if end <= doc.tokens.len()
                    && doc.token_sequence(start, end)
                        == expected_tokens
                            .iter()
                            .map(String::as_str)
                            .collect::<Vec<_>>()
                {
                    matches.push(start..end);
                }
            }
            match matches.as_slice() {
                [range] => {
                    let first = &doc.tokens[range.start];
                    let last = &doc.tokens[range.end - 1];
                    Ok(vec![Edit {
                        start: first.start,
                        end: last.end,
                        replacement: replacement.to_owned(),
                    }])
                }
                [] => Err(unsupported(file, "toast link color expression not found")),
                _ => Err(unsupported(
                    file,
                    "toast link color expression is ambiguous",
                )),
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizer_ignores_comment_and_string_delimiters() {
        let source = r#"Item { // }
            property string note: "{ not a block }"
            Rectangle { id: child }
        }"#;
        let parsed = QmlDoc::parse(source, "fixture.qml").unwrap();
        let root = parsed
            .tokens
            .iter()
            .enumerate()
            .find(|(index, _)| parsed.text(*index) == "{")
            .unwrap()
            .0;
        assert_eq!(parsed.text(parsed.matching[root].unwrap()), "}");
        assert_eq!(
            parsed.matching.iter().filter(|pair| pair.is_some()).count(),
            4
        );
    }

    #[test]
    fn patches_one_scoped_object_without_touching_identical_sibling() {
        let input = r##"Item {
            property var first: ({ AREA: { COLOR: "#112233" } })
            property var second: ({ AREA: { COLOR: "#112233" } })
        }"##;
        let doc = QmlDoc::parse(input, "fixture.qml").unwrap();
        let brace = doc
            .tokens
            .iter()
            .enumerate()
            .find(|(index, _)| doc.text(*index) == "{")
            .unwrap()
            .0;
        let first = doc.property_object(brace, "first", "fixture.qml").unwrap();
        let area = doc.object_member(first, "AREA", "fixture.qml").unwrap();
        let edit = scalar_edit(
            &doc,
            area,
            "COLOR",
            "\"#112233\"",
            "\"#000000\"",
            "fixture.qml",
        )
        .unwrap()
        .remove(0);
        let mut output = input.to_owned();
        output.replace_range(edit.start..edit.end, &edit.replacement);
        assert!(output.contains("first: ({ AREA: { COLOR: \"#000000\" } })"));
        assert!(output.contains("second: ({ AREA: { COLOR: \"#112233\" } })"));
    }
}
