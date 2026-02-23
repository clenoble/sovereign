//! Camera panel stub — full implementation on the jiminy branch.

use std::sync::{Arc, Mutex};

use iced::widget::{column, container, text};
use iced::{Element, Length};

use crate::app::Message;
use crate::theme;

pub type SharedFrame = Arc<Mutex<Option<Vec<u8>>>>;

pub struct CameraPanel {
    pub visible: bool,
    _frame: SharedFrame,
}

impl CameraPanel {
    pub fn new(frame: SharedFrame) -> Self {
        Self {
            visible: false,
            _frame: frame,
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let content = column![
            text("Camera (not available)")
                .size(13)
                .color(theme::text_dim()),
        ]
        .padding(12)
        .width(200);

        container(content)
            .style(theme::skill_panel_style)
            .width(Length::Shrink)
            .into()
    }

    pub fn poll_frame(&mut self) {}
}
