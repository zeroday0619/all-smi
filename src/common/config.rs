/// Application configuration constants
pub struct AppConfig;

impl AppConfig {
    // UI Rendering Constants
    pub const MIN_RENDER_INTERVAL_MS: u64 = 33; // ~30 FPS
    pub const EVENT_POLL_TIMEOUT_MS: u64 = 50;
    pub const SCROLL_UPDATE_FREQUENCY: u64 = 2; // Every 2 frames

    // Network Configuration
    pub const MAX_CONCURRENT_CONNECTIONS: usize = 64;
    pub const CONNECTION_TIMEOUT_SECS: u64 = 5;
    pub const POOL_IDLE_TIMEOUT_SECS: u64 = 60;
    pub const POOL_MAX_IDLE_PER_HOST: usize = 200;
    pub const TCP_KEEPALIVE_SECS: u64 = 30;
    pub const HTTP2_KEEPALIVE_SECS: u64 = 30;
    pub const RETRY_ATTEMPTS: u32 = 3;
    pub const RETRY_BASE_DELAY_MS: u64 = 50;

    // Data Collection
    pub const DEFAULT_UPDATE_INTERVAL_SECS: u64 = 2;
    pub const HISTORY_MAX_ENTRIES: usize = 100;
    pub const CONNECTION_STAGGER_BASE_MS: u64 = 500;

    // UI Layout Constants
    pub const PROGRESS_BAR_LABEL_WIDTH: usize = 5;
    pub const PROGRESS_BAR_BRACKET_WIDTH: usize = 4; // ": [" + "]"
    pub const PROGRESS_BAR_TEXT_WIDTH: usize = 8;
    pub const DASHBOARD_ITEM_WIDTH: usize = 15;
    pub const DEFAULT_TERMINAL_WIDTH: u16 = 80;
    pub const DEFAULT_TERMINAL_HEIGHT: u16 = 24;

    // Memory and Performance
    pub const LINUX_PAGE_SIZE_BYTES: u64 = 4096;
    pub const LINUX_JIFFIES_PER_SECOND: u64 = 100;
    pub const NOTIFICATION_DURATION_SECS: u64 = 5;

    // Color Thresholds
    pub const CRITICAL_THRESHOLD: f64 = 0.8;
    pub const WARNING_THRESHOLD: f64 = 0.7;
    pub const NORMAL_THRESHOLD: f64 = 0.25;
    pub const LOW_THRESHOLD: f64 = 0.05;
}

/// Environment-specific configuration
pub struct EnvConfig;

impl EnvConfig {
    pub fn adaptive_interval(node_count: usize) -> u64 {
        match node_count {
            0..=1 => 2,
            2..=10 => 3,
            11..=50 => 4,
            51..=100 => 5,
            _ => 6,
        }
    }

    pub fn max_concurrent_connections(total_hosts: usize) -> usize {
        std::cmp::min(total_hosts, AppConfig::MAX_CONCURRENT_CONNECTIONS)
    }

    pub fn connection_stagger_delay(host_index: usize, total_hosts: usize) -> u64 {
        (host_index as u64 * AppConfig::CONNECTION_STAGGER_BASE_MS) / total_hosts as u64
    }

    pub fn retry_delay(attempt: u32) -> u64 {
        AppConfig::RETRY_BASE_DELAY_MS * attempt as u64
    }
}

/// UI Theme configuration
pub struct ThemeConfig;

impl ThemeConfig {
    pub fn progress_bar_color(fill_ratio: f64) -> crossterm::style::Color {
        use crossterm::style::Color;

        if fill_ratio > AppConfig::CRITICAL_THRESHOLD {
            Color::Red
        } else if fill_ratio > AppConfig::WARNING_THRESHOLD {
            Color::Yellow
        } else if fill_ratio > AppConfig::NORMAL_THRESHOLD {
            Color::Green
        } else if fill_ratio > AppConfig::LOW_THRESHOLD {
            Color::DarkGreen
        } else {
            Color::DarkGrey
        }
    }

    pub fn utilization_color(utilization: f64) -> crossterm::style::Color {
        use crossterm::style::Color;

        if utilization > 80.0 {
            Color::Red
        } else if utilization > 50.0 {
            Color::Yellow
        } else if utilization > 20.0 {
            Color::Green
        } else {
            Color::DarkGrey
        }
    }
}
