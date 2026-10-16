use std::fmt;

fn parse_u32(bytes: &[u8]) -> Option<u32> {
    bytes.iter().try_fold(0u32, |value, &byte| {
        let digit = byte.checked_sub(b'0')?;

        if digit > 9 {
            return None;
        }

        value.checked_mul(10)?.checked_add(u32::from(digit))
    })
}

enum CsiState {
    Waiting,
    Escaped,
    ExpectingParams,
    FoundParam,
}

fn is_csi_ending(b: u8) -> bool {
    match b {
        b'H' | b'J' => true,
        _ => false,
    }
}

struct Params(Vec<u32>);

impl fmt::Display for Params {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (idx, val) in self.0.iter().enumerate() {
            if idx != 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", val)?;
        }
        Ok(())
    }
}

struct NumberBucket(Vec<u8>);

impl fmt::Display for NumberBucket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (idx, val) in self.0.iter().enumerate() {
            if idx != 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", val)?;
        }
        Ok(())
    }
}

pub struct AnsiParser {
    state: CsiState,
    params: Params,
    number_bucket: NumberBucket,
}

impl Default for AnsiParser {
    fn default() -> AnsiParser {
        AnsiParser {
            state: CsiState::Waiting,
            params: Params(Vec::new()),
            number_bucket: NumberBucket(Vec::new()),
        }
    }
}

pub enum TerminalAction {
    ClearEntireScreen,
}

pub enum ParsedResult {
    Passthrough(u8),
    CurrentlyParsing,
    Action(TerminalAction),
    InvalidAnsi(u8),
}

impl AnsiParser {
    pub fn next(&mut self, b: u8) -> ParsedResult {
        println!("next: parsing `{}`", b as char);
        match self.state {
            CsiState::Waiting if b == b'\x1b' => {
                self.state = CsiState::Escaped;
                ParsedResult::CurrentlyParsing
            }
            CsiState::Escaped if b == b'[' => {
                self.state = CsiState::ExpectingParams;
                ParsedResult::CurrentlyParsing
            }
            CsiState::ExpectingParams => {
                if is_csi_ending(b) {
                    return self.terminate_current_parse(b);
                }

                if b.is_ascii_digit() {
                    self.number_bucket.0.push(b);
                    self.state = CsiState::FoundParam;
                    return ParsedResult::CurrentlyParsing;
                }

                println!("malformed ansi: expected digit, found: `{}`", b as char);
                self.reset_on_failure();
                ParsedResult::InvalidAnsi(b)
            }
            CsiState::FoundParam => {
                if is_csi_ending(b) {
                    return self.terminate_current_parse(b);
                }

                if b == b';' {
                    self.collect_bucket();
                    self.state = CsiState::ExpectingParams;
                    return ParsedResult::CurrentlyParsing;
                }

                if b.is_ascii_digit() {
                    let digit = b - b'0';
                    self.number_bucket.0.push(digit);
                    self.state = CsiState::FoundParam;
                    return ParsedResult::CurrentlyParsing;
                }

                println!(
                    "malformed ansi: expected number, semicolon or a terminator, found: `{}`",
                    b as char
                );
                self.reset_on_failure();
                ParsedResult::InvalidAnsi(b)
            }
            _ => ParsedResult::Passthrough(b),
        }
    }

    fn collect_bucket(&mut self) -> Option<()> {
        let param = parse_u32(&self.number_bucket.0)?;
        self.params.0.push(param);
        self.number_bucket.0.clear();
        Some(())
    }

    fn reset_on_failure(&mut self) {
        self.state = CsiState::Waiting;
        self.params.0.clear();
        self.number_bucket.0.clear();
    }

    fn terminate_current_parse(&mut self, b: u8) -> ParsedResult {
        match self.collect_bucket() {
            Some(()) => {}
            None => {
                println!(
                    "malformed ansi: expected param to parse as a u32, found: `{}`",
                    self.number_bucket
                );
                self.reset_on_failure();
                return ParsedResult::InvalidAnsi(b);
            }
        };
        self.state = CsiState::Waiting;
        match (self.params.0.as_slice(), b) {
            (&[2], b'J') => {
                self.params.0.clear();
                ParsedResult::Action(TerminalAction::ClearEntireScreen)
            }
            _ => {
                self.params.0.clear();
                println!(
                    "found unimplemented CSI terminator: `{}`, with params `{}`",
                    b as char, self.params
                );
                self.reset_on_failure();
                ParsedResult::InvalidAnsi(b)
            }
        }
    }
}
