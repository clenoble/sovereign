use std::sync::{Arc, Mutex};

use iced::widget::{button, column, container, image, row, text, Space};
use iced::{Element, Length, Padding};

use crate::app::Message;
use crate::theme;

/// Shared camera frame — JPEG bytes from the Jiminy poller.
/// `None` means no frame has arrived yet (or camera is unavailable).
pub type SharedFrame = Arc<Mutex<Option<Vec<u8>>>>;

/// Floating panel that displays the live camera feed from Jiminy.
pub struct CameraPanel {
    pub visible: bool,
    shared_frame: SharedFrame,
    /// Cached image handle — updated each tick when new JPEG arrives.
    current_handle: Option<image::Handle>,
    /// Track last frame length to detect changes without comparing bytes.
    last_frame_len: usize,
}

impl CameraPanel {
    pub fn new(shared_frame: SharedFrame) -> Self {
        Self {
            visible: false,
            shared_frame,
            current_handle: None,
            last_frame_len: 0,
        }
    }

    /// Check for a new frame from the poller. Call once per tick.
    pub fn poll_frame(&mut self) {
        if !self.visible {
            return;
        }
        if let Ok(guard) = self.shared_frame.lock() {
            if let Some(ref jpeg) = *guard {
                // Only rebuild handle if frame changed (length check is cheap)
                if jpeg.len() != self.last_frame_len || self.current_handle.is_none() {
                    self.last_frame_len = jpeg.len();
                    self.current_handle =
                        Some(image::Handle::from_bytes(jpeg.clone()));
                }
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let header = row![
            text("Camera").size(14).color(theme::text_primary()),
            Space::new().width(Length::Fill),
            button(text("X").size(12))
                .on_press(Message::CameraToggled)
                .style(theme::taskbar_button_style)
                .padding(Padding::from([2, 8])),
        ]
        .spacing(8)
        .padding(Padding::from([4, 8]));

        let body: Element<'_, Message> = if let Some(ref handle) = self.current_handle {
            image(handle.clone())
                .width(Length::Fixed(320.0))
                .into()
        } else {
            container(
                text("No camera feed")
                    .size(12)
                    .color(theme::text_dim()),
            )
            .width(Length::Fixed(320.0))
            .height(Length::Fixed(180.0))
            .center(Length::Fill)
            .into()
        };

        let panel = column![header, body].spacing(4);

        // Position: bottom-right, above taskbar
        container(
            container(panel)
                .style(theme::document_panel_style)
                .padding(8)
                .width(Length::Shrink),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_right(Length::Fill)
        .align_bottom(Length::Fill)
        .padding(Padding::ZERO.right(20.0).bottom(50.0))
        .into()
    }
}
