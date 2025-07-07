use chrono::{DateTime, Local};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Notification {
    pub message: String,
    pub created_at: DateTime<Local>,
    pub duration_seconds: u64,
    pub notification_type: NotificationType,
}

#[derive(Clone, Debug)]
pub enum NotificationType {
    Info,
    Warning,
    Error,
    Status,
}

impl Notification {
    pub fn new(message: String, notification_type: NotificationType) -> Self {
        Self {
            message,
            created_at: Local::now(),
            duration_seconds: 4, // Default 4 seconds
            notification_type,
        }
    }

    pub fn with_duration(
        message: String,
        notification_type: NotificationType,
        duration_seconds: u64,
    ) -> Self {
        Self {
            message,
            created_at: Local::now(),
            duration_seconds,
            notification_type,
        }
    }

    pub fn is_expired(&self) -> bool {
        let elapsed = Local::now()
            .signed_duration_since(self.created_at)
            .to_std()
            .unwrap_or(Duration::from_secs(0));

        elapsed >= Duration::from_secs(self.duration_seconds)
    }

    pub fn remaining_time(&self) -> Duration {
        let elapsed = Local::now()
            .signed_duration_since(self.created_at)
            .to_std()
            .unwrap_or(Duration::from_secs(0));

        Duration::from_secs(self.duration_seconds).saturating_sub(elapsed)
    }
}

#[derive(Clone, Debug, Default)]
pub struct NotificationManager {
    pub current_notification: Option<Notification>,
}

impl NotificationManager {
    pub fn new() -> Self {
        Self {
            current_notification: None,
        }
    }

    pub fn show(&mut self, message: String, notification_type: NotificationType) {
        self.current_notification = Some(Notification::new(message, notification_type));
    }

    pub fn show_with_duration(
        &mut self,
        message: String,
        notification_type: NotificationType,
        duration_seconds: u64,
    ) {
        self.current_notification = Some(Notification::with_duration(
            message,
            notification_type,
            duration_seconds,
        ));
    }

    pub fn clear(&mut self) {
        self.current_notification = None;
    }

    pub fn update(&mut self) {
        if let Some(notification) = &self.current_notification {
            if notification.is_expired() {
                self.current_notification = None;
            }
        }
    }

    pub fn get_current_message(&self) -> Option<&str> {
        self.current_notification
            .as_ref()
            .map(|n| n.message.as_str())
    }

    pub fn get_current_notification(&self) -> Option<&Notification> {
        self.current_notification.as_ref()
    }

    pub fn has_notification(&self) -> bool {
        self.current_notification.is_some()
    }
}

// Helper functions for common notification types
impl NotificationManager {
    pub fn info(&mut self, message: String) {
        self.show(message, NotificationType::Info);
    }

    pub fn warning(&mut self, message: String) {
        self.show(message, NotificationType::Warning);
    }

    pub fn error(&mut self, message: String) {
        self.show(message, NotificationType::Error);
    }

    pub fn status(&mut self, message: String) {
        self.show(message, NotificationType::Status);
    }

    pub fn persistent_status(&mut self, message: String) {
        self.show_with_duration(message, NotificationType::Status, u64::MAX); // Never expires
    }
}
