// Copyright 2025 Lablup Inc. and Jeongkyu Shin
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::io::Write;

use crossterm::style::Color;

use crate::common::config::{AppConfig, ThemeConfig};
use crate::ui::text::print_colored_text;

/// Enhanced progress bar with consistent styling and configuration
#[allow(dead_code)] // Future progress bar architecture
pub struct ProgressBar;

#[allow(dead_code)] // Future progress bar architecture
impl ProgressBar {
    /// Draw a progress bar with configurable styling
    pub fn draw<W: Write>(
        stdout: &mut W,
        label: &str,
        value: f64,
        max_value: f64,
        width: usize,
        options: ProgressBarOptions,
    ) {
        let formatted_label = Self::format_label(label);
        let available_bar_width = width.saturating_sub(
            AppConfig::PROGRESS_BAR_LABEL_WIDTH + AppConfig::PROGRESS_BAR_BRACKET_WIDTH + 1,
        );

        let fill_ratio = (value / max_value).clamp(0.0, 1.0);
        let filled_width = (available_bar_width as f64 * fill_ratio) as usize;

        let color = options
            .color
            .unwrap_or_else(|| ThemeConfig::progress_bar_color(fill_ratio));
        let display_text = Self::format_display_text(value, max_value, fill_ratio, &options);

        // Print label and opening bracket
        print_colored_text(stdout, &formatted_label, Color::White, None, None);
        print_colored_text(stdout, ": [", Color::White, None, None);

        // Draw bar with text overlay
        Self::draw_bar_content(
            stdout,
            available_bar_width,
            filled_width,
            &display_text,
            color,
        );

        // Print closing bracket
        print_colored_text(stdout, "]", Color::White, None, None);
    }

    /// Draw a simple bar (backward compatibility)
    pub fn draw_simple<W: Write>(
        stdout: &mut W,
        label: &str,
        value: f64,
        max_value: f64,
        width: usize,
        show_text: Option<String>,
    ) {
        let options = ProgressBarOptions {
            show_text,
            color: None,
            style: ProgressBarStyle::Filled,
            show_percentage: true,
        };
        Self::draw(stdout, label, value, max_value, width, options);
    }

    fn format_label(label: &str) -> String {
        if label.len() > AppConfig::PROGRESS_BAR_LABEL_WIDTH {
            label[..AppConfig::PROGRESS_BAR_LABEL_WIDTH].to_string()
        } else {
            format!(
                "{label:<width$}",
                width = AppConfig::PROGRESS_BAR_LABEL_WIDTH
            )
        }
    }

    fn format_display_text(
        value: f64,
        _max_value: f64,
        fill_ratio: f64,
        options: &ProgressBarOptions,
    ) -> String {
        if let Some(ref text) = options.show_text {
            if text.len() > AppConfig::PROGRESS_BAR_TEXT_WIDTH {
                text[..AppConfig::PROGRESS_BAR_TEXT_WIDTH].to_string()
            } else {
                format!("{text:>width$}", width = AppConfig::PROGRESS_BAR_TEXT_WIDTH)
            }
        } else if options.show_percentage {
            format!("{:>7.1}%", fill_ratio * 100.0)
        } else {
            format!(
                "{value:>width$.1}",
                width = AppConfig::PROGRESS_BAR_TEXT_WIDTH
            )
        }
    }

    fn draw_bar_content<W: Write>(
        stdout: &mut W,
        total_width: usize,
        filled_width: usize,
        display_text: &str,
        color: Color,
    ) {
        let text_len = display_text.len();
        let text_pos = total_width.saturating_sub(text_len);

        for i in 0..total_width {
            if i >= text_pos && i < text_pos + text_len {
                // Print text character with high contrast
                let char_index = i - text_pos;
                if let Some(ch) = display_text.chars().nth(char_index) {
                    print_colored_text(stdout, &ch.to_string(), Color::White, None, None);
                }
            } else if i < filled_width {
                // Print filled area
                print_colored_text(stdout, "▬", color, None, None);
            } else {
                // Print empty area
                print_colored_text(stdout, "─", Color::DarkGrey, None, None);
            }
        }
    }
}

/// Configuration options for progress bars
#[derive(Default)]
#[allow(dead_code)] // Future progress bar architecture
pub struct ProgressBarOptions {
    pub show_text: Option<String>,
    pub color: Option<Color>,
    pub style: ProgressBarStyle,
    pub show_percentage: bool,
}

#[allow(dead_code)] // Future progress bar architecture
impl ProgressBarOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_text(mut self, text: String) -> Self {
        self.show_text = Some(text);
        self
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    pub fn with_style(mut self, style: ProgressBarStyle) -> Self {
        self.style = style;
        self
    }

    pub fn with_percentage(mut self, show_percentage: bool) -> Self {
        self.show_percentage = show_percentage;
        self
    }
}

/// Different progress bar visual styles
#[derive(Default)]
#[allow(dead_code)] // Future progress bar architecture
pub enum ProgressBarStyle {
    #[default]
    Filled, // ▬▬▬▬──
    Blocks, // ████▓▓
    Dots,   // ●●●○○○
    Lines,  // ||||--
}

/// Specialized progress bars for different use cases
#[allow(dead_code)] // Future progress bar architecture
pub struct SpecializedBars;

#[allow(dead_code)] // Future progress bar architecture
impl SpecializedBars {
    /// GPU utilization bar with appropriate colors
    pub fn gpu_utilization<W: Write>(stdout: &mut W, utilization: f64, width: usize) {
        let options = ProgressBarOptions::new()
            .with_color(ThemeConfig::progress_bar_color(utilization / 100.0))
            .with_percentage(true);

        ProgressBar::draw(stdout, "GPU", utilization, 100.0, width, options);
    }

    /// Memory usage bar with custom text
    pub fn memory_usage<W: Write>(stdout: &mut W, used_gb: f64, total_gb: f64, width: usize) {
        let usage_percent = (used_gb / total_gb) * 100.0;
        let options = ProgressBarOptions::new()
            .with_text(format!("{used_gb:.1}GB"))
            .with_color(ThemeConfig::progress_bar_color(usage_percent / 100.0));

        ProgressBar::draw(stdout, "Mem", usage_percent, 100.0, width, options);
    }

    /// Temperature bar with heat-based colors
    pub fn temperature<W: Write>(stdout: &mut W, temp_celsius: u32, max_temp: u32, width: usize) {
        let temp_ratio = temp_celsius as f64 / max_temp as f64;
        let color = if temp_ratio > 0.9 {
            Color::Red
        } else if temp_ratio > 0.7 {
            Color::Yellow
        } else {
            Color::Green
        };

        let options = ProgressBarOptions::new()
            .with_text(format!("{temp_celsius}°C"))
            .with_color(color);

        ProgressBar::draw(
            stdout,
            "Temp",
            temp_celsius as f64,
            max_temp as f64,
            width,
            options,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_format_label() {
        assert_eq!(ProgressBar::format_label("GPU"), "GPU  ");
        assert_eq!(ProgressBar::format_label("VeryLongLabel"), "VeryL");
    }

    #[test]
    fn test_progress_bar_options() {
        let options = ProgressBarOptions::new()
            .with_text("50%".to_string())
            .with_color(Color::Green)
            .with_percentage(false);

        assert_eq!(options.show_text, Some("50%".to_string()));
        assert_eq!(options.color, Some(Color::Green));
        assert!(!options.show_percentage);
    }

    #[test]
    fn test_specialized_bars() {
        let mut buffer = Cursor::new(Vec::new());
        SpecializedBars::gpu_utilization(&mut buffer, 75.0, 40);
        // Verify that something was written
        assert!(!buffer.get_ref().is_empty());
    }
}
