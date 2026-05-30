# Screenshot list

30 shots across 15 pages. Shoot on the actual app — no mocks.

---

## Home (`/`)

**1. Hero shot**
Full canvas at zoom ~0.7, 2–3 thread lanes with lane labels, several document cards (mix of full cards and title-only), AI bubble in idle state, minimap visible in corner. Ideally some cards showing "(owned)" provenance labels. Landscape, wide crop.

**2. AI bubble close-up**
Bubble in active/thinking state (animated ring), suggestion badge showing at least one pending suggestion.

---

## AI Orchestrator (`/ai-orchestrator`)

**3. Chat panel — tool call in flight**
Multi-turn exchange where the AI has proposed a write action. Approve / Reject buttons visible, action name and specifics shown (e.g. "Create document: …").

**4. Chat panel — response with provenance**
Completed AI response showing an "(owned)" or "(external)" provenance marker on a cited result.

---

## Prompt Injection (`/prompt-injection`)

**5. Injection warning**
In-chat warning shown when an injection attempt is detected. Capture the severity indicator and the "Continue reading anyway?" option.

---

## Action Gravity (`/action-gravity`)

**6. Destructive confirm dialog**
Level 4/5 modal for a delete or purge action, showing the 30-day undo window note.

**7. Chat approve/reject buttons**
Level 2/3 action proposal in the chat panel with the conversational confirmation UI, before the user has responded.

---

## Encryption (`/encryption`)

**8. Onboarding key setup step**
The onboarding wizard screen where the master key / passphrase is created. Shows key generation happens locally.

**9. Provenance labels on canvas cards**
A canvas card showing the "(owned)" green label next to one showing the "(external)" amber label — side by side if possible.

---

## Social Recovery (`/social-recovery`)

**10. Guardian setup screen**
The onboarding wizard step for adding guardian contacts and setting the threshold (e.g. "3 of 5"). Guardian list and split-key concept visible.

---

## Spatial Canvas (`/spatial-canvas`)

**11. Normal zoom**
Canvas at zoom ~0.7: 2–3 thread lanes, lane labels, "Now" line, mix of full cards and title-only cards.

**12. Extreme zoom-out / heatmap**
Canvas at zoom < 0.15: density heatmap mode, colour gradients replacing individual cards.

**13. Minimap close-up**
Minimap in corner showing the viewport indicator and the Now line.

**14. Version history panel**
A document open with the version history / commit list panel visible alongside it.

---

## Communications (`/communications`)

**15. Inbox panel**
Unified inbox organised by person, showing threads from different channels (email icon, Signal icon) grouped under the same contact name.

---

## Content + Skills (`/content-skills`)

**16. Canvas with relationship curves**
Two or more document cards connected by visible relationship edges on the canvas. At least one labelled edge type visible (e.g. "References", "Supports").

---

## Skill System (`/skills`)

**17. Skills sidebar open**
Skills panel / sidebar attached to a document, list of available actions visible (word count, find & replace, export…). One action highlighted or in progress.

**18. Skill output inline**
Result of a skill run displayed in the panel — e.g. word count stats or a readability score result.

---

## PII Dashboard (`/pii-dashboard`)

**19. Overview — three-column layout**
Panel open with 2–3 entities in the left column (entity shapes visible), Inventory tab active in the centre showing several PII rows with kind labels (Email, Phone, IBAN…) and masked values.

**20. Vault tab with a revealed record**
Vault tab active for one entity. One record in revealed state (plaintext visible), others hidden. Reveal / copy / redact row-action buttons visible.

**21. Cookies tab**
Cookies tab active for an entity: domain, cookie name, and row buttons (reveal / copy / delete) visible.

**22. Unreviewed badge + review state**
Panel header showing the "X unreviewed" badge, at least one record in Unreviewed state alongside a Confirmed one for contrast.

---

## Browser & Reliability (`/browser-reliability`)

**23. Browser panel with reliability badge**
An article open in the embedded browser with the reliability score visible — score number + classification label (e.g. "Factual — 4/5").

**24. Save to workspace prompt**
The "Import to workspace" action after a page is loaded, with provenance metadata (URL, date, reliability score) visible in the prompt.

---

## Memory Consolidation (`/memory-consolidation`)

**25. Suggestion panel**
Suggestion panel open with 2–3 pending AI-suggested links, each showing: two document titles, relationship type (e.g. "Supports"), strength score, accept / dismiss buttons.

---

## Mobile (`/mobile`)

**M1. Canvas — normal zoom, portrait**
Main canvas on a phone: Now marker near top, 3–4 document cards below it, lane name in sticky header with dot indicators, chat sheet in peek state at bottom, taskbar. Primary establishing shot.

**M2. Density heatmap — deep zoom**
Canvas at deep zoom: all lanes as colour-coded strips, day-bucket columns, Now marker still visible. Shows the LOD system.

**M3. Chat sheet — partial or full detent**
Bottom sheet expanded to partial or full: multi-turn AI exchange with markdown, approve/reject buttons for a pending action, voice-input button in composer. Canvas dimmed behind sheet.

**M4. FAB long-press fan menu**
Five-option fan menu open above the FAB: New lane, New doc, New message, Capture, Secret.

**M5. Lane switcher sheet**
Lane-switcher modal listing thread names with doc counts, active lane marked.

**M6. Full-screen doc reader**
Document open full-screen in edit or preview mode: swipe-down handle at top, title field + markdown body + auto-save indicator visible.
