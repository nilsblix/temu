mod ansi;
mod terminal_action;

use std::os::fd::OwnedFd;

use nix::{
    errno::Errno,
    fcntl::{FcntlArg, OFlag, fcntl},
    pty::{ForkptyResult, forkpty},
};

use ansi::{AnsiParser, AnsiResult};
use terminal_action::{EraseMode, TerminalAction};

fn spawn_pty_with_shell(window_size: WindowSize) -> OwnedFd {
    let win_size = nix::pty::Winsize {
        ws_row: window_size.rows as u16,
        ws_col: window_size.cols as u16,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };

    unsafe {
        match forkpty(&win_size, None) {
            Ok(ForkptyResult::Parent { master, child: _ }) => {
                let flags = OFlag::from_bits_truncate(fcntl(&master, FcntlArg::F_GETFL).unwrap());
                fcntl(&master, FcntlArg::F_SETFL(flags | OFlag::O_NONBLOCK)).unwrap();
                master
            }
            Ok(ForkptyResult::Child) => {
                let shell_name = c"bash";

                match nix::unistd::execvp(&shell_name, &[shell_name]) {
                    Ok(infallible) => match infallible {},
                    Err(e) => {
                        println!("could not spawn shell: {e}");
                        std::process::exit(127);
                    }
                }
            }
            Err(e) => {
                panic!("error: could not fork into pty: {e}");
            }
        }
    }
}

/// 0-indexed.
#[derive(Clone, Default)]
pub struct CursorPos {
    pub row: usize,
    pub col: usize,
}

#[derive(Clone)]
pub struct WindowSize {
    pub rows: usize,
    pub cols: usize,
}

pub struct TerminalEmulator {
    fd: OwnedFd,
    buf: Vec<u8>,
    cursor: CursorPos,
    window_size: WindowSize,
    parser: AnsiParser,
}

impl TerminalEmulator {
    pub fn spawn(window_size: WindowSize) -> TerminalEmulator {
        let fd = spawn_pty_with_shell(window_size.clone());
        TerminalEmulator {
            fd,
            buf: vec![b' '; window_size.rows * window_size.rows],
            cursor: CursorPos::default(),
            window_size,
            parser: AnsiParser::default(),
        }
    }

    pub fn cursor(&self) -> CursorPos {
        self.cursor.clone()
    }

    fn cursor_idx(&self) -> usize {
        self.cursor.col + self.window_size.cols * self.cursor.row
    }

    fn put_byte_onto_screen(&mut self, b: u8) {
        let idx = self.cursor_idx();
        self.buf[idx] = b;
        if self.cursor.col + 1 < self.window_size.cols {
            self.cursor.col += 1
        } else {
            self.cursor.col = 0;
            self.line_feed();
        }
    }

    fn process_byte(&mut self, b: u8) {
        match b {
            b'\r' => self.cursor.col = 0,
            b'\n' => self.line_feed(),
            b'\x08' => self.cursor.col = self.cursor.col.saturating_sub(1),
            b'\t' => {
                let next_tab = ((self.cursor.col / 8) + 1) * 8;
                self.cursor.col = next_tab.min(self.window_size.cols - 1);
            }
            b' '..b'~' => {
                self.put_byte_onto_screen(b);
            }
            _ => {}
        }
    }

    fn line_feed(&mut self) {
        if self.cursor.row + 1 < self.window_size.rows {
            self.cursor.row += 1;
        } else {
            self.buf.copy_within(self.window_size.cols.., 0);
            let last_row = (self.window_size.rows - 1) * self.window_size.cols;
            self.buf[last_row..].fill(b' ');
        }
    }

    pub fn write(&self, bytes: &[u8]) {
        let mut to_write = bytes;
        while to_write.len() > 0 {
            match nix::unistd::write(&self.fd, bytes) {
                Ok(n) => to_write = &to_write[n..],
                Err(e) => println!("failed to write to shell: {e}"),
            }
        }
    }

    fn read_from_fd(&self) -> Vec<u8> {
        let mut incoming = [0u8; 4096];
        match nix::unistd::read(&self.fd, &mut incoming) {
            Ok(n) => {
                let mut v = Vec::new();
                v.extend_from_slice(&incoming[0..n]);
                return v;
            }
            Err(Errno::EAGAIN) => {}
            Err(e) => {
                println!("failed to read from shell: {e}");
            }
        }

        Vec::new()
    }

    pub fn read(&mut self) {
        let bytes = self.read_from_fd();
        for b in bytes {
            match self.parser.next(b) {
                AnsiResult::Passthrough(b) => self.process_byte(b),
                AnsiResult::CurrentlyParsing => {}
                AnsiResult::Action(action) => match action {
                    TerminalAction::CursorPosition { row, col } => {
                        self.cursor = CursorPos {
                            row: row - 1,
                            col: col - 1,
                        }
                    }
                    TerminalAction::CursorUp(n) => {
                        self.cursor.row = self.cursor.row.saturating_sub(n)
                    }
                    TerminalAction::CursorDown(n) => {
                        self.cursor.row = (self.cursor.row + n).min(self.window_size.rows - 1)
                    }
                    TerminalAction::CursorForward(n) => {
                        self.cursor.col = (self.cursor.col + n).min(self.window_size.cols - 1)
                    }
                    TerminalAction::CursorBackward(n) => {
                        self.cursor.col = self.cursor.col.saturating_sub(n)
                    }
                    TerminalAction::Erase(EraseMode::ToBeginning) => {
                        let idx = self.cursor_idx();
                        self.buf[0..idx].fill(b' ');
                    }
                    TerminalAction::Erase(EraseMode::Scrollback) => self.buf.fill(b' '),
                    TerminalAction::Erase(EraseMode::ToEndOfScreen) => {
                        let idx = self.cursor_idx();
                        self.buf[idx..].fill(b' ');
                    }
                },
                AnsiResult::InvalidAnsi(b) => {
                    println!(
                        "found invalid ansi when reading: `{}`, ignoring...",
                        b as char
                    );
                }
            }
        }
    }

    pub fn display_text(&self) -> String {
        let mut ret = String::with_capacity(self.window_size.rows * (self.window_size.cols + 1));
        for (row_idx, row) in self.buf.chunks(self.window_size.cols).enumerate() {
            ret.extend(row.iter().map(|b| char::from(*b)));
            if row_idx + 1 < self.window_size.rows {
                ret.push('\n');
            }
        }
        ret
    }
}
