//! C library number parsing as glibc does it, for the data file loaders ported from C++: atof/atoi,
//! and sscanf's %s %d %x %f (each returns None where sscanf would stop).

/// A simple scanner for parsing formatted text data, emulating scanf behavior.
pub struct Scan<'a> {
    s: &'a [u8],
    i: usize,
}

impl<'a> Scan<'a> {
    pub fn new(s: &'a [u8]) -> Self {
        Scan { s, i: 0 }
    }

    fn skip_whitespace(&mut self) {
        while self.i < self.s.len() {
            let b = self.s[self.i];
            match b {
                b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r' => self.i += 1,
                _ => break,
            }
        }
    }

    /// Parses a string token (%s).
    pub fn word(&mut self) -> Option<Vec<u8>> {
        self.skip_whitespace();
        if self.i >= self.s.len() {
            return None;
        }
        let start = self.i;
        while self.i < self.s.len() {
            let b = self.s[self.i];
            match b {
                b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r' => break,
                _ => self.i += 1,
            }
        }
        if self.i == start {
            return None;
        }
        Some(self.s[start..self.i].to_vec())
    }

    /// Parses a signed integer (%d).
    pub fn int(&mut self) -> Option<i32> {
        self.skip_whitespace();
        if self.i >= self.s.len() {
            return None;
        }
        let start = self.i;
        let mut sign = 1;
        if self.s[self.i] == b'+' || self.s[self.i] == b'-' {
            if self.s[self.i] == b'-' {
                sign = -1;
            }
            self.i += 1;
        }
        if self.i >= self.s.len() || !self.s[self.i].is_ascii_digit() {
            // Matching failure: position stays where it failed
            return None;
        }
        let mut val: i64 = 0;
        while self.i < self.s.len() && self.s[self.i].is_ascii_digit() {
            val = val.wrapping_mul(10).wrapping_add((self.s[self.i] - b'0') as i64);
            self.i += 1;
        }
        val = val.wrapping_mul(sign as i64);
        Some(val as i32)
    }

    /// Parses a signed hexadecimal integer (%x).
    pub fn hex(&mut self) -> Option<u32> {
        self.skip_whitespace();
        if self.i >= self.s.len() {
            return None;
        }
        let start = self.i;
        let mut sign = 1;
        if self.s[self.i] == b'+' || self.s[self.i] == b'-' {
            if self.s[self.i] == b'-' {
                sign = -1;
            }
            self.i += 1;
        }
        // Optional "0x" or "0X"
        if self.i + 1 < self.s.len() && self.s[self.i] == b'0' && (self.s[self.i + 1] == b'x' || self.s[self.i + 1] == b'X') {
            self.i += 2;
            // Must be followed by a hex digit
            if self.i >= self.s.len() || !is_hex_digit(self.s[self.i]) {
                return None;
            }
        }
        // At least one hex digit
        if self.i >= self.s.len() || !is_hex_digit(self.s[self.i]) {
            return None;
        }
        let mut val: u64 = 0;
        while self.i < self.s.len() && is_hex_digit(self.s[self.i]) {
            let digit = if self.s[self.i].is_ascii_digit() {
                self.s[self.i] - b'0'
            } else {
                (self.s[self.i].to_ascii_lowercase() - b'a') + 10
            };
            val = val.wrapping_mul(16).wrapping_add(digit as u64);
            self.i += 1;
        }
        val = val.wrapping_mul(sign as u64);
        Some(val as u32)
    }

    /// Parses a float (%f).
    pub fn float(&mut self) -> Option<f32> {
        self.skip_whitespace();
        if self.i >= self.s.len() {
            return None;
        }
        let start = self.i;
        
        // Check for sign
        if self.i < self.s.len() && (self.s[self.i] == b'+' || self.s[self.i] == b'-') {
            self.i += 1;
        }
        
        // Find the end of the float string
        let mut end = self.i;
        let mut has_digit = false;
        let mut has_dot = false;
        
        while end < self.s.len() {
            let b = self.s[end];
            if b.is_ascii_digit() {
                has_digit = true;
                end += 1;
            } else if b == b'.' {
                if has_dot { break; } // Only one dot allowed
                has_dot = true;
                end += 1;
            } else if b == b'e' || b == b'E' {
                break;
            } else {
                break;
            }
        }
        
        // Check if we have at least one digit
        if !has_digit {
            return None;
        }
        
        // Handle exponent part
        if end < self.s.len() && (self.s[end] == b'e' || self.s[end] == b'E') {
            end += 1;
            // Optional sign
            if end < self.s.len() && (self.s[end] == b'+' || self.s[end] == b'-') {
                end += 1;
            }
            // Must have at least one digit after exponent indicator
            if end >= self.s.len() || !self.s[end].is_ascii_digit() {
                // Matching failure: "1e" or "1e+"
                return None;
            }
            while end < self.s.len() && self.s[end].is_ascii_digit() {
                end += 1;
            }
        }
        
        let s = std::str::from_utf8(&self.s[start..end]).ok()?;
        self.i = end;
        s.parse::<f32>().ok()
    }
}

fn is_hex_digit(b: u8) -> bool {
    b.is_ascii_hexdigit()
}

/// atof (strtod): the longest decimal float prefix, correctly rounded to f64; 0 when none.
pub fn atof(w: &[u8]) -> f64 {
    let mut i = 0;
    while i < w.len() && (w[i] == b' ' || (b'\t'..=b'\r').contains(&w[i])) {
        i += 1;
    }
    let start = i;
    if i < w.len() && (w[i] == b'+' || w[i] == b'-') {
        i += 1;
    }
    let digits = |i: &mut usize| {
        let s = *i;
        while *i < w.len() && w[*i].is_ascii_digit() {
            *i += 1;
        }
        *i - s
    };
    let mut n = digits(&mut i);
    if i < w.len() && w[i] == b'.' {
        i += 1;
        n += digits(&mut i);
    }
    if n == 0 {
        return 0.0;
    }
    let mantissa_end = i;
    if i < w.len() && (w[i] == b'e' || w[i] == b'E') {
        let mut j = i + 1;
        if j < w.len() && (w[j] == b'+' || w[j] == b'-') {
            j += 1;
        }
        if digits(&mut j) > 0 {
            i = j;
        }
    }
    let _ = mantissa_end;
    std::str::from_utf8(&w[start..i]).ok().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0)
}

/// atoi (strtol base 10, wrapped to int).
pub fn atoi(w: &[u8]) -> i32 {
    let mut i = 0;
    while i < w.len() && (w[i] == b' ' || (b'\t'..=b'\r').contains(&w[i])) {
        i += 1;
    }
    let neg = i < w.len() && w[i] == b'-';
    if i < w.len() && (w[i] == b'+' || w[i] == b'-') {
        i += 1;
    }
    let mut v: i64 = 0;
    while i < w.len() && w[i].is_ascii_digit() {
        v = v.wrapping_mul(10).wrapping_add((w[i] - b'0') as i64);
        i += 1;
    }
    (if neg { v.wrapping_neg() } else { v }) as i32
}

