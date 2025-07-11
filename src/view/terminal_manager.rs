use std::io::stdout;

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, ClearType, EnterAlternateScreen, LeaveAlternateScreen,
    },
};

pub struct TerminalManager {
    initialized: bool,
}

impl TerminalManager {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let mut manager = Self { initialized: false };
        manager.initialize()?;
        Ok(manager)
    }

    fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if enable_raw_mode().is_err() {
            return Err("Failed to enable raw mode - terminal not available".into());
        }

        let mut stdout = stdout();
        if execute!(
            stdout,
            EnterAlternateScreen,
            EnableMouseCapture,
            crossterm::terminal::Clear(ClearType::All)
        )
        .is_err()
        {
            let _ = disable_raw_mode();
            return Err("Failed to initialize terminal display".into());
        }

        self.initialized = true;
        Ok(())
    }

    #[allow(dead_code)] // Future terminal management architecture
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }
}

impl Drop for TerminalManager {
    fn drop(&mut self) {
        if self.initialized {
            let mut stdout = stdout();
            let _ = execute!(stdout, LeaveAlternateScreen, DisableMouseCapture);
            let _ = disable_raw_mode();
        }
    }
}

impl Default for TerminalManager {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self { initialized: false })
    }
}
