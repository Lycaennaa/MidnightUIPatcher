use super::super::{PatchError, unsupported};
use super::QmlDoc;
impl<'a> QmlDoc<'a> {
    pub(in crate::patcher) fn import_insert_offset(
        &self,
        module: &str,
        file: &str,
    ) -> Result<Option<usize>, PatchError> {
        let mut last_import_end = None;
        let mut index = 0;
        while index < self.tokens.len() {
            if self.container[index].is_some() || !self.token_eq(index, "import") {
                index += 1;
                continue;
            }

            let line = self.tokens[index].line;
            let mut line_end = index + 1;
            while line_end < self.tokens.len() && self.tokens[line_end].line == line {
                line_end += 1;
            }
            if line_end > index + 1 && self.token_eq(index + 1, module) {
                if (index + 2..line_end).any(|import_token| self.token_eq(import_token, "as")) {
                    return Err(unsupported(
                        file,
                        &format!("aliased import of {module} is unsupported"),
                    ));
                }
                return Ok(None);
            }
            last_import_end = Some(self.tokens[line_end - 1].end);
            index = line_end;
        }

        let Some(last_import_end) = last_import_end else {
            return Err(unsupported(file, "no top-level import to extend"));
        };
        let line_end = self.source[last_import_end..]
            .find('\n')
            .map(|offset| last_import_end + offset + 1)
            .ok_or_else(|| unsupported(file, "last import line has no newline"))?;
        Ok(Some(line_end))
    }
    pub(in crate::patcher) fn object_member_insertion(
        &self,
        object_id: &str,
        anchor_property: &str,
        expected: &str,
        child_types: &[&str],
        file: &str,
    ) -> Result<(usize, String), PatchError> {
        if child_types.is_empty() {
            return Err(unsupported(file, "appended member has no child guards"));
        }
        let scope = self.object_by_id(object_id, file)?;
        let anchor = self.scalar_span(scope, anchor_property, expected, file)?;
        let Some(close) = self.matching[scope] else {
            return Err(unsupported(file, "missing object end"));
        };
        for child_type in child_types {
            for index in scope + 1..close {
                if self.container[index] == Some(scope)
                    && self.token_eq(index, child_type)
                    && index + 1 < close
                    && self.token_eq(index + 1, "{")
                {
                    return Err(unsupported(
                        file,
                        &format!("object {object_id} already contains {child_type}"),
                    ));
                }
            }
        }

        let anchor_line_start = self.source[..anchor.start]
            .rfind('\n')
            .map_or(0, |index| index + 1);
        let anchor_prefix = &self.source[anchor_line_start..anchor.start];
        let indent_length = anchor_prefix
            .chars()
            .take_while(|character| matches!(character, ' ' | '\t'))
            .map(char::len_utf8)
            .sum::<usize>();
        let indent = anchor_prefix[..indent_length].to_owned();

        let close_start = self.tokens[close].start;
        let close_line_start = self.source[..close_start]
            .rfind('\n')
            .map_or(0, |index| index + 1);
        if !self.source[close_line_start..close_start]
            .chars()
            .all(char::is_whitespace)
        {
            return Err(unsupported(file, "object end is not on its own line"));
        }
        Ok((close_line_start, indent))
    }
}
