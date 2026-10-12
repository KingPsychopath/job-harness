"use strict";
const $ = (id) => document.getElementById(id);
const writable =
  document.querySelector('meta[name="job-harness-mode"]').content ===
  "local_writes";
const csrf = document.querySelector('meta[name="job-harness-csrf"]').content;
const state = {
  focus: "searching",
  view: "pipeline",
  kanban: false,
  offset: 0,
  query: "",
  stage: "",
  request: 0,
  notesOffset: 0,
  attentionOffset: 0,
  archivedNotes: false,
};
const focuses = {
  searching: { view: "attention", hint: "Bring applications and unanswered requests forward.", task: "Review applications, unanswered requests and the next interview. Preserve unknown outcomes before retrying." },
  building: { view: "knowledge", hint: "Capture useful work, project context and learning while it is fresh.", task: "Paste a rough work note. Keep the original, link an existing project when known, and review sourced interpretations separately." },
  exploring: { view: "knowledge", hint: "Review preferences and evidence for a possible move.", task: "Retrieve current preferences and project stories in a bounded context pack. Let your agent discuss evidence gaps and prepare selected documents." },
  starting: { view: "knowledge", hint: "Bring new responsibilities, milestones and learning together.", task: "Capture supplied milestones and learning, then request a period review with known dates and explicit gaps." },
};
const titles = {
  pipeline: [
    "Your pipeline.",
    "One durable history. Every next step in its place.",
  ],
  attention: [
    "What needs you.",
    "Recorded actions, interviews and unresolved attempts.",
  ],
  knowledge: [
    "Context that lasts.",
    "Keep originals. Review proposals. Accept only what you can support.",
  ],
  connections: [
    "Your own connections.",
    "Choose your agent, model and authenticated host.",
  ],
};
function node(tag, cls, text) {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text !== undefined) e.textContent = String(text);
  return e;
}
function button(text, action, cls = "refresh") {
  const e = node("button", cls, text);
  e.type = "button";
  e.addEventListener("click", action);
  return e;
}
function field(label, type = "text", value = "") {
  const wrap = node("label", "field");
  wrap.append(node("span", "", label));
  const input = node(type === "textarea" ? "textarea" : "input");
  if (type !== "textarea") input.type = type;
  input.value = value;
  wrap.append(input);
  return { wrap, input };
}
function label(value) {
  return String(value || "saved")
    .replaceAll("_", " ")
    .replaceAll("-", " ");
}
function safeLink(url, text) {
  try {
    const u = new URL(url);
    if (
      ["https:", "http:"].includes(u.protocol) &&
      !u.username &&
      !u.password
    ) {
      const a = node("a", "external", text);
      a.href = u.href;
      a.target = "_blank";
      a.rel = "noreferrer noopener";
      return a;
    }
  } catch {}
  return node("span", "", text);
}
async function request(path, data) {
  const response = await fetch(path, {
    method: data === undefined ? "GET" : "POST",
    credentials: "same-origin",
    cache: "no-store",
    headers:
      data === undefined
        ? {}
        : { "content-type": "application/json", "x-job-harness-csrf": csrf },
    body: data === undefined ? undefined : JSON.stringify(data),
  });
  if (!response.ok)
    throw new Error(
      response.status === 403 || response.status === 401
        ? "This local review session needs a reload."
        : `Local request rejected (${response.status}).`,
    );
  const result = await response.json();
  if (!result.ok)
    throw new Error(
      `${result.error?.message || "Workspace operation failed."} ${result.error?.recovery || ""}`,
    );
  return result.data;
}
const read = (path) => request(path);
const post = (path, data) => request(path, data);
function message(text) {
  $("status").textContent = text;
}
function empty(text) {
  return node("div", "empty", text);
}
function section(title, sub) {
  const e = node("div", "section-top");
  const a = node("div");
  a.append(node("h2", "", title), node("p", "", sub));
  e.append(a, node("span", "pill", writable ? "REVIEW ENABLED" : "READ ONLY"));
  return e;
}
function pre(text) {
  return node("pre", "source-text", text || "No text recorded.");
}
function modal(title) {
  $("detail-title").textContent = title;
  $("detail-body").replaceChildren();
  if (!$("detail").open) $("detail").showModal();
  return $("detail-body");
}
async function source(path) {
  const body = modal(path);
  body.append(empty("Reading source…"));
  try {
    const data = await read("/api/source?" + new URLSearchParams({ path }));
    body.replaceChildren(pre(data.text));
    if (data.sha256)
      body.prepend(node("p", "metadata", `Source hash: ${data.sha256}`));
  } catch (e) {
    body.replaceChildren(empty(e.message));
  }
}
function citation(c) {
  return button(
    `${c.path} · lines ${c.start_line}–${c.end_line}`,
    () => source(c.path),
    "citation",
  );
}
function recordCard(row) {
  const e = node("article", "record");
  const company = row.company || row.title || "Application";
  e.append(node("div", "company-icon", company.slice(0, 2).toUpperCase()));
  const detail = node("div");
  detail.append(
    button(company, () => companyDetail(row), "record-link"),
    node("p", "", row.title || row.record_path),
  );
  if (row.next_action) detail.append(node("small", "", row.next_action));
  e.append(
    detail,
    node(
      "span",
      "stage " + String(row.stage || "saved").replace(/[^a-z-]/g, ""),
      label(row.stage),
    ),
  );
  return e;
}
async function companyDetail(row) {
  const body = modal(row.company || row.title);
  try {
    const data = await read(
      "/api/source?" + new URLSearchParams({ path: row.record_path }),
    );
    body.append(node("p", "metadata", row.record_path));
    if (row.vacancy_url)
      body.append(safeLink(row.vacancy_url, "Open the original vacancy ↗"));
    const related = await read(
      "/api/pipeline?" + new URLSearchParams({ q: row.company || row.title }),
    );
    const links = node("div", "related");
    for (const r of related.matches || [])
      links.append(button(r.title, () => companyDetail(r), "citation"));
    body.append(node("h3", "", "Linked applications"), links);
    const context = await read(
      "/api/company?" +
        new URLSearchParams({
          company: row.company || row.title || "",
          record: row.record_path,
        }),
    );
    body.append(node("h3", "", "Sourced company context"));
    for (const passage of context.sources || []) {
      const card = node("article", "attention-card");
      const provenance = passage.provenance || {};
      card.append(
        citation(passage),
        node(
          "p",
          "metadata",
          `${label(provenance.source_kind || "source")} · occurred ${provenance.occurred_at || provenance.occurred_period || "unknown"} · recorded ${provenance.recorded_at || "unknown"}`,
        ),
        pre(passage.excerpt),
      );
      body.append(card);
    }
    if (!(context.sources || []).length)
      body.append(empty("No linked company evidence has been recorded yet."));
    for (const gap of context.gaps || [])
      body.append(
        node("p", "metadata", `${gap.path || "Context"}: ${gap.message}`),
      );
    body.append(node("h3", "", "Canonical record"), pre(data.text));
    const packetField = field("Frozen packet ID");
    body.append(
      packetField.wrap,
      button("Inspect packet freshness", () =>
        packetPreview(packetField.input.value),
      ),
    );
    if (writable) {
      const form = node("div", "review-form");
      form.append(
        node("h3", "", "Update this local record"),
        node(
          "p",
          "metadata",
          "This changes your tracker. It sends no message or application.",
        ),
      );
      const stage = node("select");
      for (const v of [
        "saved",
        "prepared",
        "applied",
        "introduction",
        "screening",
        "technical",
        "final",
        "offer",
        "closed",
        "needs-review",
      ]) {
        const o = node("option", "", label(v));
        o.value = v;
        o.selected = v === row.stage;
        stage.append(o);
      }
      const next = field("Next action", "text", row.next_action || "");
      const reason = field("Your reason / observed evidence", "textarea");
      const key = crypto.randomUUID();
      form.append(
        stage,
        next.wrap,
        reason.wrap,
        button("Record reviewed update", async () => {
          if (!reason.input.value.trim())
            return message(
              "Add your reason or observed evidence before saving.",
            );
          try {
            await post("/api/tracking", {
              record: row.record_path,
              tracking: { stage: stage.value, next_action: next.input.value },
              reason: reason.input.value,
              idempotency_key: key,
            });
            message("Reviewed local update recorded.");
            $("detail").close();
            load();
          } catch (e) {
            message(e.message);
          }
        }),
      );
      body.append(form);
    }
  } catch (e) {
    body.append(empty(e.message));
  }
}
async function packetPreview(id) {
  const body = modal("Frozen application packet");
  try {
    const p = await read(
      "/api/packet?" + new URLSearchParams({ packet_id: id }),
    );
    body.append(
      node(
        "h3",
        "",
        p.stale ? "Sources changed: review before use" : "Frozen sources match",
      ),
      node("p", "metadata", "This preview grants no submission permit."),
      pre(JSON.stringify(p, null, 2)),
    );
  } catch (e) {
    body.append(empty(e.message));
  }
}
async function pipeline() {
  const token = state.request;
  const view = $("view");
  view.append(
    section(
      "Applications",
      "Company context and exact source records, in one place.",
    ),
  );
  const filters = node("div", "filters");
  const search = field("Search pipeline", "search", state.query);
  const stages = node("select");
  for (const v of [
    "",
    "saved",
    "prepared",
    "applied",
    "screening",
    "technical",
    "final",
    "offer",
    "closed",
    "needs-review",
  ]) {
    const o = node("option", "", v ? label(v) : "Every stage");
    o.value = v;
    o.selected = v === state.stage;
    stages.append(o);
  }
  let timer;
  search.input.addEventListener("input", () => {
    clearTimeout(timer);
    timer = setTimeout(() => {
      state.query = search.input.value;
      state.offset = 0;
      load();
    }, 300);
  });
  stages.addEventListener("change", () => {
    state.stage = stages.value;
    state.offset = 0;
    load();
  });
  filters.append(
    search.wrap,
    stages,
    button(state.kanban ? "List view" : "Kanban", () => {
      state.kanban = !state.kanban;
      load();
    }),
  );
  view.append(filters);
  const query = new URLSearchParams({ offset: String(state.offset) });
  if (state.query) query.set("q", state.query);
  if (state.stage) query.set("stage", state.stage);
  const data = await read("/api/pipeline?" + query);
  if (token !== state.request) return;
  const rows = data.matches || [];
  if (!rows.length)
    view.append(
      empty(
        "No applications match this view. Your agent or CLI can add suitable roles.",
      ),
    );
  if (state.kanban) {
    const board = node("div", "kanban");
    board.tabIndex = 0;
    board.setAttribute("role", "region");
    board.setAttribute(
      "aria-label",
      "Application stages; scroll across the board",
    );
    for (const [title, stages] of [
      ["Saved", ["saved"]],
      ["Prepared", ["prepared"]],
      ["Applied", ["applied"]],
      ["Conversations", ["introduction", "screening", "technical", "final"]],
      ["Offers", ["offer"]],
      ["Review / closed", ["needs-review", "closed"]],
    ]) {
      const column = node("section", "kanban-column");
      const group = rows.filter((r) => stages.includes(r.stage));
      column.append(node("h3", "", `${title} · ${group.length}`));
      for (const row of group) column.append(recordCard(row));
      board.append(column);
    }
    view.append(board);
    view.append(
      node("p", "metadata", "Scroll across the board to see every stage."),
    );
  } else {
    const list = node("div", "records");
    for (const row of rows) list.append(recordCard(row));
    view.append(list);
  }
  const page = data.page || {};
  message(
    `${page.total ?? rows.length} matching records · ${rows.length} shown`,
  );
  const paging = node("div", "paging");
  if (state.offset)
    paging.append(
      button("Previous", () => {
        state.offset = Math.max(0, state.offset - 50);
        load();
      }),
    );
  if (page.next_offset != null)
    paging.append(
      button("Next page", () => {
        state.offset = page.next_offset;
        load();
      }),
    );
  view.append(paging);
}
async function attention() {
  const token = state.request;
  const data = await read(
    "/api/attention?" +
      new URLSearchParams({ offset: String(state.attentionOffset) }),
  );
  if (token !== state.request) return;
  const view = $("view");
  view.append(
    section(
      "Attention queue",
      "Saved observations and durable coordination, across sessions.",
    ),
  );
  view.append(
    node(
      "p",
      "metadata",
      "Live email is not scanned here. An authenticated mail connector supplies new observations.",
    ),
  );
  for (const item of data.items || []) {
    const card = node("article", "attention-card");
    card.append(
      node("span", "stage", label(item.kind)),
      node(
        "h3",
        "",
        item.company ||
          item.title ||
          item.record_path ||
          item.attempt_id ||
          "Recorded next step",
      ),
      node(
        "p",
        "",
        item.next_action ||
          item.message ||
          item.status_detail ||
          "Inspect the linked source before acting.",
      ),
    );
    if (item.scheduled_start)
      card.append(node("small", "", item.scheduled_start));
    const path = item.record_path || item.record;
    if (path)
      card.append(button("Open source", () => source(path), "citation"));
    if (item.packet_id)
      card.append(
        button(
          "Preview frozen packet",
          () => packetPreview(item.packet_id),
          "citation",
        ),
      );
    view.append(card);
  }
  if (!(data.items || []).length)
    view.append(empty("No recorded items currently need attention."));
  message(
    `${data.page?.total ?? data.items?.length ?? 0} recorded attention items · ${(data.items || []).length} shown`,
  );
  const paging = node("div", "paging");
  if (state.attentionOffset)
    paging.append(
      button("Previous", () => {
        state.attentionOffset = Math.max(0, state.attentionOffset - 50);
        load();
      }),
    );
  if (data.page?.next_offset != null)
    paging.append(
      button("Next page", () => {
        state.attentionOffset = data.page.next_offset;
        load();
      }),
    );
  view.append(paging);
}
async function knowledge() {
  const token = state.request;
  const view = $("view");
  view.append(
    section(
      "Notes & evidence",
      "Originals, proposed interpretations and accepted context stay distinct.",
    ),
  );
  const finder = field("Search sourced evidence", "search");
  view.append(finder.wrap);
  const results = node("div", "search-results");
  view.append(results);
  finder.input.addEventListener("change", async () => {
    results.replaceChildren();
    try {
      const data = await read(
        "/api/search?" + new URLSearchParams({ q: finder.input.value }),
      );
      for (const row of data.matches || []) {
        const card = node("article", "attention-card");
        card.append(
          button(row.title || row.path, () => source(row.path), "record-link"),
          node("span", "stage", label(row.kind)),
          node("p", "", row.excerpt),
          node("small", "metadata", row.path),
        );
        results.append(card);
      }
      if (!results.children.length)
        results.append(
          empty(
            "No lexical matches. Read sources before treating a search result as a fact.",
          ),
        );
    } catch (e) {
      results.append(empty(e.message));
    }
  });
  if (writable) view.append(importForm());
  else
    view.append(
      node(
        "p",
        "metadata",
        "To import or accept reviewed notes, start: job-harness gui --allow-local-writes",
      ),
    );
  const archiveChoice = node("label", "choice");
  const archived = node("input");
  archived.type = "checkbox";
  archived.checked = state.archivedNotes;
  archiveChoice.append(
    archived,
    node("span", "metadata", "Include retained archived notes"),
  );
  archived.addEventListener("change", () => {
    state.archivedNotes = archived.checked;
    state.notesOffset = 0;
    load();
  });
  view.append(archiveChoice);
  const data = await read(
    "/api/notes?" +
      new URLSearchParams({
        offset: String(state.notesOffset),
        archived: String(state.archivedNotes),
      }),
  );
  if (token !== state.request) return;
  for (const note of data.notes || []) {
    const card = node("article", "attention-card");
    card.append(
      node("span", "stage", label(note.source_kind)),
      button(
        note.source_label || note.note_id,
        () => noteReview(note.note_id),
        "record-link",
      ),
      node(
        "p",
        "",
        `${note.proposal_count || 0} proposals · ${note.accepted_review_count || 0} accepted reviews`,
      ),
      node(
        "small",
        "metadata",
        `Occurred: ${note.occurred_at || note.occurred_period || "not supplied"} · Recorded: ${note.recorded_at || "unknown"}`,
      ),
    );
    view.append(card);
  }
  if (!(data.notes || []).length)
    view.append(
      empty(
        "Add supplied notes or a transcript. Their original words stay immutable; summaries and proposed additions require review.",
      ),
    );
  const paging = node("div", "paging");
  if (state.notesOffset)
    paging.append(
      button("Previous notes", () => {
        state.notesOffset = Math.max(0, state.notesOffset - 50);
        load();
      }),
    );
  if (data.page?.next_offset != null)
    paging.append(
      button("More notes", () => {
        state.notesOffset = data.page.next_offset;
        load();
      }),
    );
  view.append(paging);
  message(
    `${data.page?.total ?? data.notes?.length ?? 0} retained notes · ${(data.notes || []).length} shown`,
  );
}
function importForm() {
  const form = node("details", "review-form");
  form.append(node("summary", "", "Add supplied notes / transcript"));
  const labelField = field("Source label");
  const text = field("Original text", "textarea");
  const file = field("Or choose a supplied text file", "file");
  file.input.accept = ".txt,.md";
  file.input.addEventListener("change", async () => {
    const selected = file.input.files[0];
    if (selected) {
      if (selected.size > 1024 * 1024)
        return message("Use a text file up to 1 MiB.");
      text.input.value = await selected.text();
      labelField.input.value = selected.name;
    }
  });
  const kind = node("select");
  for (const k of [
    "personal_notes",
    "transcript",
    "provider_summary",
    "generated_summary",
  ]) {
    const o = node("option", "", label(k));
    o.value = k;
    kind.append(o);
  }
  const link = field("Linked application path (optional)", "text");
  const project = field("Linked project path (optional)", "text");
  const temporal = [
    field("Occurred date or period (optional)"),
    field("Valid from (supplied, optional)"),
    field("Valid to (supplied, optional)"),
    field("Supersedes note ID (optional)"),
    field("Source group (optional)"),
  ];
  let key = crypto.randomUUID();
  form.append(
    labelField.wrap,
    kind,
    file.wrap,
    text.wrap,
    link.wrap,
    project.wrap,
    ...temporal.map((f) => f.wrap),
    button("Keep original and link context", async () => {
      const links = [];
      if (link.input.value)
        links.push({ kind: "application", path: link.input.value });
      if (project.input.value)
        links.push({ kind: "project", path: project.input.value });
      try {
        const data = await post("/api/notes/import", {
          text: text.input.value,
          source_kind: kind.value,
          source_label: labelField.input.value || "Supplied notes",
          ...Object.fromEntries(
            [
              "occurred_period",
              "valid_from",
              "valid_to",
              "supersedes",
              "source_group",
            ]
              .map((key, i) => [key, temporal[i].input.value])
              .filter(([, value]) => value.trim()),
          ),
          links,
          idempotency_key: key,
        });
        key = crypto.randomUUID();
        text.input.value = "";
        file.input.value = "";
        labelField.input.value = "";
        await load();
        message("Original preserved. Review proposed interpretations next.");
        noteReview(data.note_id);
      } catch (e) {
        message(e.message);
      }
    }),
  );
  return form;
}
async function noteReview(id, proposalId) {
  const body = modal("Review supplied context");
  const query = new URLSearchParams({ note_id: id });
  if (proposalId) query.set("proposal_id", proposalId);
  try {
    const data = await read("/api/notes/review?" + query);
    if (!proposalId && data.proposals?.length)
      return noteReview(id, data.proposals.at(-1).proposal_id);
    body.append(
      node(
        "p",
        "metadata",
        `${label(data.metadata?.source_kind)} · ${data.original.path}`,
      ),
      node(
        "h3",
        "",
        data.source_fresh
          ? "Immutable original"
          : "Source mismatch: inspect before accepting",
      ),
      pre(data.original.text),
    );
    const selectors = node("div", "related");
    for (const p of data.proposals || [])
      selectors.append(
        button(
          `Proposal ${p.proposal_id}`,
          () => noteReview(id, p.proposal_id),
          "citation",
        ),
      );
    body.append(selectors);
    const proposal = data.proposal;
    if (proposal) {
      for (const question of proposal.questions || []) {
        body.append(
          node(
            "p",
            "metadata",
            typeof question === "string"
              ? question
              : question.text || question.question || JSON.stringify(question),
          ),
        );
      }
      body.append(node("h3", "", "Proposed interpretation"));
      const selected = [];
      for (const item of proposal.items || []) {
        const block = node("article", "proposal-item");
        const choose = node("input");
        choose.type = "checkbox";
        choose.disabled = !writable;
        const row = node("label", "choice");
        row.append(choose, node("strong", "", label(item.kind)));
        block.append(row, node("p", "", item.text));
        for (const c of item.citations || []) {
          block.append(citation(c));
          if (c.excerpt) block.append(pre(c.excerpt));
        }
        choose.addEventListener("change", () => {
          const at = selected.indexOf(item.id);
          if (choose.checked && at < 0) selected.push(item.id);
          if (!choose.checked && at >= 0) selected.splice(at, 1);
        });
        body.append(block);
      }
      if (writable && !data.archived) {
        const by = field("Approved by");
        const evidence = field("Your review statement", "textarea");
        const key = crypto.randomUUID();
        body.append(
          by.wrap,
          evidence.wrap,
          button("Accept selected items", async () => {
            if (
              !selected.length ||
              !by.input.value.trim() ||
              !evidence.input.value.trim()
            )
              return message(
                "Select supported items and add your name and review statement.",
              );
            const hash =
              (data.proposals || []).find(
                (p) => p.proposal_id === proposal.proposal_id,
              )?.sha256 || proposal.sha256;
            try {
              await post("/api/notes/accept", {
                note_id: id,
                proposal_id: proposal.proposal_id,
                expected_proposal_sha256: hash,
                selected_item_ids: selected,
                approved_by: by.input.value,
                approval_evidence: evidence.input.value,
                idempotency_key: key,
              });
              message(
                "Selected context accepted with citations. CV facts were not automatically changed.",
              );
              noteReview(id, proposal.proposal_id);
            } catch (e) {
              message(e.message);
            }
          }),
        );
      }
    }
    for (const accepted of data.accepted_reviews || []) {
      body.append(
        node("h3", "", `Accepted review · ${accepted.approved_by}`),
        button(
          accepted.accepted_path,
          () => source(accepted.accepted_path),
          "citation",
        ),
      );
    }
    if (writable && !data.archived) {
      const draft = field("Draft a proposed summary / addition", "textarea");
      const key = crypto.randomUUID();
      body.append(
        node("h3", "", "Create a sourced proposal"),
        node(
          "p",
          "metadata",
          "Your draft is a proposal, not a confirmed fact. Verify its support in the original before acceptance.",
        ),
        draft.wrap,
        button("Save proposal for review", async () => {
          try {
            const p = await post("/api/notes/propose", {
              note_id: id,
              expected_source_sha256: data.original.sha256,
              items: [
                {
                  id: "summary",
                  kind: "summary",
                  text: draft.input.value,
                  citations: [
                    {
                      path: data.original.path,
                      sha256: data.original.sha256,
                      start_line: 1,
                      end_line: data.original.line_count,
                    },
                  ],
                },
              ],
              idempotency_key: key,
            });
            noteReview(id, p.proposal_id);
          } catch (e) {
            message(e.message);
          }
        }),
      );
      const reason = field("Archive reason / evidence", "textarea");
      const archiveKey = crypto.randomUUID();
      body.append(
        reason.wrap,
        button("Archive note and retain evidence", async () => {
          if (!reason.input.value.trim())
            return message("Add an archive reason first.");
          try {
            await post("/api/notes/archive", {
              note_id: id,
              expected_metadata_sha256: data.metadata_sha256,
              reason: reason.input.value,
              evidence: reason.input.value,
              idempotency_key: archiveKey,
            });
            message("Note archived; original and reviews retained.");
            $("detail").close();
            load();
          } catch (e) {
            message(e.message);
          }
        }),
      );
    }
  } catch (e) {
    body.append(empty(e.message));
  }
}
async function connections() {
  const token = state.request;
  const data = await read("/api/connections");
  if (token !== state.request) return;
  const view = $("view");
  view.append(
    section(
      "Agent, model and workspace",
      "Access stays in your host. This panel does not download models or read credentials.",
    ),
  );
  for (const [title, text, command] of [
    [
      "Onboarding",
      "Supply an approved PDF and attributed answers in the private workspace.",
      "job-harness call bootstrap",
    ],
    [
      "MCP",
      "Any supporting agent uses the fixed workspace and versioned tools.",
      "job-harness connect mcp",
    ],
    [
      "ACP",
      "Launch a trusted authenticated agent. Submission boundaries require explicit external scope.",
      "job-harness acp --agent /absolute/path/to/agent --prompt TASK",
    ],
    [
      "Local / remote model",
      "Choose a compatible tool-capable model. Keys are selected by environment variable name.",
      "job-harness agent run --endpoint URL --model MODEL --prompt TASK",
    ],
    [
      "Optional context adapters",
      "Semantic retrieval, summaries and supplied audio need explicitly selected host resources.",
      "job-harness context --help",
    ],
  ]) {
    const card = node("article", "attention-card");
    card.append(node("h3", "", title), node("p", "", text), pre(command));
    view.append(card);
  }
  view.append(node("h3", "", "Optional resource choices"));
  for (const capability of data.models?.capabilities || []) {
    const card = node("article", "attention-card");
    card.append(
      node("h3", "", label(capability.id)),
      node("p", "", capability.question),
      node("p", "metadata", `Requires: ${capability.requires}`),
    );
    if (capability.id !== "lexical")
      card.append(
        node(
          "p",
          "metadata",
          `Download: ${capability.download_bytes == null ? "unknown until a model/runtime is chosen" : capability.download_bytes + " bytes"} · Working memory: ${capability.working_memory_bytes == null ? "unknown until chosen" : capability.working_memory_bytes + " bytes"}`,
        ),
      );
    view.append(card);
  }
  for (const profile of data.models?.profiles || []) {
    const card = node("article", "attention-card");
    card.append(node("h3", "", profile.id), node("p", "", (profile.capabilities || []).join(" · ")));
    card.append(node("p", "metadata", `Status: ${label(profile.status)} · Explicit download: ${(profile.download_bytes / 1024 / 1024).toFixed(0)} MiB of model files`));
    card.append(node("p", "metadata", profile.hardware));
    card.append(node("p", "metadata", profile.working_memory_bytes == null ? "Working memory requirement is unknown; see scoped measurements before choosing." : `Working memory: ${profile.working_memory_bytes} bytes`));
    if (profile.measured?.scope) card.append(node("p", "metadata", profile.measured.scope));
    if (profile.model_card) card.append(safeLink(profile.model_card, "Model source and requirements"));
    view.append(card);
  }
  view.append(
    node(
      "p",
      "metadata",
      `Workspace: ${data.workspace} · ${data.mode} · version ${data.version}`,
    ),
  );
  message(
    "Connection capabilities shown; no model or connector request was made.",
  );
}
async function load() {
  const token = ++state.request;
  $("refresh").disabled = true;
  const title = titles[state.view];
  $("page-title").textContent = title[0];
  $("page-subtitle").textContent = title[1];
  $("view").replaceChildren();
  message("Reading your workspace…");
  try {
    const overview = await read("/api/overview");
    const health = overview.health || {};
    $("total").textContent = health.records ?? "—";
    $("runs").textContent = health.unfinished_run_count ?? "—";
    $("recovery").textContent = health.unresolved_attempt_count ?? "—";
    $("health").textContent = "Connected";
    if (token !== state.request) return;
    await { pipeline, attention, knowledge, connections }[state.view]();
  } catch (e) {
    if (token === state.request) {
      $("health").textContent = "Check workspace";
      message(e.message);
    }
  } finally {
    if (token === state.request) $("refresh").disabled = false;
  }
}
const initial = location.hash.slice(1);
if (titles[initial]) state.view = initial;
if (initial.startsWith("note=")) state.view = "knowledge";
document
  .querySelectorAll("[data-view]")
  .forEach((el) =>
    el.classList.toggle("active", el.dataset.view === state.view),
  );
$("mode-label").textContent = writable
  ? "Local, private · review enabled"
  : "Local, private · read only";
document.querySelectorAll("[data-view]").forEach((el) =>
  el.addEventListener("click", () => {
    state.view = el.dataset.view;
    location.hash = state.view;
    document
      .querySelectorAll("[data-view]")
      .forEach((n) => n.classList.toggle("active", n === el));
    load();
  }),
);
$("refresh").addEventListener("click", load);
$("focus").addEventListener("change", () => {
  state.focus = $("focus").value;
  const focus = focuses[state.focus];
  $("focus-hint").textContent = focus.hint;
  state.view = focus.view;
  document.querySelectorAll("[data-view]").forEach((item) => item.classList.toggle("active", item.dataset.view === state.view));
  load();
});
$("playbook").addEventListener("click", () => {
  const body = modal($("focus").selectedOptions[0].textContent);
  body.append(node("p", "", focuses[state.focus].task));
  body.append(node("p", "metadata", "This view changes presentation for this browser session. It records no employment status or new fact. Use your connected agent for interpretation and writing."));
  body.append(button("Read the short playbooks", () => source("docs/workflows.md"), "citation"));
});
$("close-detail").addEventListener("click", () => $("detail").close());
load().then(() => {
  if (initial.startsWith("note="))
    noteReview(decodeURIComponent(initial.slice(5)));
});
