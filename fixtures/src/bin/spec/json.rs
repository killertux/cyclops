//! Checks that text is one JSON document with nothing around it.

/// How deep arrays and objects may nest before the text is refused.
const MAX_DEPTH: usize = 64;

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn skip_space(&mut self) {
        while self.peek().is_some_and(|byte| byte.is_ascii_whitespace()) {
            self.at += 1;
        }
    }

    fn eat(&mut self, byte: u8) -> bool {
        self.skip_space();
        let found = self.peek() == Some(byte);
        if found {
            self.at += 1;
        }
        found
    }

    fn literal(&mut self, word: &str) -> bool {
        let found = self.bytes.get(self.at..self.at + word.len()) == Some(word.as_bytes());
        if found {
            self.at += word.len();
        }
        found
    }

    fn string(&mut self) -> bool {
        if !self.eat(b'"') {
            return false;
        }
        while let Some(byte) = self.peek() {
            self.at += 1;
            match byte {
                b'"' => return true,
                b'\\' => self.at += 1,
                byte if byte < 0x20 => return false,
                _ => {}
            }
        }
        false
    }

    fn number(&mut self) -> bool {
        let start = self.at;
        while self.peek().is_some_and(|byte| {
            byte.is_ascii_digit() || matches!(byte, b'-' | b'+' | b'.' | b'e' | b'E')
        }) {
            self.at += 1;
        }
        self.bytes
            .get(start..self.at)
            .and_then(|text| std::str::from_utf8(text).ok())
            .is_some_and(|text| text.parse::<f64>().is_ok())
    }

    fn value(&mut self, depth: usize) -> bool {
        if depth > MAX_DEPTH {
            return false;
        }
        self.skip_space();
        match self.peek() {
            Some(b'{') => {
                self.at += 1;
                if self.eat(b'}') {
                    return true;
                }
                loop {
                    if !(self.string() && self.eat(b':') && self.value(depth + 1)) {
                        return false;
                    }
                    if self.eat(b'}') {
                        return true;
                    }
                    if !self.eat(b',') {
                        return false;
                    }
                }
            }
            Some(b'[') => {
                self.at += 1;
                if self.eat(b']') {
                    return true;
                }
                loop {
                    if !self.value(depth + 1) {
                        return false;
                    }
                    if self.eat(b']') {
                        return true;
                    }
                    if !self.eat(b',') {
                        return false;
                    }
                }
            }
            Some(b'"') => self.string(),
            Some(b't') => self.literal("true"),
            Some(b'f') => self.literal("false"),
            Some(b'n') => self.literal("null"),
            Some(_) => self.number(),
            None => false,
        }
    }
}

pub fn is_valid(text: &str) -> bool {
    let mut reader = Reader {
        bytes: text.as_bytes(),
        at: 0,
    };
    let valid = reader.value(0);
    reader.skip_space();
    valid && reader.peek().is_none()
}
