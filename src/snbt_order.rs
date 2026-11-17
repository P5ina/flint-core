//! Order-insensitive SNBT comparison.
//!
//! NBT compounds are unordered: vanilla `CompoundTag.equals` ignores key
//! order, and `/data get` prints keys in hash order. Test specs copy that
//! order while servers may report keys sorted, so both sides are rewritten
//! with every compound's entries sorted by key before comparing.

/// Sorts the entries of every compound in an SNBT value (whitespace already
/// stripped outside quotes). Returns the input unchanged if it does not parse.
pub(crate) fn sort_compound_keys(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    let mut parser = Parser {
        chars: &chars,
        pos: 0,
    };
    match parser.value() {
        Some(sorted) if parser.pos == chars.len() => sorted,
        _ => value.to_string(),
    }
}

struct Parser<'a> {
    chars: &'a [char],
    pos: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn eat(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn value(&mut self) -> Option<String> {
        match self.peek()? {
            '{' => self.compound(),
            '[' => self.list(),
            '"' | '\'' => self.quoted().map(|(raw, _)| raw),
            _ => self.bare(),
        }
    }

    fn compound(&mut self) -> Option<String> {
        self.pos += 1;
        let mut entries = Vec::new();
        if !self.eat('}') {
            loop {
                let (raw_key, key) = match self.peek()? {
                    '"' | '\'' => self.quoted()?,
                    _ => {
                        let raw = self.bare()?;
                        (raw.clone(), raw)
                    }
                };
                if !self.eat(':') {
                    return None;
                }
                let value = self.value()?;
                entries.push((key, format!("{raw_key}:{value}")));
                if self.eat('}') {
                    break;
                }
                if !self.eat(',') {
                    return None;
                }
            }
        }
        entries.sort_by(|(left, _), (right, _)| left.encode_utf16().cmp(right.encode_utf16()));
        let body: Vec<String> = entries.into_iter().map(|(_, entry)| entry).collect();
        Some(format!("{{{}}}", body.join(",")))
    }

    fn list(&mut self) -> Option<String> {
        self.pos += 1;
        let mut out = String::from("[");
        // Typed arrays: `[B;`, `[I;`, `[L;`.
        if matches!(self.peek(), Some('B' | 'I' | 'L'))
            && self.chars.get(self.pos + 1) == Some(&';')
        {
            out.push(self.chars[self.pos]);
            out.push(';');
            self.pos += 2;
        }
        if self.eat(']') {
            out.push(']');
            return Some(out);
        }
        loop {
            out.push_str(&self.value()?);
            if self.eat(']') {
                out.push(']');
                return Some(out);
            }
            if !self.eat(',') {
                return None;
            }
            out.push(',');
        }
    }

    /// Returns the quoted token as written and its unescaped text.
    fn quoted(&mut self) -> Option<(String, String)> {
        let quote = self.peek()?;
        let start = self.pos;
        self.pos += 1;
        let mut text = String::new();
        loop {
            let c = self.peek()?;
            self.pos += 1;
            if c == '\\' {
                text.push(self.peek()?);
                self.pos += 1;
            } else if c == quote {
                let raw: String = self.chars[start..self.pos].iter().collect();
                return Some((raw, text));
            } else {
                text.push(c);
            }
        }
    }

    fn bare(&mut self) -> Option<String> {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if matches!(c, ',' | ':' | '}' | ']' | '{' | '[') {
                break;
            }
            self.pos += 1;
        }
        (self.pos > start).then(|| self.chars[start..self.pos].iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::sort_compound_keys;

    #[test]
    fn compound_key_order_does_not_matter() {
        let spec = r#"[{components:{"minecraft:damage":1},count:1,Slot:0b,id:"minecraft:flint_and_steel"}]"#;
        let server = r#"[{Slot:0b,components:{"minecraft:damage":1},count:1,id:"minecraft:flint_and_steel"}]"#;
        assert_eq!(sort_compound_keys(spec), sort_compound_keys(server));
    }

    #[test]
    fn nested_compounds_and_typed_arrays_are_kept() {
        assert_eq!(
            sort_compound_keys(r#"{b:{y:[I;1,2],x:'a,b'},a:[]}"#),
            r#"{a:[],b:{x:'a,b',y:[I;1,2]}}"#
        );
    }

    #[test]
    fn list_order_still_matters() {
        assert_ne!(sort_compound_keys("[1,2]"), sort_compound_keys("[2,1]"));
    }

    #[test]
    fn plain_values_pass_through() {
        assert_eq!(sort_compound_keys("1b"), "1b");
        assert_eq!(sort_compound_keys("{broken"), "{broken");
    }
}
