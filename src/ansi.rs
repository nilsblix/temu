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

/// Taken from https://gist.github.com/fnky/458719343aabd01cfb17a3a4f7296797
fn is_csi_terminator(b: u8) -> bool {
    b"HfABCDEFGnMsuJKm".contains(&b)
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
            write!(f, "{}", *val as char)?;
        }
        Ok(())
    }
}

// TODO: Factor this out into `CsiParser` and make `AnsiParser` an enum over
// several parsers, which enables us to parse several different escape
// sequences.
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

/// Taken from https://gist.github.com/fnky/458719343aabd01cfb17a3a4f7296797
pub enum TerminalAction {
    CursorToOrigin,
    EraseFromCursorToBeginningOfScreen,
    EraseScrollback,
    EraseFromCursorUntilEndOfScreen,
}

impl TerminalAction {
    fn from_params_and_terminator(params: &[u32], terminator: u8) -> Option<TerminalAction> {
        Some(match (params, terminator) {
            (&[], b'H') => Self::CursorToOrigin,
            (&[], b'J') | (&[0], b'J') => Self::EraseFromCursorUntilEndOfScreen,
            (&[2], b'J') => Self::EraseFromCursorToBeginningOfScreen,
            (&[3], b'J') => Self::EraseScrollback,
            _ => return None,
        })
    }
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
                if is_csi_terminator(b) {
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
                if is_csi_terminator(b) {
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
        if !self.number_bucket.0.is_empty() {
            let param = parse_u32(&self.number_bucket.0)?;
            self.params.0.push(param);
            self.number_bucket.0.clear();
        }
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
        let action_result = TerminalAction::from_params_and_terminator(&self.params.0, b);
        let ret = match action_result {
            Some(action) => ParsedResult::Action(action),
            None => {
                println!(
                    "found unhandled CSI terminator: `{}`, with params `{}`",
                    b as char, self.params
                );
                self.reset_on_failure();
                ParsedResult::InvalidAnsi(b)
            }
        };
        self.params.0.clear();
        ret
    }
}
