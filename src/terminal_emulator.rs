use std::os::fd::OwnedFd;

use super::ansi::{AnsiParser, ParsedResult, TerminalAction};

use nix::errno::Errno;
use nix::fcntl::{FcntlArg, OFlag, fcntl};
use nix::pty::{ForkptyResult, forkpty};

fn spawn_pty_with_shell() -> OwnedFd {
    unsafe {
        match forkpty(None, None) {
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

/// 0-indexed into `TerminalEmulator.buf`
#[derive(Clone, Default)]
pub struct CursorPos(usize);

impl CursorPos {
    /// Returns the 1-indexed visual position of the cursor.
    pub fn visual_pos(&self, _window_size_in_chars: (usize, usize), data: &[u8]) -> (usize, usize) {
        let up_to_cursor = &data[0..self.0];
        let lines: Vec<_> = up_to_cursor.split(|b| *b == b'\n').collect();
        let num_chars_before_last = up_to_cursor.len() - lines.last().map(|x| x.len()).unwrap_or(0);
        (self.0 - num_chars_before_last + 1, lines.len())
    }
}

pub struct TerminalEmulator {
    fd: OwnedFd,
    buf: Vec<u8>,
    cursor: CursorPos,
    parser: AnsiParser,
}

impl TerminalEmulator {
    pub fn spawn() -> TerminalEmulator {
        let fd = spawn_pty_with_shell();
        TerminalEmulator {
            fd,
            buf: Vec::new(),
            cursor: CursorPos::default(),
            parser: AnsiParser::default(),
        }
    }

    pub fn cursor_visual_pos(&self, window_size_in_chars: (usize, usize)) -> (usize, usize) {
        self.cursor.visual_pos(window_size_in_chars, &self.buf)
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

    fn increment_cursor(&mut self) {
        self.cursor.0 += 1;
    }

    pub fn read(&mut self) {
        let bytes = self.read_from_fd();
        for b in bytes {
            match self.parser.next(b) {
                ParsedResult::Passthrough(b) => {
                    self.increment_cursor();
                    self.buf.push(b);
                }
                ParsedResult::CurrentlyParsing => {}
                ParsedResult::Action(action) => match action {
                    TerminalAction::CursorToOrigin => self.cursor = CursorPos::default(),
                    TerminalAction::EraseInDisplay => todo!(),
                    TerminalAction::EraseFromCursorToBeginningOfScreen => self.buf.clear(),
                },
                ParsedResult::InvalidAnsi(b) => {
                    println!(
                        "found invalid ansi when reading: `{}`, not adding to buf",
                        b as char
                    );
                }
            }
        }
    }

    pub fn data(&self) -> &[u8] {
        &self.buf
    }
}
