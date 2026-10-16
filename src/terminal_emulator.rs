use std::os::fd::OwnedFd;

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

/// 1-indexed.
#[derive(Clone)]
pub struct CursorPos {
    pub row: usize,
    pub col: usize,
}

impl Default for CursorPos {
    fn default() -> Self {
        Self { row: 1, col: 1 }
    }
}

pub struct TerminalEmulator {
    fd: OwnedFd,
    buf: Vec<u8>,
    cursor: CursorPos,
}

impl TerminalEmulator {
    pub fn spawn() -> TerminalEmulator {
        let fd = spawn_pty_with_shell();
        TerminalEmulator {
            fd,
            buf: Vec::new(),
            cursor: CursorPos::default(),
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

    fn increment_cursor(&mut self, bytes: &[u8]) {
        for b in bytes {
            // TODO: This assumes that the screen is infinite. We should instead
            // take the window size, and calculate the cursor with respect to
            // that.
            match *b {
                b'\n' => {
                    self.cursor.row += 1;
                    self.cursor.col = 1;
                }
                _ => self.cursor.col += 1,
            }
        }
    }

    pub fn read(&mut self) {
        let mut bytes = self.read_from_fd();
        self.increment_cursor(&bytes);
        self.buf.append(&mut bytes);
    }

    pub fn cursor(&self) -> CursorPos {
        self.cursor.clone()
    }

    pub fn data(&self) -> &[u8] {
        &self.buf
    }
}
