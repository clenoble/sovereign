use std::sync::Arc;

use iced::widget::{
    button, column, container, image, markdown, mouse_area, row, scrollable, text,
    text_editor, text_input, Space,
};
use iced::{ContentFit, Element, Length, Padding};

use sovereign_core::content::{ContentImage, ContentVideo};
use sovereign_db::schema::Commit;

use crate::app::Message;
use crate::theme;

/// Transparent margin around the visible panel that captures mouse events
/// without triggering any panel action — prevents canvas drag bleed-through.
pub const DEAD_ZONE: f32 = 10.0;

/// Height of the toolbar row used as a drag handle.
pub const DRAG_BAR_HEIGHT: f32 = 44.0;

/// Formatting actions for the markdown toolbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatKind {
    Bold,
    Italic,
    Heading1,
    Heading2,
    Heading3,
    BulletList,
    Code,
    Link,
}

/// A floating document editing panel.
pub struct FloatingPanel {
    pub doc_id: String,
    pub title: String,
    pub body: text_editor::Content,
    pub images: Vec<ContentImage>,
    pub videos: Vec<ContentVideo>,
    pub position: iced::Point,
    pub size: iced::Size,
    pub visible: bool,
    // Markdown preview
    pub preview_mode: bool,
    pub markdown_items: Vec<markdown::Item>,
    // Version history
    pub commits: Vec<Commit>,
    pub show_history: bool,
    pub selected_commit: Option<usize>,
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
        videos: Vec<ContentVideo>,
        commits: Vec<Commit>,
    ) -> Self {
        Self {
            doc_id,
            title,
            body: text_editor::Content::with_text(&body),
            images,
            videos,
            position: iced::Point::new(200.0, 100.0),
            size: iced::Size::new(700.0, 500.0),
            visible: true,
            preview_mode: false,
            markdown_items: Vec::new(),
            commits,
            show_history: false,
            selected_commit: None,
            dragging: false,
            last_local_cursor: iced::Point::ORIGIN,
            drag_start_screen: None,
            drag_start_panel: None,
        }
    }

    pub fn get_body_text(&self) -> String {
        self.body.text()
    }

    /// Apply a formatting action by wrapping selection or inserting markdown syntax.
    pub fn apply_format(&mut self, kind: FormatKind) {
        let selected = self.body.selection().unwrap_or_default();
        let replacement = match kind {
            FormatKind::Bold => format!("**{selected}**"),
            FormatKind::Italic => format!("*{selected}*"),
            FormatKind::Heading1 => format!("# {selected}"),
            FormatKind::Heading2 => format!("## {selected}"),
            FormatKind::Heading3 => format!("### {selected}"),
            FormatKind::BulletList => format!("- {selected}"),
            FormatKind::Code => {
                if selected.contains('\n') {
                    format!("```\n{selected}\n```")
                } else {
                    format!("`{selected}`")
                }
            }
            FormatKind::Link => format!("[{selected}](url)"),
        };
        self.body.perform(text_editor::Action::Edit(
            text_editor::Edit::Paste(Arc::new(replacement)),
        ));
    }

    /// Parse body text into markdown items for preview rendering.
    pub fn refresh_preview(&mut self) {
        let body = self.body.text();
        self.markdown_items = markdown::parse(&body).collect();
    }

    pub fn view(&self, index: usize) -> Element<'_, Message> {
        // Row 1 (toolbar): History toggle + right-aligned Save + Close
        let history_label = if self.show_history { "Editor" } else { "History" };
        let history_btn = button(text(history_label).size(13))
            .on_press(Message::ToggleHistory(index))
            .style(theme::skill_button_style)
            .padding(Padding::from([8, 16]));

        let preview_label = if self.preview_mode { "Edit" } else { "Preview" };
        let preview_btn = button(text(preview_label).size(13))
            .on_press(Message::TogglePreview(index))
            .style(theme::skill_button_style)
            .padding(Padding::from([8, 16]));

        let toolbar = row![
            history_btn,
            preview_btn,
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

        let content = if self.show_history {
            // History view: scrollable commit list
            let history_content = self.view_history(index);
            column![header, history_content].spacing(0)
        } else if self.preview_mode {
            // Markdown preview mode
            let style = markdown::Style::from_palette(theme::iced_theme().palette());
            let preview = markdown::view(
                &self.markdown_items,
                markdown::Settings::with_style(style),
            )
            .map(move |url| Message::MarkdownLinkClicked(url.to_string()));

            let preview_container = container(scrollable(preview).height(Length::Fill))
                .padding(Padding::from([8, 12]))
                .width(Length::Fill)
                .height(Length::Fill);

            let mut col = column![header, preview_container].spacing(0);
            col = self.append_media_gallery(col, index);
            col
        } else {
            // Normal editor view with formatting toolbar
            let format_bar = self.view_format_toolbar(index);

            let editor = text_editor(&self.body)
                .on_action(move |action| Message::DocBodyAction {
                    panel_idx: index,
                    action,
                })
                .size(14)
                .padding(Padding::from([8, 12]))
                .height(Length::Fill);

            let mut col = column![header, format_bar, editor].spacing(0);
            col = self.append_media_gallery(col, index);
            col
        };

        // mouse_area captures all events in the dead-zone + panel area,
        // preventing leakthrough to the canvas shader underneath.
        let panel = mouse_area(
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

    /// Formatting toolbar with markdown syntax buttons.
    fn view_format_toolbar(&self, index: usize) -> Element<'_, Message> {
        let fmt_btn = |label: &'static str, kind: FormatKind| -> Element<'_, Message> {
            button(text(label).size(12))
                .on_press(Message::FormatAction {
                    panel_idx: index,
                    kind,
                })
                .style(theme::skill_button_style)
                .padding(Padding::from([4, 8]))
                .into()
        };

        row![
            fmt_btn("B", FormatKind::Bold),
            fmt_btn("I", FormatKind::Italic),
            fmt_btn("H1", FormatKind::Heading1),
            fmt_btn("H2", FormatKind::Heading2),
            fmt_btn("H3", FormatKind::Heading3),
            fmt_btn("List", FormatKind::BulletList),
            fmt_btn("Code", FormatKind::Code),
            fmt_btn("Link", FormatKind::Link),
        ]
        .spacing(4)
        .padding(Padding::from([4, 12]))
        .into()
    }

    /// Append image and video galleries to the content column.
    fn append_media_gallery<'a>(
        &'a self,
        mut col: iced::widget::Column<'a, Message>,
        index: usize,
    ) -> iced::widget::Column<'a, Message> {
        // Image gallery
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
                    text(caption).size(11).color(theme::text_dim()),
                ]
                .spacing(2)
                .width(80);
                gallery = gallery.push(card);
            }
            col = col.push(scrollable(gallery).direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::default(),
            )));
        }

        // Video gallery
        if !self.videos.is_empty() {
            let mut gallery = row![].spacing(8).padding(8);
            for (i, video) in self.videos.iter().enumerate() {
                let filename = std::path::Path::new(&video.path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| video.path.clone());
                let caption = if !video.caption.is_empty() {
                    video.caption.clone()
                } else {
                    filename
                };

                let duration_text = video
                    .duration_secs
                    .map(|d| {
                        let mins = (d / 60.0) as u32;
                        let secs = (d % 60.0) as u32;
                        format!("{mins}:{secs:02}")
                    })
                    .unwrap_or_default();

                let thumb: Element<'_, Message> =
                    if let Some(ref thumb_path) = video.thumbnail_path {
                        image(thumb_path.as_str())
                            .width(80)
                            .height(60)
                            .content_fit(ContentFit::Cover)
                            .into()
                    } else {
                        container(text("VIDEO").size(11).color(theme::text_dim()))
                            .width(80)
                            .height(60)
                            .center_x(80)
                            .center_y(60)
                            .style(theme::document_panel_style)
                            .into()
                    };

                let play_btn = button(text("Play").size(10))
                    .on_press(Message::VideoPlay {
                        panel_idx: index,
                        video_idx: i,
                    })
                    .style(theme::skill_button_style)
                    .padding(Padding::from([2, 6]));

                let mut card = column![
                    thumb,
                    text(caption).size(11).color(theme::text_dim()),
                ]
                .spacing(2)
                .width(80);

                if !duration_text.is_empty() {
                    card = card.push(text(duration_text).size(10).color(theme::text_dim()));
                }
                card = card.push(play_btn);

                gallery = gallery.push(card);
            }
            col = col.push(scrollable(gallery).direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::default(),
            )));
        }

        col
    }

    /// Render the commit history list with optional snapshot preview.
    fn view_history(&self, index: usize) -> Element<'_, Message> {
        if self.commits.is_empty() {
            return container(
                text("No version history yet").size(13).color(theme::text_dim()),
            )
            .padding(16)
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
        }

        let mut list = column![].spacing(4).padding(Padding::from([8, 12]));

        for (i, commit) in self.commits.iter().enumerate() {
            let ts = commit.timestamp.format("%Y-%m-%d %H:%M").to_string();
            let is_selected = self.selected_commit == Some(i);

            let header_row = row![
                text(ts).size(12).color(theme::border_accent()),
                Space::new().width(8),
                text(commit.message.clone()).size(12).color(theme::text_primary()),
            ]
            .spacing(0);

            let mut entry = column![header_row].spacing(2);

            if is_selected {
                let snap_title = text(format!("Title: {}", commit.snapshot.title))
                    .size(11)
                    .color(theme::text_dim());
                let preview: String = commit.snapshot.content.chars().take(200).collect();
                let snap_body = text(preview).size(11).color(theme::text_dim());
                entry = entry.push(snap_title).push(snap_body);
            }

            let entry_btn = button(entry)
                .on_press(Message::SelectCommit {
                    panel_idx: index,
                    commit_idx: i,
                })
                .style(theme::skill_button_style)
                .padding(Padding::from([6, 10]))
                .width(Length::Fill);

            list = list.push(entry_btn);
        }

        scrollable(list)
            .height(Length::Fill)
            .width(Length::Fill)
            .into()
    }
}
