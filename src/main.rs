mod gui;
mod terminal_emulator;

use terminal_emulator::TerminalEmulator;

fn main() {
    let emu = TerminalEmulator::spawn();
    gui::TemuGui::run(emu, 14.0);
}
