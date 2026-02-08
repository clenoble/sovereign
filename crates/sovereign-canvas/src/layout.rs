use std::collections::HashMap;

use sovereign_db::schema::{Document, Thread};

// Layout constants
pub const CARD_WIDTH: f32 = 200.0;
pub const CARD_HEIGHT: f32 = 80.0;
pub const CARD_SPACING_H: f32 = 20.0;
pub const LANE_SPACING_V: f32 = 40.0;
pub const LANE_HEADER_WIDTH: f32 = 160.0;
pub const LANE_PADDING_TOP: f32 = 30.0;

/// A positioned card on the canvas.
#[derive(Debug, Clone)]
pub struct CardLayout {
    pub doc_id: String,
    pub title: String,
    pub doc_type: String,
    pub is_owned: bool,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// A horizontal lane representing a thread.
#[derive(Debug, Clone)]
pub struct LaneLayout {
    pub thread_id: String,
    pub thread_name: String,
    pub y: f32,
    pub height: f32,
}

/// Complete canvas layout: all cards and lanes.
#[derive(Debug, Clone)]
pub struct CanvasLayout {
    pub cards: Vec<CardLayout>,
    pub lanes: Vec<LaneLayout>,
}

/// Compute thread-lane layout from documents and threads.
///
/// Documents are grouped by `thread_id`, placed left-to-right within each lane.
/// Threads are sorted by creation date. Documents with an unknown thread_id
/// are placed in an "Uncategorized" lane at the bottom.
/// Documents with non-zero `spatial_x`/`spatial_y` keep their DB positions.
pub fn compute_layout(docs: &[Document], threads: &[Thread]) -> CanvasLayout {
    // Group documents by thread_id
    let mut by_thread: HashMap<String, Vec<&Document>> = HashMap::new();
    for doc in docs {
        by_thread
            .entry(doc.thread_id.clone())
            .or_default()
            .push(doc);
    }

    // Sort threads by creation date
    let mut sorted_threads: Vec<&Thread> = threads.iter().collect();
    sorted_threads.sort_by_key(|t| t.created_at);

    let known_thread_ids: Vec<String> = sorted_threads
        .iter()
        .filter_map(|t| t.id_string())
        .collect();

    let mut cards = Vec::new();
    let mut lanes = Vec::new();
    let mut current_y: f32 = 0.0;

    // Lay out known threads
    for thread in &sorted_threads {
        let tid = thread.id_string().unwrap_or_default();
        let thread_docs = by_thread.remove(&tid).unwrap_or_default();

        let lane_height = compute_lane_height(&thread_docs);
        lanes.push(LaneLayout {
            thread_id: tid.clone(),
            thread_name: thread.name.clone(),
            y: current_y,
            height: lane_height,
        });

        place_cards_in_lane(&thread_docs, &tid, current_y, &mut cards);
        current_y += lane_height + LANE_SPACING_V;
    }

    // Collect uncategorized docs (thread_id not matching any known thread)
    let mut uncategorized: Vec<&Document> = Vec::new();
    for (tid, docs_in_thread) in by_thread.drain() {
        if !known_thread_ids.contains(&tid) {
            uncategorized.extend(docs_in_thread);
        }
    }

    if !uncategorized.is_empty() {
        let lane_height = compute_lane_height(&uncategorized);
        lanes.push(LaneLayout {
            thread_id: String::new(),
            thread_name: "Uncategorized".to_string(),
            y: current_y,
            height: lane_height,
        });
        place_cards_in_lane(&uncategorized, "", current_y, &mut cards);
    }

    CanvasLayout { cards, lanes }
}

fn compute_lane_height(docs: &[&Document]) -> f32 {
    if docs.is_empty() {
        LANE_PADDING_TOP + CARD_HEIGHT
    } else {
        LANE_PADDING_TOP + CARD_HEIGHT
    }
}

fn place_cards_in_lane(
    docs: &[&Document],
    _thread_id: &str,
    lane_y: f32,
    cards: &mut Vec<CardLayout>,
) {
    for (i, doc) in docs.iter().enumerate() {
        let has_spatial = doc.spatial_x != 0.0 || doc.spatial_y != 0.0;
        let (x, y) = if has_spatial {
            (doc.spatial_x, doc.spatial_y)
        } else {
            (
                LANE_HEADER_WIDTH + i as f32 * (CARD_WIDTH + CARD_SPACING_H),
                lane_y + LANE_PADDING_TOP,
            )
        };

        cards.push(CardLayout {
            doc_id: doc.id_string().unwrap_or_default(),
            title: doc.title.clone(),
            doc_type: doc.doc_type.to_string(),
            is_owned: doc.is_owned,
            x,
            y,
            w: CARD_WIDTH,
            h: CARD_HEIGHT,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use sovereign_db::schema::{Document, DocumentType, Thread};
    use surrealdb::sql::Thing;

    fn make_thread(id: &str, name: &str) -> Thread {
        Thread {
            id: Some(Thing::from(("thread", id))),
            name: name.to_string(),
            description: String::new(),
            created_at: Utc::now(),
        }
    }

    fn make_doc(id: &str, title: &str, thread_id: &str, is_owned: bool) -> Document {
        Document {
            id: Some(Thing::from(("document", id))),
            title: title.to_string(),
            doc_type: DocumentType::Markdown,
            content: String::new(),
            thread_id: thread_id.to_string(),
            is_owned,
            created_at: Utc::now(),
            modified_at: Utc::now(),
            spatial_x: 0.0,
            spatial_y: 0.0,
        }
    }

    #[test]
    fn empty_input_produces_empty_layout() {
        let layout = compute_layout(&[], &[]);
        assert!(layout.cards.is_empty());
        assert!(layout.lanes.is_empty());
    }

    #[test]
    fn single_thread_with_docs_places_left_to_right() {
        let thread = make_thread("abc", "Research");
        let docs = vec![
            make_doc("d1", "Doc 1", "thread:abc", true),
            make_doc("d2", "Doc 2", "thread:abc", true),
            make_doc("d3", "Doc 3", "thread:abc", false),
        ];
        let layout = compute_layout(&docs, &[thread]);

        assert_eq!(layout.lanes.len(), 1);
        assert_eq!(layout.lanes[0].thread_name, "Research");
        assert_eq!(layout.cards.len(), 3);

        // Cards should be left-to-right with increasing x
        assert!(layout.cards[0].x < layout.cards[1].x);
        assert!(layout.cards[1].x < layout.cards[2].x);

        // All at same y (lane_y + padding)
        assert_eq!(layout.cards[0].y, layout.cards[1].y);
    }

    #[test]
    fn two_threads_create_two_lanes() {
        let t1 = make_thread("t1", "Research");
        let t2 = make_thread("t2", "Development");
        let docs = vec![
            make_doc("d1", "Doc 1", "thread:t1", true),
            make_doc("d2", "Doc 2", "thread:t2", true),
        ];
        let layout = compute_layout(&docs, &[t1, t2]);

        assert_eq!(layout.lanes.len(), 2);
        assert!(layout.lanes[0].y < layout.lanes[1].y);
        assert_eq!(layout.cards.len(), 2);
    }

    #[test]
    fn docs_with_spatial_position_keep_db_coords() {
        let thread = make_thread("t1", "Test");
        let mut doc = make_doc("d1", "Placed", "thread:t1", true);
        doc.spatial_x = 500.0;
        doc.spatial_y = 300.0;
        let layout = compute_layout(&[doc], &[thread]);

        assert_eq!(layout.cards[0].x, 500.0);
        assert_eq!(layout.cards[0].y, 300.0);
    }

    #[test]
    fn unknown_thread_id_goes_to_uncategorized() {
        let thread = make_thread("t1", "Known");
        let docs = vec![
            make_doc("d1", "Known Doc", "thread:t1", true),
            make_doc("d2", "Orphan", "thread:nonexistent", true),
        ];
        let layout = compute_layout(&docs, &[thread]);

        assert_eq!(layout.lanes.len(), 2);
        assert_eq!(layout.lanes[1].thread_name, "Uncategorized");
        assert_eq!(layout.cards.len(), 2);
    }
}
