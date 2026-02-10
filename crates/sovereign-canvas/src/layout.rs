use std::collections::{BTreeMap, HashMap};

use chrono::{Datelike, TimeZone, Utc};
use sovereign_db::schema::{Document, Milestone, RelatedTo, RelationType, Thread};

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
    pub is_owned: bool,
    pub thread_id: String,
    pub created_at_ts: i64,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl CardLayout {
    /// Center point of the card in world coordinates.
    pub fn center(&self) -> (f32, f32) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
}

/// A horizontal lane representing a thread.
#[derive(Debug, Clone)]
pub struct LaneLayout {
    pub thread_id: String,
    pub thread_name: String,
    pub y: f32,
    pub height: f32,
}

/// A vertical date marker on the timeline.
#[derive(Debug, Clone)]
pub struct TimelineMarker {
    pub x: f32,
    pub label: String,
    pub is_milestone: bool,
}

/// A visual edge between two thread lanes, representing a branch.
#[derive(Debug, Clone)]
pub struct BranchEdge {
    pub from_thread_id: String,
    pub to_thread_id: String,
    /// X position where the branch starts (from-lane)
    pub from_x: f32,
    /// Y center of the from-lane
    pub from_y: f32,
    /// Y center of the to-lane
    pub to_y: f32,
}

/// Complete canvas layout: all cards and lanes.
#[derive(Debug, Clone)]
pub struct CanvasLayout {
    pub cards: Vec<CardLayout>,
    pub lanes: Vec<LaneLayout>,
    pub timeline_markers: Vec<TimelineMarker>,
    pub branch_edges: Vec<BranchEdge>,
}

/// Compute thread-lane layout from documents, threads, and optional relationships.
///
/// Documents are grouped by `thread_id`, placed left-to-right within each lane.
/// Threads are sorted by creation date. Documents with an unknown thread_id
/// are placed in an "Uncategorized" lane at the bottom.
/// Documents with non-zero `spatial_x`/`spatial_y` keep their DB positions.
pub fn compute_layout(docs: &[Document], threads: &[Thread]) -> CanvasLayout {
    compute_layout_full(docs, threads, &[], &[])
}

/// Like `compute_layout`, but also computes branch edges from relationships.
pub fn compute_layout_with_edges(
    docs: &[Document],
    threads: &[Thread],
    relationships: &[RelatedTo],
) -> CanvasLayout {
    compute_layout_full(docs, threads, relationships, &[])
}

/// Full layout computation with relationships and milestones.
pub fn compute_layout_full(
    docs: &[Document],
    threads: &[Thread],
    relationships: &[RelatedTo],
    milestones: &[Milestone],
) -> CanvasLayout {
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

    let mut timeline_markers = compute_timeline_markers(&cards);
    // Add milestone markers at appropriate X positions
    for ms in milestones {
        let ms_ts = ms.timestamp.timestamp();
        // Find the closest card X position for this milestone's timestamp
        let x = cards
            .iter()
            .filter(|c| c.thread_id == ms.thread_id)
            .min_by_key(|c| (c.created_at_ts - ms_ts).unsigned_abs())
            .map(|c| c.x)
            .unwrap_or(0.0);
        timeline_markers.push(TimelineMarker {
            x,
            label: ms.title.clone(),
            is_milestone: true,
        });
    }
    let branch_edges = compute_branch_edges(relationships, &cards, &lanes);
    CanvasLayout {
        cards,
        lanes,
        timeline_markers,
        branch_edges,
    }
}

/// Compute branch edges from `BranchesFrom` relationships.
/// For each BranchesFrom relationship between two documents, we draw an edge
/// between the lanes of the threads those documents belong to.
pub fn compute_branch_edges(
    relationships: &[RelatedTo],
    cards: &[CardLayout],
    lanes: &[LaneLayout],
) -> Vec<BranchEdge> {
    use sovereign_db::schema::thing_to_raw;

    let mut edges = Vec::new();
    for rel in relationships {
        if rel.relation_type != RelationType::BranchesFrom {
            continue;
        }
        let from_id = rel.out.as_ref().map(|t| thing_to_raw(t));
        let to_id = rel.in_.as_ref().map(|t| thing_to_raw(t));

        if let (Some(from_id), Some(to_id)) = (from_id, to_id) {
            let from_card = cards.iter().find(|c| c.doc_id == from_id);
            let to_card = cards.iter().find(|c| c.doc_id == to_id);

            if let (Some(fc), Some(tc)) = (from_card, to_card) {
                let from_lane = lanes.iter().find(|l| l.thread_id == fc.thread_id);
                let to_lane = lanes.iter().find(|l| l.thread_id == tc.thread_id);

                if let (Some(fl), Some(tl)) = (from_lane, to_lane) {
                    edges.push(BranchEdge {
                        from_thread_id: fc.thread_id.clone(),
                        to_thread_id: tc.thread_id.clone(),
                        from_x: fc.x + fc.w,
                        from_y: fl.y + fl.height / 2.0,
                        to_y: tl.y + tl.height / 2.0,
                    });
                }
            }
        }
    }
    edges
}

/// Compute timeline markers by grouping cards by month/year.
/// Each unique month gets a marker at the average X of its cards.
pub fn compute_timeline_markers(cards: &[CardLayout]) -> Vec<TimelineMarker> {
    if cards.is_empty() {
        return Vec::new();
    }

    // Group card x-positions by (year, month)
    let mut by_month: BTreeMap<(i32, u32), Vec<f32>> = BTreeMap::new();
    for card in cards {
        let dt = Utc.timestamp_opt(card.created_at_ts, 0).single();
        if let Some(dt) = dt {
            by_month
                .entry((dt.year(), dt.month()))
                .or_default()
                .push(card.x);
        }
    }

    let mut markers = Vec::new();
    for ((year, month), xs) in &by_month {
        let min_x = xs.iter().cloned().fold(f32::INFINITY, f32::min);
        let label = format!("{}/{}", month, year);
        markers.push(TimelineMarker {
            x: min_x - 10.0,
            label,
            is_milestone: false,
        });
    }

    markers
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
    // Sort by modified_at ascending (oldest → left, newest → right) for timeline layout
    let mut sorted: Vec<&Document> = docs.to_vec();
    sorted.sort_by_key(|d| d.modified_at);

    for (i, doc) in sorted.iter().enumerate() {
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
            is_owned: doc.is_owned,
            thread_id: doc.thread_id.clone(),
            created_at_ts: doc.created_at.timestamp(),
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
    use sovereign_db::schema::{Document, Thread};
    use surrealdb::sql::Thing;

    fn make_thread(id: &str, name: &str) -> Thread {
        Thread {
            id: Some(Thing::from(("thread", id))),
            name: name.to_string(),
            description: String::new(),
            created_at: Utc::now(),
            deleted_at: None,
        }
    }

    fn make_doc(id: &str, title: &str, thread_id: &str, is_owned: bool) -> Document {
        Document {
            id: Some(Thing::from(("document", id))),
            title: title.to_string(),
            content: r#"{"body":"","images":[]}"#.to_string(),
            thread_id: thread_id.to_string(),
            is_owned,
            created_at: Utc::now(),
            modified_at: Utc::now(),
            spatial_x: 0.0,
            spatial_y: 0.0,
            head_commit: None,
            deleted_at: None,
        }
    }

    #[test]
    fn card_center_calculation() {
        let card = CardLayout {
            doc_id: "d1".into(),
            title: "T".into(),
            is_owned: true,
            thread_id: "t1".into(),
            created_at_ts: 0,
            x: 100.0,
            y: 200.0,
            w: 200.0,
            h: 80.0,
        };
        let (cx, cy) = card.center();
        assert_eq!(cx, 200.0);
        assert_eq!(cy, 240.0);
    }

    #[test]
    fn card_layout_has_thread_id_and_timestamp() {
        let thread = make_thread("t1", "Test");
        let doc = make_doc("d1", "Doc", "thread:t1", true);
        let layout = compute_layout(&[doc], &[thread]);
        assert_eq!(layout.cards[0].thread_id, "thread:t1");
        assert!(layout.cards[0].created_at_ts > 0);
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

    #[test]
    fn timeline_markers_empty_cards() {
        let markers = compute_timeline_markers(&[]);
        assert!(markers.is_empty());
    }

    #[test]
    fn timeline_markers_groups_by_month() {
        use chrono::TimeZone;
        let jan = Utc.with_ymd_and_hms(2025, 1, 15, 0, 0, 0).unwrap();
        let feb = Utc.with_ymd_and_hms(2025, 2, 10, 0, 0, 0).unwrap();
        let feb2 = Utc.with_ymd_and_hms(2025, 2, 20, 0, 0, 0).unwrap();

        let cards = vec![
            CardLayout {
                doc_id: "d1".into(),
                title: "A".into(),
                is_owned: true,
                thread_id: "t1".into(),
                created_at_ts: jan.timestamp(),
                x: 100.0, y: 30.0, w: 200.0, h: 80.0,
            },
            CardLayout {
                doc_id: "d2".into(),
                title: "B".into(),
                is_owned: true,
                thread_id: "t1".into(),
                created_at_ts: feb.timestamp(),
                x: 320.0, y: 30.0, w: 200.0, h: 80.0,
            },
            CardLayout {
                doc_id: "d3".into(),
                title: "C".into(),
                is_owned: true,
                thread_id: "t1".into(),
                created_at_ts: feb2.timestamp(),
                x: 540.0, y: 30.0, w: 200.0, h: 80.0,
            },
        ];
        let markers = compute_timeline_markers(&cards);
        assert_eq!(markers.len(), 2); // Jan + Feb
        assert!(markers[0].label.contains("2025"));
        assert!(markers[1].label.contains("2025"));
    }

    #[test]
    fn branch_edges_empty_relationships() {
        let edges = compute_branch_edges(&[], &[], &[]);
        assert!(edges.is_empty());
    }

    #[test]
    fn branch_edges_non_branch_ignored() {
        use sovereign_db::schema::RelatedTo;
        let rel = RelatedTo {
            id: None,
            in_: Some(surrealdb::sql::Thing::from(("document", "d1"))),
            out: Some(surrealdb::sql::Thing::from(("document", "d2"))),
            relation_type: sovereign_db::schema::RelationType::References,
            strength: 1.0,
            created_at: Utc::now(),
        };
        let cards = vec![
            CardLayout {
                doc_id: "document:d1".into(),
                title: "A".into(),
                is_owned: true,
                thread_id: "thread:t1".into(),
                created_at_ts: 0,
                x: 100.0, y: 30.0, w: 200.0, h: 80.0,
            },
        ];
        let lanes = vec![LaneLayout {
            thread_id: "thread:t1".into(),
            thread_name: "T1".into(),
            y: 0.0,
            height: 110.0,
        }];
        let edges = compute_branch_edges(&[rel], &cards, &lanes);
        assert!(edges.is_empty());
    }

    #[test]
    fn milestone_markers_included_in_layout() {
        let thread = make_thread("t1", "Research");
        let doc = make_doc("d1", "Doc", "thread:t1", true);
        let ms = sovereign_db::schema::Milestone {
            id: None,
            title: "Alpha Release".into(),
            timestamp: Utc::now(),
            thread_id: "thread:t1".into(),
            description: "First release".into(),
        };
        let layout = compute_layout_full(&[doc], &[thread], &[], &[ms]);
        let milestone_markers: Vec<_> = layout
            .timeline_markers
            .iter()
            .filter(|m| m.is_milestone)
            .collect();
        assert_eq!(milestone_markers.len(), 1);
        assert_eq!(milestone_markers[0].label, "Alpha Release");
    }

    #[test]
    fn timeline_markers_same_month_produces_one_marker() {
        let cards = vec![
            CardLayout {
                doc_id: "d1".into(),
                title: "A".into(),
                is_owned: true,
                thread_id: "t1".into(),
                created_at_ts: 1700000000, // Nov 2023
                x: 100.0, y: 30.0, w: 200.0, h: 80.0,
            },
            CardLayout {
                doc_id: "d2".into(),
                title: "B".into(),
                is_owned: true,
                thread_id: "t1".into(),
                created_at_ts: 1700100000, // also Nov 2023
                x: 320.0, y: 30.0, w: 200.0, h: 80.0,
            },
        ];
        let markers = compute_timeline_markers(&cards);
        assert_eq!(markers.len(), 1);
    }
}
