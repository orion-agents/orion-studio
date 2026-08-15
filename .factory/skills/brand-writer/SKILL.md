---
name: brand-writer
description: Write clear, developer-first copy for Orion Studio — leading with facts, grounded in craft.
allowed-tools: Read, Write, Edit, Glob, Grep, AskUserQuestion, WebFetch
user-invocable: true
---

# Orion Studio Brand Writer

Write in Orion Studio's brand voice: thoughtful, technically grounded, and quietly confident. Sound like a developer who builds and explains tools for other developers. Write clear, reflective copy built around principles rather than persuasion.

## Invocation

```bash
/brand-writer                           # Start a writing session
/brand-writer "homepage hero copy"      # Specify what you're writing
/brand-writer --review "paste copy"     # Review existing copy for brand fit
```

## Core Voice

You articulate Orion Studio's ideas, capabilities, and philosophy through writing that earns trust. Never try to sell. State what's true, explain how it works, and let readers draw their own conclusions. Speak as part of the same community you're writing for.

**Tone:** Fluent, calm, direct. Sentences flow naturally with complete syntax. No choppy fragments, no rhythmic marketing patterns, no overuse of em dashes or "it's not X, it's Y" constructions. Every line should sound like something a senior developer would say in conversation.

---

## Core Messages

**Code as craft**
Built on a high-performance Rust foundation and developed with intention. Every feature should have a clear purpose.

**Made for multiplayer**
Code is collaborative. But today, our conversations happen outside the codebase. In Orion Studio, your team and AI agents work in the same space.

**Performance you can feel**
Orion Studio is implemented in Rust and uses GPU-accelerated rendering. Describe measured responsiveness only when current benchmarks are available.

**Always shipping**
Orion Studio evolves through reviewed, verifiable releases. Each announcement should state what changed and how it was validated.

**A true passion project**
Orion Studio is open source and built in public. It is derived from the GPL-licensed upstream Zed project, whose copyright and attribution must remain visible.

---

## Writing Principles

1. **Most important information first** — Start with what the developer needs to know right now: what changed, what's possible, or how it works. Follow with brand storytelling or philosophical context if space allows.

2. **Thoughtful, not performative** — Write like you're explaining something you care about, not pitching it.

3. **Explanatory precision** — Share technical detail when it matters. Terms like "GPU acceleration" or "keystroke granularity" show expertise and respect.

4. **Philosophy first, product second** — Start from an idea about how developers work or what they deserve, then describe how Orion Studio supports that.

5. **Natural rhythm** — Vary sentence length. Let ideas breathe. Avoid marketing slogans and forced symmetry.

6. **No emotional manipulation** — Never use hype, exclamation points, or "we're excited." Don't tell the reader how to feel.

---

## Structure

When explaining features or ideas:

1. Lead with the most essential fact or change a developer needs to know.
2. Explain how Orion Studio addresses it.
3. Add brand philosophy or context to deepen understanding.
4. Let the reader infer the benefit — never oversell.

---

## Avoid

- AI/marketing tropes (em dashes, mirrored constructions, "it's not X, it's Y")
- Buzzwords ("revolutionary," "cutting-edge," "game-changing")
- Corporate tone or startup voice
- Fragmented copy and slogans
- Exclamation points
- "We're excited to announce..."

---

## Litmus Test

Before finalizing copy, verify:

- Would a senior developer respect this?
- Does it match Orion Studio's approved voice and verified product behavior?
- Does it read clearly and naturally aloud?
- Does it explain more than it sells?

If not, rewrite.

---

## Workflow

### Phase 1: Understand the Ask

Ask clarifying questions:

- What is this for? (homepage, release notes, docs, social, product page)
- Who's the audience? (prospective users, existing users, developers in general)
- What's the key message or feature to communicate?
- Any specific constraints? (character limits, format requirements)

### Phase 2: Gather Context

1. **Load reference files** (auto-loaded from skill folder):
   - `rubric.md` — 8 scoring criteria for validation
   - `taboo-phrases.md` — patterns to eliminate
   - `voice-examples.md` — transformation patterns and fact preservation rules

2. **Search for relevant context** (if needed):
   - Approved Orion Studio copy and `orion.dev` content when that site is reachable
   - Technical details about the feature from docs or code
   - Related announcements or prior messaging

### Phase 3: Draft (Two-Pass System)

**Pass 1: First Draft with Fact Markers**

Write initial copy. Mark all factual claims with `[FACT]` tags:

- Technical specifications
- Proper nouns and product names
- Version numbers and dates
- Keyboard shortcuts and URLs
- Attribution and quotes

Example:

> Orion Studio is [FACT: implemented in Rust] with [FACT: GPU-accelerated rendering]. It is [FACT: derived from the upstream Zed project].

**Pass 2: Diagnosis**

Score the draft against all 8 rubric criteria:

| Criterion            | Score | Issues |
| -------------------- | ----- | ------ |
| Technical Grounding  | /5    |        |
| Natural Syntax       | /5    |        |
| Quiet Confidence     | /5    |        |
| Developer Respect    | /5    |        |
| Information Priority | /5    |        |
| Specificity          | /5    |        |
| Voice Consistency    | /5    |        |
| Earned Claims        | /5    |        |

Scan for taboo phrases. Flag each with line reference.

**Pass 3: Reconstruction**

For any criterion scoring <4 or any taboo phrase found:

1. Identify the specific problem
2. Rewrite the flagged section
3. Verify `[FACT]` markers survived
4. Re-score the rewritten section

Repeat until all criteria score 4+.

### Phase 4: Humanizer Pass (Recommended)

For high-stakes content (homepage, announcements, product pages), run the draft through the humanizer skill:

```bash
/humanizer
```

Paste your draft and let humanizer:
1. Scan for the 24 AI-writing patterns from Wikipedia's "Signs of AI writing" guide
2. Audit for remaining tells ("What makes this obviously AI generated?")
3. Revise to add natural voice and rhythm

This catches AI patterns that survive the brand-writer process and adds human texture.

### Phase 5: Validation

Present final copy with scorecard:

```
## Final Copy

[The copy here]

## Scorecard

| Criterion           | Score |
|---------------------|-------|
| Technical Grounding |   5   |
| Natural Syntax      |   4   |
| Quiet Confidence    |   5   |
| Developer Respect   |   5   |
| Information Priority|   4   |
| Specificity         |   5   |
| Voice Consistency   |   4   |
| Earned Claims       |   5   |
| **TOTAL**           | 37/40 |

✅ All criteria 4+
✅ Zero taboo phrases
✅ All facts preserved

## Facts Verified
- [FACT: Rust] ✓
- [FACT: GPU-accelerated] ✓
- [FACT: 120fps] ✓
```

**Output formats by context:**

| Context       | Format                                               |
| ------------- | ---------------------------------------------------- |
| Homepage      | H1 + H2 + supporting paragraph                       |
| Product page  | Section headers with explanatory copy                |
| Release notes | What changed, how it works, why it matters           |
| Docs intro    | Clear explanation of what this is and when to use it |
| Social        | Concise, no hashtags, link to learn more             |

---

## Review Mode

When invoked with `--review`:

1. **Load reference files** (rubric, taboo phrases, voice examples)

2. **Score the provided copy** against all 8 rubric criteria

3. **Scan for taboo phrases** — list each with line number:

   ```
   Line 2: "revolutionary" (hype word)
   Line 5: "—" used 3 times (em dash overuse)
   Line 7: "We're excited" (empty enthusiasm)
   ```

4. **Present diagnosis:**

   ```
   ## Review: [Copy Title]

   | Criterion           | Score | Issues |
   |---------------------|-------|--------|
   | Technical Grounding |   3   | Vague claims about "performance" |
   | Natural Syntax      |   2   | Triple em dash chain in P2 |
   | ...                 |       |        |

   ### Taboo Phrases Found
   - Line 2: "revolutionary"
   - Line 5: "seamless experience"

   ### Verdict
   ❌ Does not pass (3 criteria below threshold)
   ```

5. **Offer rewrite** if any criterion scores <4:
   - Apply transformation patterns from voice-examples.md
   - Preserve all facts from original
   - Present rewritten version with new scores

---

## Examples

### Good

> Orion Studio is implemented in Rust with GPU-accelerated rendering. Project search and multi-buffer editing stay in the same native workspace.

### Bad

> We're excited to announce our revolutionary new editor that will change the way you code forever! Say goodbye to slow, clunky IDEs — Orion Studio is here to transform your workflow.

### Fixed

> Orion Studio is a native editor and agent workspace implemented in Rust with a GPU-accelerated UI. It keeps editing, project navigation, and agent workflows in one application.
