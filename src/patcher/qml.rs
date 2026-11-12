use std::collections::BTreeMap;
use std::ops::Range;

use super::{PatchError, unsupported};
mod lexer;

use self::lexer::{Token, lex_fragment};

pub(super) struct QmlDoc<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    matching: Vec<Option<usize>>,
    container: Vec<Option<usize>>,
}

impl<'a> QmlDoc<'a> {
    pub(super) fn parse(source: &'a str, file: &str) -> Result<Self, PatchError> {
        let (tokens, matching, container) = lexer::lex(source, file)?;
        Ok(Self {
            source,
            tokens,
            matching,
            container,
        })
    }

    pub(super) fn source(&self) -> &'a str {
        self.source
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
        let member_tokens = lex_fragment(name);
        if member_tokens.is_empty() {
            return Err(PatchError::Unsupported {
                file: "<qml>".into(),
                detail: "empty property path".into(),
            });
        }
        let mut matches = Vec::new();
        for index in scope + 1..close {
            let colon = index + member_tokens.len();
            if colon >= close
                || self.container[index] != Some(scope)
                || member_tokens.iter().enumerate().any(|(offset, token)| {
                    let member_index = index + offset;
                    self.container[member_index] != Some(scope)
                        || self.text(member_index) != token.as_str()
                })
                || !self.token_eq(colon, ":")
            {
                continue;
            }
            matches.push((index, colon + 1));
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

    pub(super) fn object_by_id(&self, id: &str, file: &str) -> Result<usize, PatchError> {
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

    pub(super) fn child_object_with_property(
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

    pub(super) fn function_body(
        &self,
        parent: usize,
        name: &str,
        file: &str,
    ) -> Result<(usize, usize), PatchError> {
        let Some(parent_close) = self.matching[parent] else {
            return Err(unsupported(file, "missing parent object end"));
        };
        let mut found = Vec::new();
        for index in parent + 1..parent_close {
            if self.container[index] != Some(parent)
                || !self.token_eq(index, "function")
                || self.tokens.get(index + 1).is_none()
                || !self.token_eq(index + 1, name)
                || self.tokens.get(index + 2).is_none()
                || !self.token_eq(index + 2, "(")
            {
                continue;
            }
            let params_open = index + 2;
            let Some(params_close) = self.matching[params_open] else {
                return Err(unsupported(file, "function parameters are unbalanced"));
            };
            let body_open = params_close + 1;
            if body_open >= parent_close || !self.token_eq(body_open, "{") {
                return Err(unsupported(file, &format!("function {name} has no body")));
            }
            let Some(body_close) = self.matching[body_open] else {
                return Err(unsupported(file, "function body is unbalanced"));
            };
            found.push((body_open, body_close));
        }
        match found.as_slice() {
            [one] => Ok(*one),
            [] => Err(unsupported(file, &format!("function {name} not found"))),
            _ => Err(unsupported(file, &format!("function {name} is ambiguous"))),
        }
    }
    pub(super) fn handler_body(
        &self,
        scope: usize,
        handler: &str,
        file: &str,
    ) -> Result<(usize, usize), PatchError> {
        let (_, body_open) = self
            .direct_member(scope, handler)
            .map_err(|error| refile(error, file))?;
        if body_open >= self.tokens.len() || !self.token_eq(body_open, "{") {
            return Err(unsupported(
                file,
                &format!("signal handler {handler} has no body"),
            ));
        }
        let Some(body_close) = self.matching[body_open] else {
            return Err(unsupported(file, "signal handler body is unbalanced"));
        };
        Ok((body_open, body_close))
    }

    pub(super) fn statement_spans(
        &self,
        body_open: usize,
        body_close: usize,
        expected: &str,
        expected_count: usize,
        file: &str,
    ) -> Result<Vec<Range<usize>>, PatchError> {
        let expected_tokens = lex_fragment(expected);
        if expected_tokens.is_empty()
            || expected_count == 0
            || expected_tokens.last().map(|token| token.as_str()) != Some(";")
        {
            return Err(unsupported(
                file,
                "invalid function statement or expected count",
            ));
        }
        let mut found = Vec::with_capacity(expected_count);
        for start in body_open + 1..body_close {
            if start > body_open + 1 && !matches!(self.text(start - 1), ";" | "{" | "}") {
                continue;
            }
            let end = start + expected_tokens.len();
            if end > body_close
                || expected_tokens
                    .iter()
                    .enumerate()
                    .any(|(offset, token)| self.text(start + offset) != token)
            {
                continue;
            }
            found.push(self.tokens[start].start..self.tokens[end - 1].end);
        }
        if found.len() != expected_count {
            return Err(unsupported(
                file,
                &format!(
                    "function has {} matching statements for {expected}; expected {expected_count}",
                    found.len()
                ),
            ));
        }
        Ok(found)
    }

    pub(super) fn if_condition_spans(
        &self,
        body_open: usize,
        body_close: usize,
        expected: &str,
        expected_count: usize,
        file: &str,
    ) -> Result<Vec<Range<usize>>, PatchError> {
        let expected_tokens = lex_fragment(expected);
        if expected_tokens.is_empty() || expected_count == 0 {
            return Err(unsupported(file, "empty if condition or expected count"));
        }
        let mut found = Vec::new();
        for index in body_open + 1..body_close {
            if !self.token_eq(index, "if") {
                continue;
            }
            let condition_open = index + 1;
            if condition_open >= body_close || !self.token_eq(condition_open, "(") {
                continue;
            }
            let Some(condition_close) = self.matching[condition_open] else {
                return Err(unsupported(file, "if condition is unbalanced"));
            };
            if condition_close >= body_close {
                return Err(unsupported(file, "if condition extends beyond code block"));
            }
            let actual = self.token_sequence(condition_open + 1, condition_close);
            if actual.len() == expected_tokens.len()
                && actual
                    .iter()
                    .zip(&expected_tokens)
                    .all(|(actual, expected)| *actual == expected.as_str())
            {
                let first = &self.tokens[condition_open + 1];
                let last = &self.tokens[condition_close - 1];
                found.push(first.start..last.end);
            }
        }
        if found.len() != expected_count {
            return Err(unsupported(
                file,
                &format!(
                    "code block has {} matching if conditions; expected {expected_count}",
                    found.len()
                ),
            ));
        }
        Ok(found)
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

    pub(super) fn scalar_span(
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

    pub(super) fn expression_literal_spans(
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

fn refile(error: PatchError, file: &str) -> PatchError {
    match error {
        PatchError::Unsupported { detail, .. } => PatchError::Unsupported {
            file: file.to_owned(),
            detail,
        },
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::QmlDoc;

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
}
