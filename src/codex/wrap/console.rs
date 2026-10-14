#[cfg(windows)]
mod platform {
    use std::io;

    use windows_sys::Win32::System::Console::{
        CONSOLE_MODE, ENABLE_VIRTUAL_TERMINAL_INPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING, GetConsoleMode, GetStdHandle,
        STD_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, SetConsoleMode,
    };

    pub struct Mode {
        input: CONSOLE_MODE,
        output: CONSOLE_MODE,
    }

    fn get(handle: STD_HANDLE) -> CONSOLE_MODE {
        let mut mode = 0;
        unsafe { GetConsoleMode(GetStdHandle(handle), &mut mode) };
        mode
    }

    fn set(handle: STD_HANDLE, mode: CONSOLE_MODE) {
        unsafe { SetConsoleMode(GetStdHandle(handle), mode) };
    }

    impl Mode {
        pub fn enter() -> io::Result<Self> {
            let (input, output) = (get(STD_INPUT_HANDLE), get(STD_OUTPUT_HANDLE));
            crossterm::terminal::enable_raw_mode()?;
            set(STD_INPUT_HANDLE, get(STD_INPUT_HANDLE) | ENABLE_VIRTUAL_TERMINAL_INPUT);
            set(STD_OUTPUT_HANDLE, output | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
            Ok(Self { input, output })
        }
    }

    impl Drop for Mode {
        fn drop(&mut self) {
            let _ = crossterm::terminal::disable_raw_mode();
            set(STD_INPUT_HANDLE, self.input);
            set(STD_OUTPUT_HANDLE, self.output);
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use std::io;

    pub struct Mode;

    impl Mode {
        pub fn enter() -> io::Result<Self> {
            crossterm::terminal::enable_raw_mode()?;
            Ok(Self)
        }
    }

    impl Drop for Mode {
        fn drop(&mut self) {
            let _ = crossterm::terminal::disable_raw_mode();
        }
    }
}

pub use platform::Mode;
