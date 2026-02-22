# Sovereign OS â€” Consolidated Design Decisions

## Canvas Model
- **Metaphor**: Infinite canvas viewed through a viewport (like a microfiche reader / Miro)
- **Horizontal axis**: Timeline (left = past, right = future)
- **Vertical axis**: Extent of all intent threads and interactions
- **Homebase**: Present moment â€” minimal, clean canvas (or background image) with subtle hint of recent activity on the left

## Navigation
- **Zoom in/out**: Click + scroll for detail vs. overview
- **Scroll up/down**: Vertical move across threads
- **Drag left/right**: Move through timeline
- **Timeline style**: Hybrid â€” continuous scroll with milestone markers at meaningful events
- **Minimap**: On demand (toggle or hover-reveal)
- **Timebox**: Instant jump to a year or date
- **Search field**: Always accessible
- **Filters**: By type â€” documents, skills, contacts

## Visual Density (Zoom Levels)
- **Sparse**: Recognizable miniature card previews
- **Dense**: Abstract heat-map blobs
- **Progressive**: Cards transition to blobs as density increases

## Homebase (Present View)
- **Minimal**: Just taskbar hints, clean canvas
- **Left edge**: Subtle hint of last actions (fading into past)

## Taskbar
- **Position**: Anchored to bottom edge, always visible
- **Default content**: Last intent thread, last document, last contact interaction
- **Customization**: User can pin specific threads/documents for quick access

## Document Windows
- **Behavior**: Free-floating windows (overlap allowed)
- **Opens as**: Layer on top of the canvas, resizable

## Sovereignty Halo â€” Shape-Based Trust
- **Owned content**: Rectangles on user-chosen background color
- **External content**: Parallelograms (slanted rectangles) on contrasting color
- **Rationale**: Leverages preattentive shape recognition â€” works for color-blind users
- **Ingest animation**: External parallelogram morphs into owned rectangle when user adopts content

## Intent Threads
- **Creation**: Hybrid â€” AI auto-creates, user can merge/split
- **Structure**: Flat (no hierarchy) with soft branching â€” visual links between related threads
- **Branching**: Dotted/curved lines connecting related threads, no parent-child nesting

## AI Orchestrator
- **Visual presence**: Floating bubble (minimal, expandable on interaction)
- **Input modes**: Voice, stylus (OCR/drawing), natural language typing, touch
- **Proactive suggestions**: Non-intrusive hints for relevant Skills

## Microphone
- **Model**: User chooses in settings â€” push-to-talk OR wake-word
- **Explicit button**: Available for users who disable always-listening