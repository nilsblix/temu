pub enum EraseMode {
    ToBeginning,
    Scrollback,
    ToEndOfScreen,
}

/// Taken from https://gist.github.com/fnky/458719343aabd01cfb17a3a4f7296797
///
/// Note that these are defined via the standard, for example CursorPosition is
/// 1-indexed, as opposed to `terminal_emulator::CursorPosition` which is
/// 0-indexed.
pub enum TerminalAction {
    CursorPosition { row: usize, col: usize },
    CursorUp(usize),
    CursorDown(usize),
    CursorForward(usize),
    CursorBackward(usize),
    Erase(EraseMode),
}

impl TerminalAction {
    pub fn from_params_and_terminator(params: &[u32], terminator: u8) -> Option<TerminalAction> {
        Some(match (params, terminator) {
            (&[], b'H') => Self::CursorPosition { row: 1, col: 1 },
            (&[row, col], b'H') => Self::CursorPosition {
                row: row as usize,
                col: col as usize,
            },
            (&[n], b'A') => Self::CursorUp(n as usize),
            (&[n], b'B') => Self::CursorDown(n as usize),
            (&[n], b'C') => Self::CursorForward(n as usize),
            (&[n], b'D') => Self::CursorBackward(n as usize),
            (&[], b'J') | (&[0], b'J') => Self::Erase(EraseMode::ToEndOfScreen),
            (&[2], b'J') => Self::Erase(EraseMode::ToBeginning),
            (&[3], b'J') => Self::Erase(EraseMode::Scrollback),
            _ => return None,
        })
    }
}
