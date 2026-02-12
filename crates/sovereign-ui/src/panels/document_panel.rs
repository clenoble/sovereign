use iced::widget::{button, column, container, image, mouse_area, row, scrollable, text, text_editor, text_input, Space};
use iced::{ContentFit, Element, Length, Padding};

use sovereign_core::content::ContentImage;

use crate::app::Message;
use crate::theme;

/// Transparent margin around the visible panel that captures mouse events
/// without triggering any panel action — prevents canvas drag bleed-through.
pub const DEAD_ZONE: f32 = 10.0;

/// Height of the toolbar row used as a drag handle.
pub const DRAG_BAR_HEIGHT: f32 = 44.0;

/// A floating document editing panel.
pub struct FloatingPanel {
    pub doc_id: String,
    pub title: String,
    pub body: text_editor::Content,
    pub images: Vec<ContentImage>,
    pub position: iced::Point,
    pub size: iced::Size,
    pub visible: bool,
    // Drag state
    pub dragging: bool,
    pub last_local_cursor: iced::Point,
    pub drag_start_screen: Option<iced::Point>,
    pub drag_start_panel: Option<iced::Point>,
}

impl FloatingPanel {
    pub fn new(
        doc_id: String,
        title: String,
        body: String,
        images: Vec<ContentImage>,
    ) -> Self {
        Self {
            doc_id,
            title,
            body: text_editor::Content::with_text(&body),
            images,
            position: iced::Point::new(200.0, 100.0),
            size: iced::Size::new(700.0, 500.0),
            visible: true,
            dragging: false,
            last_local_cursor: iced::Point::ORIGIN,
            drag_start_screen: None,
            drag_start_panel: None,
        }
    }

    pub fn get_body_text(&self) -> String {
        self.body.text()
    }

    pub fn view(&self, index: usize) -> Element<'_, Message> {
        // Row 1 (toolbar): right-aligned Save + Close — this row is the drag handle
        let toolbar = row![
            Space::new().width(Length::Fill),
            button(text("Save").size(13))
                .on_press(Message::SaveDocument(index))
                .style(theme::skill_button_style)
                .padding(Padding::from([8, 16])),
            button(text("Close").size(13))
                .on_press(Message::CloseDocument(index))
                .style(theme::reject_button_style)
                .padding(Padding::from([8, 12])),
        ]
        .spacing(8)
        .padding(Padding::from([8, 12]));

        // Row 2 (title): full-width title input on its own line
        let title_row = text_input("Document title", &self.title)
            .on_input(move |t| Message::DocTitleChanged {
                panel_idx: index,
                title: t,
            })
            .style(theme::search_input_style)
            .padding(Padding::from([8, 12]))
            .size(14)
            .width(Length::Fill);

        let header = column![toolbar, title_row].spacing(0);

        // Body: text editor
        let editor = text_editor(&self.body)
            .on_action(move |action| Message::DocBodyAction {
                panel_idx: index,
                action,
            })
            .size(14)
            .padding(Padding::from([8, 12]))
            .height(Length::Fill);

        let mut content = column![header, editor].spacing(0);

        // Image gallery (if any) — thumbnail + caption per image
        if !self.images.is_empty() {
            let mut gallery = row![].spacing(8).padding(8);
            for img in &self.images {
                let filename = std::path::Path::new(&img.path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| img.path.clone());
                let caption = if !img.caption.is_empty() {
                    img.caption.clone()
                } else {
                    filename
                };
                let thumbnail = image(&img.path)
                    .width(80)
                    .height(60)
                    .content_fit(ContentFit::Cover);
                let card = column![
                    thumbnail,
                    text(caption).size(11).color(theme::TEXT_DIM),
                ]
                .spacing(2)
                .width(80);
                gallery = gallery.push(card);
            }
            content = content.push(scrollable(gallery).direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::default(),
            )));
        }

        // mouse_area captures all events in the dead-zone + panel area,
        // preventing leakthrough to the canvas shader underneath.
        let panel = mouse_area(
            // Dead zone: 10px transparent padding around the visible panel.
            // Events here are swallowed by mouse_area but don't trigger drag.
            container(
                container(content)
                    .width(self.size.width)
                    .height(self.size.height)
                    .style(theme::document_panel_style),
            )
            .padding(DEAD_ZONE),
        )
        .on_press(Message::PanelDragStart(index))
        .on_release(Message::PanelDragEnd(index))
        .on_move(move |p| Message::PanelDragMove { panel_idx: index, local: p })
        .on_scroll(|_| Message::Ignore);

        // Outer: positions the panel so the *visible* panel sits at self.position
        // (offset by -DEAD_ZONE to compensate for the dead zone padding).
        container(panel)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(
                Padding::ZERO
                    .top((self.position.y - DEAD_ZONE).max(0.0))
                    .left((self.position.x - DEAD_ZONE).max(0.0)),
            )
            .into()
    }
}
