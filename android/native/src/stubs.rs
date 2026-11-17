//! What the menu (src/settings.rs) asks of the desktop app that the phone
//! app and the web page don't have: updates (the app stores and the page
//! update themselves), the log file. The menu hides those rows here.

pub mod update {
    /// Where updating stands (the desktop's own; never anything here).
    #[allow(dead_code)]
    pub enum Status {
        Unsupported,
        Managed,
        Idle,
        Checking,
        UpToDate,
        Available(String),
        Installing(String),
        Installed(String),
        Failed(String),
    }

    pub fn current() -> &'static str {
        ""
    }

    pub fn status() -> Status {
        Status::Managed
    }

    pub fn running() -> bool {
        false
    }

    pub fn remote_status() -> Option<Status> {
        None
    }

    pub fn offer() -> Option<String> {
        None
    }
}

pub mod platform {
    /// No log file here.
    pub fn log_path() -> Option<std::path::PathBuf> {
        None
    }
}
