use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Element, Length, Padding};

use crate::app::Message;
use crate::theme;

/// A single GGUF model entry.
#[derive(Debug, Clone)]
pub struct ModelEntry {
    pub filename: String,
    pub size_mb: u64,
}

/// Role a model can be assigned to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelRole {
    Router,
    Reasoning,
}

/// Panel for viewing and managing AI models.
pub struct ModelPanel {
    pub visible: bool,
    pub model_dir: String,
    pub models: Vec<ModelEntry>,
    pub router_model: String,
    pub reasoning_model: String,
}

impl ModelPanel {
    pub fn new(model_dir: String, router_model: String, reasoning_model: String) -> Self {
        let mut panel = Self {
            visible: false,
            model_dir,
            models: Vec::new(),
            router_model,
            reasoning_model,
        };
        panel.refresh();
        panel
    }

    /// Scan the model directory for .gguf files.
    pub fn refresh(&mut self) {
        self.models.clear();
        let dir = std::path::Path::new(&self.model_dir);
        if dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("gguf") {
                        let filename = path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        let size_mb = std::fs::metadata(&path)
                            .map(|m| m.len() / (1024 * 1024))
                            .unwrap_or(0);
                        self.models.push(ModelEntry { filename, size_mb });
                    }
                }
            }
        }
        self.models.sort_by(|a, b| a.filename.cmp(&b.filename));
    }

    /// Delete a model file by index.
    pub fn delete_model(&mut self, idx: usize) {
        if let Some(entry) = self.models.get(idx) {
            let path = std::path::Path::new(&self.model_dir).join(&entry.filename);
            if let Err(e) = std::fs::remove_file(&path) {
                tracing::error!("Failed to delete model {}: {e}", entry.filename);
            } else {
                tracing::info!("Deleted model: {}", entry.filename);
            }
        }
        self.refresh();
    }

    /// Assign a model to a role.
    pub fn assign_role(&mut self, idx: usize, role: ModelRole) {
        if let Some(entry) = self.models.get(idx) {
            match role {
                ModelRole::Router => self.router_model = entry.filename.clone(),
                ModelRole::Reasoning => self.reasoning_model = entry.filename.clone(),
            }
            tracing::info!("Assigned {} as {:?}", entry.filename, role);
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let mut col = column![].spacing(8).padding(16).width(520);

        // Header
        col = col.push(
            row![
                text("Model Management").size(18).color(theme::text_primary()),
                Space::new().width(Length::Fill),
                button(text("Refresh").size(12))
                    .on_press(Message::ModelRefresh)
                    .style(theme::skill_button_style)
                    .padding(Padding::from([6, 12])),
                button(text("Close").size(12))
                    .on_press(Message::ModelPanelToggled)
                    .style(theme::reject_button_style)
                    .padding(Padding::from([6, 12])),
            ]
            .spacing(8),
        );

        // Current assignments
        col = col.push(
            column![
                text(format!("Router: {}", if self.router_model.is_empty() { "(none)" } else { &self.router_model }))
                    .size(12)
                    .color(theme::border_accent()),
                text(format!("Reasoning: {}", if self.reasoning_model.is_empty() { "(none)" } else { &self.reasoning_model }))
                    .size(12)
                    .color(theme::border_accent()),
            ]
            .spacing(2),
        );

        col = col.push(Space::new().height(8));

        // Model directory info
        col = col.push(
            text(format!("Directory: {}", self.model_dir))
                .size(11)
                .color(theme::text_dim()),
        );

        // Model list
        if self.models.is_empty() {
            col = col.push(
                text("No .gguf models found").size(13).color(theme::text_dim()),
            );
        } else {
            let mut list = column![].spacing(4);
            for (i, entry) in self.models.iter().enumerate() {
                let is_router = entry.filename == self.router_model;
                let is_reasoning = entry.filename == self.reasoning_model;

                let role_label = if is_router && is_reasoning {
                    " [Router + Reasoning]"
                } else if is_router {
                    " [Router]"
                } else if is_reasoning {
                    " [Reasoning]"
                } else {
                    ""
                };

                let name_color = if is_router || is_reasoning {
                    theme::border_accent()
                } else {
                    theme::text_primary()
                };

                let size_label = if entry.size_mb >= 1024 {
                    format!("{:.1} GB", entry.size_mb as f64 / 1024.0)
                } else {
                    format!("{} MB", entry.size_mb)
                };

                let mut entry_row = row![
                    text(format!("{}{}", entry.filename, role_label))
                        .size(12)
                        .color(name_color),
                    Space::new().width(Length::Fill),
                    text(size_label).size(11).color(theme::text_dim()),
                ]
                .spacing(8);

                // Action buttons
                if !is_router {
                    entry_row = entry_row.push(
                        button(text("Router").size(10))
                            .on_press(Message::ModelAssignRole {
                                model_idx: i,
                                role: ModelRole::Router,
                            })
                            .style(theme::skill_button_style)
                            .padding(Padding::from([3, 8])),
                    );
                }
                if !is_reasoning {
                    entry_row = entry_row.push(
                        button(text("Reason").size(10))
                            .on_press(Message::ModelAssignRole {
                                model_idx: i,
                                role: ModelRole::Reasoning,
                            })
                            .style(theme::skill_button_style)
                            .padding(Padding::from([3, 8])),
                    );
                }
                if !is_router && !is_reasoning {
                    entry_row = entry_row.push(
                        button(text("Del").size(10))
                            .on_press(Message::ModelDelete(i))
                            .style(theme::reject_button_style)
                            .padding(Padding::from([3, 8])),
                    );
                }

                list = list.push(
                    container(entry_row.padding(Padding::from([6, 10])))
                        .style(theme::document_panel_style),
                );
            }
            col = col.push(scrollable(list).height(Length::Fixed(350.0)));
        }

        container(col).style(theme::chat_panel_style).into()
    }
}
