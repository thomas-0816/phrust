//! Request-local state for stateful text builtins.

/// Request-local state for `strtok`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StrtokState {
    input: Vec<u8>,
    offset: usize,
    mode: StrtokMode,
    emitted_token: bool,
}

impl StrtokState {
    /// Starts tokenization over a new input string.
    pub fn reset(&mut self, input: Vec<u8>) {
        self.input = input;
        self.offset = 0;
        self.mode = StrtokMode::Active;
        self.emitted_token = false;
    }

    /// Whether one-argument `strtok()` needs a new input string first.
    #[must_use]
    pub const fn requires_input(&self) -> bool {
        matches!(self.mode, StrtokMode::NeedsInput)
    }

    /// Returns the next token separated by any byte in `delimiters`.
    pub fn next_token(&mut self, delimiters: &[u8]) -> Option<Vec<u8>> {
        if delimiters.is_empty() {
            return if self.offset == 0 {
                let token = self.input.clone();
                self.offset = self.input.len();
                Some(token)
            } else {
                None
            };
        }
        let skipped_start = self.offset;
        while self.offset < self.input.len() && delimiters.contains(&self.input[self.offset]) {
            self.offset += 1;
        }
        if self.offset >= self.input.len() {
            // A trailing delimiter leaves the state ready to report that a new
            // input string is required, matching PHP's saved-pointer behavior.
            self.mode = if self.input.is_empty()
                || (self.emitted_token && self.offset.saturating_sub(skipped_start) == 0)
            {
                StrtokMode::Exhausted
            } else {
                StrtokMode::NeedsInput
            };
            return None;
        }
        let start = self.offset;
        while self.offset < self.input.len() && !delimiters.contains(&self.input[self.offset]) {
            self.offset += 1;
        }
        let token = self.input[start..self.offset].to_vec();
        if self.offset < self.input.len() {
            self.offset += 1;
        }
        self.mode = StrtokMode::Active;
        self.emitted_token = true;
        Some(token)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum StrtokMode {
    #[default]
    Exhausted,
    Active,
    NeedsInput,
}

/// Request-local iconv encoding configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IconvEncodingState {
    input_encoding: String,
    output_encoding: String,
    internal_encoding: String,
}

impl Default for IconvEncodingState {
    fn default() -> Self {
        Self {
            input_encoding: "UTF-8".to_owned(),
            output_encoding: "UTF-8".to_owned(),
            internal_encoding: "UTF-8".to_owned(),
        }
    }
}

impl IconvEncodingState {
    /// Returns the input encoding used by iconv defaults.
    #[must_use]
    pub fn input_encoding(&self) -> &str {
        &self.input_encoding
    }

    /// Returns the output encoding used by iconv defaults.
    #[must_use]
    pub fn output_encoding(&self) -> &str {
        &self.output_encoding
    }

    /// Returns the internal encoding used by iconv defaults.
    #[must_use]
    pub fn internal_encoding(&self) -> &str {
        &self.internal_encoding
    }

    /// Updates one named iconv encoding setting.
    pub fn set(&mut self, name: &str, encoding: impl Into<String>) -> bool {
        match name {
            "input_encoding" => self.input_encoding = encoding.into(),
            "output_encoding" => self.output_encoding = encoding.into(),
            "internal_encoding" => self.internal_encoding = encoding.into(),
            _ => return false,
        }
        true
    }
}
