"use strict";
const $ = (id) => document.getElementById(id);
let writable =
  document.querySelector('meta[name="job-harness-mode"]').content ===
  "local_writes";
let csrf = document.querySelector('meta[name="job-harness-csrf"]').content;
try {
  if (csrf) sessionStorage.setItem("job-harness-session-nonce", csrf);
  else csrf = sessionStorage.getItem("job-harness-session-nonce") || "";
} catch {}
if (location.pathname.startsWith("/launch/"))
  history.replaceState(null, "", "/" + location.hash);
const state = {
  focus: "searching",
  view: "pipeline",
  kanban: true,
  offset: 0,
  query: "",
  stage: "",
  request: 0,
  notesOffset: 0,
  attentionOffset: 0,
  archivedNotes: false,
};
let detailReturn = null;
let detailSnapshot = null;
let detailGeneration = 0;
let detailOpener = null;
let reviewingNote = null;
const focuses = {
  searching: {
    view: "pipeline",
    hint: "Bring applications and unanswered requests forward.",
    task: "Review applications, unanswered requests and the next interview. Preserve unknown outcomes before retrying.",
  },
  building: {
    view: "knowledge",
    hint: "Capture useful work, project context and learning while it is fresh.",
    task: "Paste a rough work note. Keep the original, link it to a project when known, and review suggested summaries separately.",
  },
  exploring: {
    view: "knowledge",
    hint: "Review preferences and evidence for a possible move.",
    task: "Find relevant preferences and project stories. Let your agent flag missing evidence and draft the documents you choose.",
  },
  starting: {
    view: "knowledge",
    hint: "Bring new responsibilities, milestones and learning together.",
    task: "Save milestones and learning, then review that period of work using recorded dates and flag anything missing.",
  },
};
const titles = {
  pipeline: ["Applications"],
  attention: [
    "Daily review",
    "Recorded requests, interviews and work awaiting review.",
  ],
  knowledge: [
    "Notes & evidence",
    "Your notes, suggested updates and reviewed additions.",
  ],
  sources: [
    "Search evidence",
    "Search the selected workspace. Open sources to inspect their exact wording.",
  ],
  activity: ["Activity", "Changes recorded by you and your connected agents."],
  connections: [
    "Connections",
    "Workspace settings, agents and optional model resources.",
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
  e.addEventListener("click", async (event) => {
    let result;
    try {
      result = action(event);
    } catch (error) {
      message(error.message);
      return;
    }
    if (result && typeof result.then === "function") {
      e.disabled = true;
      try {
        await result;
      } catch (error) {
        message(error.message);
      } finally {
        if (e.isConnected) e.disabled = false;
      }
    }
  });
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
  const names = {
    saved: "Saved",
    prepared: "Materials ready",
    applied: "Applied",
    introduction: "Introduction",
    screening: "Screening",
    technical: "Technical interview",
    final: "Final interview",
    offer: "Offer",
    closed: "Closed",
    "needs-review": "Needs review",
    personal_notes: "Personal notes",
    transcript: "Transcript",
    provider_summary: "Summary from another tool",
    generated_summary: "Draft summary",
    unknown_submission: "Verify submission",
    supplied_mail: "Unanswered request",
    pending_note_review: "Note to review",
    interview: "Interview",
    reserved: "Materials reserved",
    local_writes: "Review enabled",
    read_only: "Read only",
  };
  if (names[value]) return names[value];
  const text = String(value || "Source")
    .replaceAll("_", " ")
    .replaceAll("-", " ");
  return text.charAt(0).toUpperCase() + text.slice(1);
}
function technical(title, content) {
  const details = node("details", "technical-details");
  details.append(
    node("summary", "", title),
    typeof content === "string" ? pre(content) : content,
  );
  return details;
}
function trackingSnapshot(row, source) {
  const blocks = [
    ...source.text.matchAll(/<!-- tracking\r?\n([\s\S]*?)\r?\n-->/g),
  ];
  if (blocks.length !== 1)
    throw new Error(
      "This application needs a tracker review before editing. Ask your connected agent to organize its saved record.",
    );
  const tracking = JSON.parse(blocks[0][1]);
  const title = source.text.match(/^# (.+)$/m)?.[1] || row.title;
  return { ...row, ...tracking, title };
}
function sourceName(path) {
  return String(path || "Source")
    .split("/")
    .at(-1)
    .replace(/\.md$/, "")
    .replaceAll("-", " ");
}
function narrative(text) {
  return String(text || "")
    .replace(/<!-- tracking[\s\S]*?-->/g, "")
    .trim();
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
      const group = node("span", "external-group");
      group.append(
        a,
        button(
          "Copy link",
          async () => {
            try {
              await navigator.clipboard.writeText(u.href);
              message("Link copied.");
            } catch {
              message("Copy this link: " + u.href);
            }
          },
          "citation",
        ),
      );
      return group;
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
        ? { "x-job-harness-csrf": csrf }
        : { "content-type": "application/json", "x-job-harness-csrf": csrf },
    body: data === undefined ? undefined : JSON.stringify(data),
  });
  if (!response.ok) {
    if (response.status === 401 || response.status === 403)
      $("reload-session").hidden = false;
    throw new Error(
      response.status === 405
        ? "This session is read only. Refresh to see its current review mode."
        : response.status === 403 || response.status === 401
        ? "This local review session needs a reload."
        : `Local request rejected (${response.status}).`,
    );
  }
  const result = await response.json();
  if (!result.ok)
    throw new Error(
      `${result.error?.message || "Workspace operation failed."} ${result.error?.recovery || ""}`,
    );
  return result.data;
}
const read = (path) => request(path);
const post = async (path, data) => {
  state.saving = (state.saving || 0) + 1;
  try {
    const result = await request(path, data);
    state.revision = null;
    state.pendingChange = null;
    state.writeEpoch = (state.writeEpoch || 0) + 1;
    return result;
  } finally {
    state.saving -= 1;
  }
};
function message(text) {
  $("status").textContent = text;
  if ($("detail").open) $("detail-status").textContent = text;
}
function empty(text) {
  return node("div", "empty", text);
}
function section(title, sub) {
  const e = node("div", "section-top");
  const a = node("div");
  a.append(node("h2", "", title));
  e.append(a);
  if (!writable) e.append(node("span", "mode-description", "Read only"));
  else e.hidden = true;
  return e;
}
function pre(text) {
  return node("pre", "source-text", text || "No text recorded.");
}
function reviewDirty() {
  return (
    $("detail").open &&
    (detailSnapshot?.content.dataset.dirty === "true" ||
      $("detail-body").querySelector(".detail-content")?.dataset.dirty ===
        "true")
  );
}
function askDiscard(proceed) {
  $("discard-review")?.remove();
  const choice = node("div", "discard-choice");
  choice.id = "discard-review";
  choice.setAttribute("role", "alert");
  const actions = node("div", "toolbar-actions");
  actions.append(
    button("Keep editing", () => {
      choice.remove();
      $("detail-title").focus();
    }),
    button(
      "Discard edits",
      () => {
        delete $("detail-body").querySelector(".detail-content")?.dataset.dirty;
        if (detailSnapshot) delete detailSnapshot.content.dataset.dirty;
        choice.remove();
        proceed();
      },
      "danger",
    ),
  );
  choice.append(node("p", "", "This review has unsaved changes."), actions);
  $("detail-body").prepend(choice);
  $("detail-body").scrollTop = 0;
  actions.firstElementChild.focus();
}
function modal(title, kind = "inspector", preserveReturn = false, retry) {
  if (
    $("detail").open &&
    reviewDirty() &&
    !(preserveReturn && detailSnapshot)
  ) {
    askDiscard(retry || (() => closeDetail(true)));
    return null;
  }
  if (!$("detail").open) detailOpener = document.activeElement;
  openPicker?.close();
  detailGeneration += 1;
  $("detail-title").textContent = title;
  const content = node("div", "detail-content");
  content.dataset.generation = String(detailGeneration);
  $("detail-body").replaceChildren(content);
  const markDraftDirty = (event) => {
    // Download consent is a transient action gate, not an unsaved document.
    if (event.target.closest(".download-confirm")) return;
    content.dataset.dirty = "true";
  };
  content.addEventListener("input", markDraftDirty);
  content.addEventListener("change", markDraftDirty);
  $("detail-status").textContent = "";
  if (!preserveReturn) {
    detailReturn = null;
    detailSnapshot = null;
    reviewingNote = null;
  }
  $("detail").classList.toggle("document-view", kind === "document");
  document
    .querySelector(".shell")
    .classList.toggle("inspector-open", kind !== "document");
  document
    .querySelector(".shell")
    .classList.toggle("document-open", kind === "document");
  $("close-detail").textContent = detailReturn
    ? "Back to review"
    : kind === "document"
      ? "Back"
      : "Close";
  if (!$("detail").open) $("detail").show();
  $("detail-title").tabIndex = -1;
  $("detail-title").focus();
  return content;
}
async function source(path) {
  if (reviewingNote && $("detail").classList.contains("document-view")) {
    const snapshot = {
      content: $("detail-body").querySelector(".detail-content"),
      title: $("detail-title").textContent,
      note: reviewingNote,
      scroll: $("detail-body").scrollTop,
      focus: document.activeElement,
    };
    detailSnapshot = snapshot;
    detailReturn = () => {
      const restored = modal(snapshot.title, "document", true);
      if (!restored) return;
      detailSnapshot = null;
      $("detail-body").replaceChildren(snapshot.content);
      snapshot.content.dataset.generation = String(detailGeneration);
      reviewingNote = snapshot.note;
      $("detail-body").scrollTop = snapshot.scroll;
      if (snapshot.focus?.isConnected) snapshot.focus.focus();
      $("close-detail").textContent = "Back";
    };
  }
  const body = modal(sourceName(path), "inspector", true, () => source(path));
  if (!body) return;
  const generation = detailGeneration;
  body.append(empty("Reading source…"));
  try {
    const data = await read("/api/source?" + new URLSearchParams({ path }));
    if (generation !== detailGeneration || !$("detail").open) return;
    body.replaceChildren(
      pre(narrative(data.text)),
      technical(
        "Source details",
        `Path: ${path}\nSHA-256: ${data.sha256 || "Unknown"}\n\n${data.text}`,
      ),
    );
  } catch (e) {
    body.replaceChildren(empty(e.message));
  }
}
function citation(c, action) {
  const link = button(
    `${sourceName(c.path)} · lines ${c.start_line}–${c.end_line}`,
    () => (action ? action() : source(c.path)),
    "citation",
  );
  link.title = c.path;
  return link;
}
function roleTitle(row) {
  const title = row.title || row.record_path;
  const prefix = (row.company || "") + " — ";
  return row.company && title.startsWith(prefix)
    ? title.slice(prefix.length)
    : title;
}
function recordCard(row) {
  const card = node("article", "record");
  card.tabIndex = 0;
  card.setAttribute("role", "button");
  card.setAttribute(
    "aria-label",
    [row.company, roleTitle(row), label(row.stage)].join(" · "),
  );
  card.append(
    node("strong", "", row.company || "Application"),
    node("p", "", roleTitle(row)),
  );
  if (row.next_action) card.append(node("small", "", row.next_action));
  const open = () => {
    if (
      !state.dragging &&
      performance.now() > (state.suppressCardClickUntil || 0)
    )
      companyDetail(row);
  };
  card.addEventListener("click", open);
  card.addEventListener("keydown", (event) => {
    if (["Enter", " "].includes(event.key)) {
      event.preventDefault();
      open();
    }
  });
  card.draggable = false;
  if (writable)
    card.addEventListener("pointerdown", (event) => {
      if (event.button !== 0 || event.pointerType === "touch") return;
      if (state.saving) {
        message("A local save is in progress.");
        return;
      }
      dragGesture = {
        card,
        row,
        pointer: event.pointerId,
        x: event.clientX,
        y: event.clientY,
        moved: false,
        column: null,
      };
      card.setPointerCapture(event.pointerId);
    });
  return card;
}
let dragGesture = null;
function clearDrag() {
  const drag = dragGesture;
  dragGesture = null;
  state.dragging = false;
  if (drag) {
    drag.card.classList.remove("dragging");
    drag.card.style.removeProperty("transform");
    drag.card.style.removeProperty("z-index");
    try {
      drag.card.releasePointerCapture(drag.pointer);
    } catch {}
  }
  document
    .querySelectorAll(".drop-target")
    .forEach((column) => column.classList.remove("drop-target"));
  return drag;
}
document.addEventListener("pointermove", (event) => {
  const drag = dragGesture;
  if (!drag || event.pointerId !== drag.pointer) return;
  const dx = event.clientX - drag.x,
    dy = event.clientY - drag.y;
  if (!drag.moved && Math.hypot(dx, dy) < 6) return;
  drag.moved = true;
  state.dragging = true;
  drag.card.classList.add("dragging");
  drag.card.style.transform = "translate(" + dx + "px," + dy + "px)";
  drag.card.style.zIndex = "30";
  const column = document
    .elementFromPoint(event.clientX, event.clientY)
    ?.closest(".kanban-column");
  document
    .querySelectorAll(".drop-target")
    .forEach((item) => item.classList.remove("drop-target"));
  drag.column =
    column && column._stages && !column._stages.includes(drag.row.stage)
      ? column
      : null;
  if (drag.column) drag.column.classList.add("drop-target");
  event.preventDefault();
});
document.addEventListener("pointerup", (event) => {
  if (!dragGesture || event.pointerId !== dragGesture.pointer) return;
  const drag = clearDrag();
  if (drag.moved) {
    state.suppressCardClickUntil = performance.now() + 250;
    event.preventDefault();
    if (drag.column && writable)
      commitCardMove(drag.row, drag.column._stages[0]);
  }
});
document.addEventListener("pointercancel", clearDrag);
window.addEventListener("blur", clearDrag);
document.addEventListener("keydown", (event) => {
  if (event.key === "Escape" && dragGesture) {
    clearDrag();
    event.preventDefault();
  }
});
let openPicker = null;
function stagePicker(name, values, initial, changed = () => {}) {
  let value = initial;
  const wrapper = node("div", "stage-picker"),
    trigger = button(
      initial ? label(initial) : "All stages",
      () => toggle(),
      "stage-trigger",
    ),
    menu = node("div", "stage-options");
  trigger.setAttribute("aria-label", name);
  trigger.setAttribute("aria-haspopup", "listbox");
  trigger.setAttribute("aria-expanded", "false");
  menu.setAttribute("role", "listbox");
  menu.setAttribute("aria-label", name);
  menu.id = "choices-" + crypto.randomUUID();
  trigger.setAttribute("aria-controls", menu.id);
  menu.hidden = true;
  menu.addEventListener("keydown", (event) => {
    if (event.key === "Tab") close();
  });
  const options = values.map((item) => {
    const option = button(
      item ? label(item) : "All stages",
      () => pick(item),
      "stage-option",
    );
    option.setAttribute("role", "option");
    option.setAttribute("aria-selected", String(item === value));
    option.addEventListener("keydown", (event) => {
      const index = options.indexOf(option);
      if (["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
        event.preventDefault();
        options[
          event.key === "Home"
            ? 0
            : event.key === "End"
              ? options.length - 1
              : (index + (event.key === "ArrowDown" ? 1 : options.length - 1)) %
                options.length
        ].focus();
      } else if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        close();
        trigger.focus();
      }
    });
    return option;
  });
  function close() {
    menu.hidden = true;
    trigger.setAttribute("aria-expanded", "false");
    if (openPicker?.wrapper === wrapper) openPicker = null;
  }
  function toggle() {
    if (!menu.hidden) return close();
    openPicker?.close();
    menu.hidden = false;
    trigger.setAttribute("aria-expanded", "true");
    openPicker = { wrapper, close };
    options[Math.max(0, values.indexOf(value))].focus();
  }
  function pick(item) {
    value = item;
    trigger.textContent = item ? label(item) : "All stages";
    options.forEach((option, index) =>
      option.setAttribute("aria-selected", String(values[index] === item)),
    );
    close();
    trigger.focus();
    wrapper.dispatchEvent(new Event("change", { bubbles: true }));
    changed(value);
  }
  trigger.addEventListener("keydown", (event) => {
    if (["ArrowDown", "ArrowUp"].includes(event.key)) {
      event.preventDefault();
      toggle();
    }
  });
  menu.append(...options);
  wrapper.append(trigger, menu);
  return {
    wrapper,
    get value() {
      return value;
    },
  };
}
document.addEventListener("pointerdown", (event) => {
  if (openPicker && !openPicker.wrapper.contains(event.target))
    openPicker.close();
});
const stageValues = [
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
];
async function openPanel(name) {
  const body = modal(titles[name][0], "inspector", false, () =>
    openPanel(name),
  );
  if (!body) return;
  const generation = detailGeneration;
  body.classList.add("workspace-panel");
  try {
    await { attention, sources, activity, connections }[name](body);
  } catch (error) {
    if (body.isConnected) body.append(empty(error.message));
  } finally {
    if (generation === detailGeneration && body.isConnected)
      $("detail-title").focus();
  }
}
function table(headers, rows, className = "") {
  const wrap = node("div", "table-wrap");
  const table = node("table", className);
  const head = node("thead");
  const header = node("tr");
  for (const title of headers) {
    const cell = node("th", "", title);
    cell.scope = "col";
    header.append(cell);
  }
  head.append(header);
  const body = node("tbody");
  for (const cells of rows) {
    const row = node("tr");
    for (const value of cells) {
      const cell = node("td");
      cell.append(
        value instanceof Node ? value : node("span", "", value || "—"),
      );
      row.append(cell);
    }
    body.append(row);
  }
  table.append(head, body);
  wrap.append(table);
  return wrap;
}
async function commitCardMove(row, stage) {
  if (!writable || stage === row.stage) return;
  try {
    const current = await read(
      "/api/source?" + new URLSearchParams({ path: row.record_path }),
    );
    const fresh = trackingSnapshot(row, current);
    if (fresh.stage !== row.stage)
      throw new Error(
        "This application changed while you were moving it. Refresh the board and choose its current stage.",
      );
    await post("/api/tracking", {
      record: row.record_path,
      tracking: { stage },
      expected_record_sha256: current.sha256,
      board_move: true,
      reason:
        "Local user moved this card from " +
        label(row.stage) +
        " to " +
        label(stage) +
        ". This records the user's local tracking choice; no employer action or submission is inferred.",
      idempotency_key: crypto.randomUUID(),
    });
    await load();
    message((row.company || "Application") + " moved to " + label(stage) + ".");
  } catch (error) {
    message(error.message);
  }
}
function payloadKey() {
  let signature = null,
    key = null;
  return (payload) => {
    const next = JSON.stringify(payload);
    if (next !== signature) {
      signature = next;
      key = crypto.randomUUID();
    }
    return key;
  };
}
async function companyDetail(row) {
  const body = modal(row.company || row.title, "inspector", false, () =>
    companyDetail(row),
  );
  if (!body) return;
  const generation = detailGeneration;
  try {
    const data = await read(
      "/api/source?" + new URLSearchParams({ path: row.record_path }),
    );
    if (generation !== detailGeneration || !$("detail").open) return;
    try {
      row = trackingSnapshot(row, data);
    } catch (error) {
      if (writable) throw error;
    }
    body.append(
      node("p", "", roleTitle(row)),
      node(
        "p",
        "metadata",
        `${label(row.stage)} · ${row.next_action || "No next action recorded"}`,
      ),
    );
    if (row.vacancy_url) body.append(safeLink(row.vacancy_url, "Open vacancy"));
    const related = await read(
      "/api/pipeline?" + new URLSearchParams({ q: row.company || row.title }),
    );
    if (generation !== detailGeneration || !$("detail").open) return;
    const links = node("div", "related");
    for (const r of related.matches || [])
      links.append(button(r.title, () => companyDetail(r), "citation"));
    body.append(node("h3", "", "Linked applications"), links);
    await window.JobHarnessDailyReview.renderCompany(body, {
      read, node, button, safeLink, empty,
      openDaily: () => openPanel("attention"),
    }, row.record_path);
    if (generation !== detailGeneration || !$("detail").open) return;
    const context = await read(
      "/api/company?" +
        new URLSearchParams({
          company: row.company || row.title || "",
          record: row.record_path,
        }),
    );
    if (generation !== detailGeneration || !$("detail").open) return;
    body.append(node("h3", "", "Company notes and evidence"));
    for (const passage of context.sources || []) {
      const card = node("article", "attention-card");
      const provenance = passage.provenance || {};
      card.append(
        citation(passage),
        node(
          "p",
          "metadata",
          `${label(provenance.source_kind || "source")} · Date: ${provenance.occurred_at || provenance.occurred_period || "unknown"} · Saved: ${provenance.recorded_at || "unknown"}`,
        ),
        pre(narrative(passage.excerpt)),
      );
      body.append(card);
    }
    if (!(context.sources || []).length)
      body.append(empty("No linked company evidence has been recorded yet."));
    for (const gap of context.gaps || [])
      body.append(node("p", "metadata", gap.message));
    body.append(technical("Application source", pre(narrative(data.text))));
    const packetField = field("Application materials reference");
    const packetDetails = node("details", "technical-details");
    packetDetails.append(
      node("summary", "", "Inspect saved application materials"),
      packetField.wrap,
      button("Check materials", () => packetPreview(packetField.input.value)),
    );
    body.append(packetDetails);
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
      const stage = stagePicker("Application stage", stageValues, row.stage);
      const next = field("Next action", "text", row.next_action || "");
      const reason = field("Your reason / observed evidence", "textarea");
      const keyFor = payloadKey();
      const stageField = node("label", "field");
      stageField.append(node("span", "", "Stage"), stage.wrapper);
      form.append(
        stageField,
        next.wrap,
        reason.wrap,
        button("Save tracker update", async () => {
          if (!reason.input.value.trim())
            return message(
              "Add your reason or observed evidence before saving.",
            );
          try {
            await post("/api/tracking", {
              record: row.record_path,
              tracking: { stage: stage.value, next_action: next.input.value },
              reason: reason.input.value,
              expected_record_sha256: data.sha256,
              idempotency_key: keyFor({
                stage: stage.value,
                next: next.input.value,
                reason: reason.input.value,
                hash: data.sha256,
              }),
            });
            if (generation !== detailGeneration || !body.isConnected) return;
            message("Reviewed local update recorded.");
            closeDetail(true);
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
  reviewingNote = null;
  const body = modal("Application materials", "inspector", false, () =>
    packetPreview(id),
  );
  if (!body) return;
  const generation = detailGeneration;
  try {
    const p = await read(
      "/api/packet?" + new URLSearchParams({ packet_id: id }),
    );
    if (generation !== detailGeneration || !$("detail").open) return;
    body.append(
      node(
        "h3",
        "",
        p.stale ? "Materials need another review" : "Materials are current",
      ),
      node("p", "metadata", "Checking materials does not send an application."),
      technical("Detailed material checks", JSON.stringify(p, null, 2)),
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
  const stages = stagePicker(
    "Filter application stage",
    ["", ...stageValues],
    state.stage,
    (value) => {
      state.stage = value;
      state.offset = 0;
      load();
    },
  );
  search.input.placeholder = "Search company or role";
  search.input.setAttribute("aria-label", "Search applications");
  const applySearch = () => {
    state.query = search.input.value;
    state.offset = 0;
    load();
  };
  search.input.addEventListener("keydown", (event) => {
    if (event.key === "Enter") applySearch();
  });
  filters.append(
    search.wrap,
    button("Search", applySearch),
    stages.wrapper,
    button(
      state.kanban ? "Table" : "Board",
      () => {
        state.kanban = !state.kanban;
        state.offset = 0;
        load();
      },
      "view-toggle",
    ),
  );
  view.append(filters);
  const query = new URLSearchParams({ offset: String(state.offset) });
  if (state.query) query.set("q", state.query);
  if (state.stage) query.set("stage", state.stage);
  const data = await read("/api/pipeline?" + query);
  if (token !== state.request) return;
  const rows = [...(data.matches || [])];
  if (state.kanban) {
    let next = data.page?.next_offset,
      pages = 1;
    while (next != null && pages < 5) {
      query.set("offset", String(next));
      const more = await read("/api/pipeline?" + query);
      if (token !== state.request) return;
      rows.push(...(more.matches || []));
      next = more.page?.next_offset;
      pages += 1;
    }
    data.page = { ...data.page, next_offset: next };
    const seen = new Set();
    for (let index = 0; index < rows.length; ) {
      if (seen.has(rows[index].record_path)) rows.splice(index, 1);
      else {
        seen.add(rows[index].record_path);
        index += 1;
      }
    }
  }
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
      ["Materials ready", ["prepared"]],
      ["Applied", ["applied"]],
      ["Conversations", ["introduction", "screening", "technical", "final"]],
      ["Offers", ["offer"]],
      ["Review / closed", ["needs-review", "closed"]],
    ]) {
      const column = node("section", "kanban-column");
      const group = rows.filter((r) => stages.includes(r.stage));
      column.append(node("h3", "", `${title} · ${group.length}`));
      column.setAttribute("role", "list");
      column.setAttribute(
        "aria-label",
        title + " applications shown on this page",
      );
      column._stages = stages;
      for (const row of group) column.append(recordCard(row));
      board.append(column);
    }
    view.append(board);
    view.append(
      node(
        "p",
        "metadata",
        writable ? "Drag a card to move to another eligible stage." : "Read only.",
      ),
    );
  } else {
    if (rows.length)
      view.append(
        table(
          ["Company", "Role", "Stage", "Next action"],
          rows.map((row) => [
            button(
              row.company || row.title || "Application",
              () => companyDetail(row),
              "record-link",
            ),
            roleTitle(row),
            node("span", "stage", label(row.stage)),
            row.next_action || "No next action recorded",
          ]),
          "application-table",
        ),
      );
  }
  const page = data.page || {};
  message(
    state.offset || page.next_offset != null
      ? `Showing ${state.offset + 1}–${state.offset + rows.length} of ${page.total ?? rows.length} roles. Column counts reflect these shown roles.`
      : "",
  );
  const paging = node("div", "paging");
  if (state.offset)
    paging.append(
      button("Previous", () => {
        state.offset = Math.max(0, state.offset - (state.kanban ? 250 : 50));
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
async function attention(target) {
  const view = target || $("view");
  const token = state.request;
  await window.JobHarnessDailyReview.render(view, {
    node, button, read, post, safeLink, source, noteReview, packetPreview,
    companyDetail, empty, message, label, payloadKey,
  }, {
    writable,
    offset: state.attentionOffset,
    isCurrent: () => token === state.request,
    onUpdate: updateDailyCount,
    onPage: (offset) => {
      state.attentionOffset = offset;
      openPanel("attention");
    },
  });
}
function updateDailyCount(data) {
  const count = data.counts?.review_items ?? data.attention?.page?.total ?? 0;
  $("attention-button").textContent = "Daily review" + (count ? " · " + count : "");
  $("attention-button").setAttribute("aria-label", count + " recorded items in Daily review");
}
async function knowledge() {
  const token = state.request;
  const view = $("view");
  view.append(
    section(
      "Notes & evidence",
      "Interview notes, project stories and the sources behind your answers.",
    ),
  );
  view.append(
    node(
      "p",
      "notes-intro metadata",
      "Interview notes, project stories and the sources behind your answers.",
    ),
  );
  const actions = node("div", "notes-toolbar");
  if (!writable)
    view.append(
      node(
        "p",
        "metadata",
        "Read only. In the desktop app, turn off Workspace → Read only to add or review notes.",
      ),
    );
  const archiveChoice = node("label", "choice");
  const archived = node("input");
  archived.type = "checkbox";
  archived.checked = state.archivedNotes;
  archiveChoice.append(
    archived,
    node("span", "metadata", "Show archived notes"),
  );
  archived.addEventListener("change", () => {
    state.archivedNotes = archived.checked;
    state.notesOffset = 0;
    load();
  });
  actions.append(archiveChoice);
  if (writable) actions.append(button("Add note", composeNote, "primary"));
  view.append(actions);
  const data = await read(
    "/api/notes?" +
      new URLSearchParams({
        offset: String(state.notesOffset),
        archived: String(state.archivedNotes),
      }),
  );
  if (token !== state.request) return;
  const noteRows = (data.notes || []).map((note) => [
    button(
      note.source_label || note.note_id,
      () => noteReview(note.note_id),
      "record-link",
    ),
    node("span", "stage", label(note.source_kind)),
    String(note.proposal_count || 0),
    String(note.accepted_review_count || 0),
    note.occurred_at || note.occurred_period || "Not supplied",
  ]);
  if (noteRows.length)
    view.append(
      table(["Note", "Type", "Drafts", "Reviews", "When"], noteRows),
    );
  if (!(data.notes || []).length)
    view.append(
      empty(
        "Add a note or transcript. Keep the original, then review any proposed summary beside it.",
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
async function sources(target) {
  const view = target || $("view");
  view.append(section("Find evidence"));
  view.append(
    node(
      "p",
      "metadata",
      "Search saved projects, interview notes, answers and applications by their words. Open a result to read the original.",
    ),
  );
  const finder = field("Words to find", "search", state.sourceQuery || "");
  finder.input.placeholder = "Project, company or topic";
  finder.input.maxLength = 500;
  const results = node("div", "search-results");
  view.append(results);
  let searchGeneration = 0;
  const search = async () => {
    const generation = ++searchGeneration;
    state.sourceQuery = finder.input.value;
    results.replaceChildren();
    try {
      const data = await read(
        "/api/search?" + new URLSearchParams({ q: finder.input.value }),
      );
      if (!results.isConnected || generation !== searchGeneration) return;
      for (const row of data.matches || []) {
        const card = node("article", "evidence-result");
        card.tabIndex = 0;
        card.setAttribute("role", "button");
        card.setAttribute(
          "aria-label",
          "Read " + (row.title || sourceName(row.path)),
        );
        card.append(
          node("strong", "", row.title || sourceName(row.path)),
          node(
            "small",
            "metadata",
            label(row.kind) + " · " + sourceName(row.path),
          ),
          node("p", "", row.excerpt),
        );
        const open = () => source(row.path);
        card.addEventListener("click", open);
        card.addEventListener("keydown", (event) => {
          if (["Enter", " "].includes(event.key)) {
            event.preventDefault();
            open();
          }
        });
        results.append(card);
      }
      if (!results.children.length)
        results.append(
          empty(
            "No matches. Try a company, project name or a short phrase from your notes.",
          ),
        );
    } catch (e) {
      if (results.isConnected && generation === searchGeneration)
        results.append(empty(e.message));
    }
  };
  finder.input.addEventListener("keydown", (event) => {
    if (event.key === "Enter") search();
  });
  if (state.sourceQuery) await search();
  const controls = node("div", "search-controls");
  controls.append(finder.wrap, button("Search", search));
  view.insertBefore(controls, results);
}
function composeNote() {
  const body = modal("Add note", "document", false, composeNote);
  if (!body) return;
  body.append(importForm());
}
function importForm() {
  const form = node("section", "note-composer");
  const labelField = field("Title");
  labelField.input.placeholder = "e.g. Product dashboard review";
  const text = field("Your note or transcript", "textarea");
  text.input.placeholder =
    "Paste what happened, what you built or the words from a conversation.";
  const file = field("Import a text file (optional)", "file");
  file.input.accept = ".txt,.md";
  let fileGeneration = 0;
  file.input.addEventListener("change", async () => {
    const generation = ++fileGeneration;
    const selected = file.input.files[0];
    if (selected) {
      if (selected.size > 1024 * 1024)
        return message("Use a text file up to 1 MiB.");
      const content = await selected.text();
      if (generation !== fileGeneration || !text.input.isConnected) return;
      text.input.value = content;
      labelField.input.value = selected.name;
    }
  });
  const kind = node("select");
  kind.setAttribute("aria-label", "Note type");
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
  const link = field("Linked application (optional)");
  const applicationSelect = node("select");
  applicationSelect.append(node("option", "", "None"));
  applicationSelect.options[0].value = "";
  link.input.replaceWith(applicationSelect);
  link.input = applicationSelect;
  read("/api/pipeline")
    .then((data) => {
      if (!applicationSelect.isConnected) return;
      for (const row of data.matches || []) {
        const option = node(
          "option",
          "",
          `${row.company || "Application"} · ${roleTitle(row)}`,
        );
        option.value = row.record_path;
        applicationSelect.append(option);
      }
    })
    .catch(() =>
      message(
        "Application links are unavailable. You can preserve the original without one.",
      ),
    );
  const project = field("Linked project path (optional)", "text");
  const temporal = [
    field("Occurred date or period (optional)"),
    field("Valid from (supplied, optional)"),
    field("Valid to (supplied, optional)"),
    field("Supersedes note ID (optional)"),
    field("Source group (optional)"),
  ];
  const keyFor = payloadKey();
  const grid = node("div", "form-grid");
  const kindField = node("label", "field");
  kindField.append(node("span", "", "Note type"), kind);
  text.wrap.classList.add("full-width");
  const timing = node("details", "full-width");
  timing.append(
    node(
      "summary",
      "metadata",
      "Dates, source group and earlier version (optional)",
    ),
  );
  const timingGrid = node("div", "form-grid");
  timingGrid.append(...temporal.map((f) => f.wrap));
  timing.append(timingGrid);
  const advancedLinks = node("details", "full-width technical-details");
  advancedLinks.append(
    node("summary", "", "Advanced project link"),
    project.wrap,
  );
  grid.append(
    labelField.wrap,
    kindField,
    file.wrap,
    text.wrap,
    link.wrap,
    timing,
    advancedLinks,
  );
  form.append(
    grid,
    button(
      "Save note",
      async () => {
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
            idempotency_key: keyFor({
              text: text.input.value,
              kind: kind.value,
              label: labelField.input.value,
              links,
              temporal: temporal.map((f) => f.input.value),
            }),
          });
          if (!form.isConnected) return;
          delete form.closest(".detail-content").dataset.dirty;
          await load();
          if (form.isConnected) noteReview(data.note_id);
          message("Note saved. Its original text is retained.");
        } catch (e) {
          message(e.message);
        }
      },
      "primary",
    ),
  );
  return form;
}
async function noteReview(id, proposalId) {
  const body = modal("Review this note", "document", false, () =>
    noteReview(id, proposalId),
  );
  if (!body) return;
  const generation = detailGeneration;
  reviewingNote = { id, proposalId };
  const query = new URLSearchParams({ note_id: id });
  if (proposalId) query.set("proposal_id", proposalId);
  try {
    const data = await read("/api/notes/review?" + query);
    if (
      generation !== detailGeneration ||
      !$("detail").open ||
      reviewingNote?.id !== id
    )
      return;
    if (!proposalId && data.proposals?.length)
      return noteReview(id, data.proposals.at(-1).proposal_id);
    $("detail-title").textContent =
      data.metadata?.source_label || "Review this note";
    const intro = node("div", "review-intro");
    intro.append(
      node(
        "p",
        "metadata",
        `${label(data.metadata?.source_kind)} · ${data.metadata?.occurred_at || data.metadata?.occurred_period || "Occurrence date not supplied"}`,
      ),
      node(
        "p",
        data.source_fresh ? "metadata" : "warning",
        data.source_fresh
          ? "Original text is unchanged."
          : "Original text changed. Review it before accepting.",
      ),
    );
    body.append(intro);
    const comparison = node("div", "note-comparison");
    const original = node("section", "original-pane");
    const originalText = node("pre", "source-text original-text");
    const lines = String(data.original.text || "").split("\n");
    lines.forEach((line, index) => {
      const span = node("span", "source-line", line || " ");
      span.dataset.line = String(index + 1);
      originalText.append(span);
    });
    original.append(node("h3", "", "Original"), originalText);
    const proposed = node("section", "proposed-pane");
    proposed.append(node("h3", "", "Proposed items"));
    comparison.append(original, proposed);
    body.append(comparison);
    const selectors = node("div", "related");
    for (const [index, p] of (data.proposals || []).entries())
      selectors.append(
        button(
          `Draft ${index + 1}`,
          () => noteReview(id, p.proposal_id),
          "citation",
        ),
      );
    proposed.append(selectors);
    if (!data.source_fresh)
      proposed.append(
        node(
          "p",
          "warning",
          "The source changed. Review it before accepting any item.",
        ),
      );
    const proposal = data.proposal;
    if (proposal) {
      for (const question of proposal.questions || []) {
        proposed.append(
          node(
            "p",
            "metadata",
            typeof question === "string"
              ? question
              : question.text ||
                  question.question ||
                  "A question in this draft needs clarification.",
          ),
        );
      }
      const selected = [];
      const acceptedIds = new Set(
        (data.accepted_reviews || [])
          .filter((r) => r.proposal_id === proposal.proposal_id)
          .flatMap((r) => r.selected_item_ids || []),
      );
      let pendingCount = 0;
      for (const item of proposal.items || []) {
        if (acceptedIds.has(item.id)) continue;
        pendingCount += 1;
        const block = node("article", "proposal-item");
        const choose = node("input");
        choose.type = "checkbox";
        choose.disabled = !writable || data.archived || !data.source_fresh;
        const row = node("label", "choice");
        choose.setAttribute("aria-label", `${label(item.kind)}: ${item.text}`);
        row.append(choose, node("strong", "", label(item.kind)));
        block.append(row, node("p", "", item.text));
        for (const c of item.citations || []) {
          const link = citation(
            c,
            c.path === data.original.path && c.sha256 === data.original.sha256
              ? () =>
                  originalText
                    .querySelector(`[data-line="${c.start_line}"]`)
                    ?.scrollIntoView({
                      block: "nearest",
                      behavior: matchMedia("(prefers-reduced-motion: reduce)")
                        .matches
                        ? "auto"
                        : "smooth",
                    })
              : undefined,
          );
          const highlight = (active) => {
            if (
              c.path !== data.original.path ||
              c.sha256 !== data.original.sha256
            )
              return;
            originalText
              .querySelectorAll(".source-line")
              .forEach((line) =>
                line.classList.toggle(
                  "highlighted",
                  active &&
                    Number(line.dataset.line) >= c.start_line &&
                    Number(line.dataset.line) <= c.end_line,
                ),
              );
          };
          link.addEventListener("mouseenter", () => highlight(true));
          link.addEventListener("mouseleave", () => highlight(false));
          link.addEventListener("focus", () => highlight(true));
          link.addEventListener("blur", () => highlight(false));
          block.append(link);
          if (c.excerpt) block.append(pre(c.excerpt));
        }
        choose.addEventListener("change", () => {
          const at = selected.indexOf(item.id);
          if (choose.checked && at < 0) selected.push(item.id);
          if (!choose.checked && at >= 0) selected.splice(at, 1);
          const accept = proposed.querySelector(".accept-selected");
          if (accept) {
            accept.textContent = `Accept ${selected.length} selected`;
            accept.disabled = !selected.length || !data.source_fresh;
          }
        });
        proposed.append(block);
      }
      if (!pendingCount)
        proposed.append(
          empty("All items in this draft have been reviewed and saved."),
        );
      if (!writable)
        proposed.append(
          node(
            "p",
            "notice",
            "Read only. In the desktop app, turn off Workspace → Read only to accept selected items.",
          ),
        );
      if (writable && !data.archived && pendingCount) {
        const by = field("Your name");
        const evidence = field("Review note", "textarea");
        const reviewHelp = node(
          "span",
          "field-help",
          "Describe what you checked against the original before accepting these items.",
        );
        reviewHelp.id = "review-help-" + detailGeneration;
        evidence.input.setAttribute("aria-describedby", reviewHelp.id);
        evidence.wrap.append(reviewHelp);
        const keyFor = payloadKey();
        const accept = button(
          "Accept 0 selected",
          async () => {
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
                selected_item_ids: [...selected].sort(),
                approved_by: by.input.value,
                approval_evidence: evidence.input.value,
                idempotency_key: keyFor({
                  proposal: proposal.proposal_id,
                  hash,
                  selected: [...selected].sort(),
                  by: by.input.value,
                  evidence: evidence.input.value,
                }),
              });
              if (
                body.dataset.generation !== String(detailGeneration) ||
                !body.isConnected
              )
                return;
              delete body.dataset.dirty;
              await load();
              if (body.dataset.generation === String(detailGeneration))
                await noteReview(id, proposal.proposal_id);
              message(
                "Selected updates saved with source links. Your CV was not changed.",
              );
            } catch (e) {
              message(e.message);
            }
          },
          "primary accept-selected",
        );
        accept.disabled = true;
        const actions = node("div", "review-actions");
        actions.append(by.wrap, evidence.wrap, accept);
        proposed.append(actions);
      }
    }
    const acceptedSection = node("details", "review-section");
    acceptedSection.append(
      node(
        "summary",
        "",
        `Reviewed additions (${(data.accepted_reviews || []).length})`,
      ),
    );
    for (const accepted of data.accepted_reviews || []) {
      acceptedSection.append(
        node("h3", "", `Reviewed by ${accepted.approved_by}`),
        button(
          "Read saved additions",
          () => source(accepted.accepted_path),
          "citation",
        ),
      );
    }
    if ((data.accepted_reviews || []).length) body.append(acceptedSection);
    body.append(
      technical(
        "Source details",
        `Path: ${data.original.path}\nSHA-256: ${data.original.sha256}`,
      ),
    );
    if (writable && !data.archived) {
      const draft = field("Your suggested summary or addition", "textarea");
      const keyFor = payloadKey();
      const draftSection = node("details", "review-section");
      draftSection.append(node("summary", "", "Draft an update from this note"));
      draftSection.append(
        node(
          "p",
          "metadata",
          "Check your draft against the original note before saving it as a reviewed addition.",
        ),
        draft.wrap,
        button("Save draft for review", async () => {
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
              idempotency_key: keyFor({
                text: draft.input.value,
                hash: data.original.sha256,
              }),
            });
            if (
              body.dataset.generation !== String(detailGeneration) ||
              !body.isConnected
            )
              return;
            delete body.dataset.dirty;
            await load();
            if (body.dataset.generation === String(detailGeneration))
              await noteReview(id, p.proposal_id);
          } catch (e) {
            message(e.message);
          }
        }),
      );
      body.append(draftSection);
      const archiveSection = node("details", "review-section");
      archiveSection.append(node("summary", "", "Archive this note"));
      const reason = field("Archive reason / evidence", "textarea");
      const archiveKey = payloadKey();
      archiveSection.append(
        node(
          "p",
          "metadata",
          "The original and all reviews are retained. Archived notes can still be included in the notes view.",
        ),
        reason.wrap,
        button("Archive note", async () => {
          if (!reason.input.value.trim())
            return message("Add an archive reason first.");
          try {
            await post("/api/notes/archive", {
              note_id: id,
              expected_metadata_sha256: data.metadata_sha256,
              reason: reason.input.value,
              evidence: reason.input.value,
              idempotency_key: archiveKey({
                reason: reason.input.value,
                hash: data.metadata_sha256,
              }),
            });
            if (
              body.dataset.generation !== String(detailGeneration) ||
              !body.isConnected
            )
              return;
            message("Note archived; original and reviews retained.");
            closeDetail(true);
            load();
          } catch (e) {
            message(e.message);
          }
        }),
      );
      body.append(archiveSection);
    }
  } catch (e) {
    body.append(empty(e.message));
  }
}
function formatBytes(bytes) {
  if (bytes == null) return "Unknown";
  return bytes < 1024 * 1024 * 1024
    ? (bytes / 1024 / 1024).toFixed(0) + " MiB"
    : (bytes / 1024 / 1024 / 1024).toFixed(1) + " GiB";
}
function dateLabel(value) {
  const date = value && new Date(value);
  return date && !Number.isNaN(date.getTime())
    ? new Intl.DateTimeFormat(undefined, {
        dateStyle: "medium",
        timeStyle: "short",
      }).format(date)
    : "Unknown";
}
async function activity(target) {
  const view = target || $("view");
  const token = state.request;
  const data = await read("/api/activity");
  if (token !== state.request) return;
  const rows = [...(data.events || [])].reverse().map((event) => {
    const item = node("div");
    item.append(
      node(
        "p",
        "",
        event.summary
          ? event.summary.replace(/^Explicit (local )?GUI review:\s*/, "")
          : operationLabel(event.tool),
      ),
    );
    if (event.record_path)
      item.append(
        button("Open application", () => source(event.record_path), "citation"),
      );
    return [dateLabel(event.at), item];
  });
  view.append(
    rows.length
      ? table(["Recorded", "Change"], rows)
      : empty("No workspace changes have been recorded yet."),
  );
  message(
    "Showing " +
      rows.length +
      " of " +
      (data.page?.total || 0) +
      " recorded changes.",
  );
}
function operationLabel(tool) {
  const names = {
    start_run: "Work started",
    finish_run: "Work completed",
    update_application: "Application tracker updated",
    intake_note: "Original note preserved",
    propose_note: "Suggested note update saved",
    accept_note: "Reviewed additions saved",
    archive_note: "Note archived",
    prepare_application: "Application materials prepared",
    begin_submission: "Submission started",
    finish_submission: "Submission outcome recorded",
    record_event: "Evidence recorded",
    reconcile_tracker: "Tracker reviewed",
  };
  return names[tool] || "Workspace updated";
}
async function connections(target) {
  return window.renderConnections({
    target: target || $("view"),
    read,
    post,
    node,
    button,
    technical,
    safeLink,
    label,
    formatBytes,
    message,
    section,
  });
}

async function load() {
  const token = ++state.request;
  document.querySelectorAll("[data-view]").forEach((item) => {
    item.classList.toggle("active", item.dataset.view === state.view);
    if (item.dataset.view === state.view)
      item.setAttribute("aria-current", "page");
    else item.removeAttribute("aria-current");
  });
  state.loading = true;
  $("refresh").disabled = true;
  const title = titles[state.view];
  $("page-title").textContent = title[0];
  document.title = title[0] + " — Job Harness";
  $("update-notice").hidden = true;
  state.viewDirty = false;
  $("view").replaceChildren();
  message("Reading your workspace…");
  try {
    const overview = await read("/api/overview");
    const health = overview.health || {};
    if (token !== state.request) return;
    $("health").hidden = true;
    if (state.view === "pipeline") {
      const count = node("span", "title-count", " " + (health.records ?? ""));
      $("page-title").append(count);
    }
    const daily = await read("/api/daily?offset=0");
    if (token !== state.request) return;
    updateDailyCount(daily);
    if (token !== state.request) return;
    await { pipeline, attention, knowledge, sources, activity, connections }[
      state.view
    ]();
  } catch (e) {
    if (token === state.request) {
      $("health").hidden = false;
      $("health").textContent = "Check workspace";
      message(e.message);
    }
  } finally {
    if (token === state.request) {
      $("refresh").disabled = false;
      state.loading = false;
      if (state.pendingChange && !state.changeApplying && !interactionBusy())
        queueMicrotask(flushChanges);
    }
  }
}
const initial = location.hash.slice(1);
if (["pipeline", "knowledge"].includes(initial)) state.view = initial;
if (initial.startsWith("note=")) state.view = "knowledge";
document.querySelectorAll("[data-view]").forEach((el) => {
  el.classList.toggle("active", el.dataset.view === state.view);
  if (el.dataset.view === state.view) el.setAttribute("aria-current", "page");
  else el.removeAttribute("aria-current");
});
$("mode-label").textContent = writable ? "Local workspace" : "Read only";
$("mode-label").hidden = writable;
document.querySelectorAll("[data-view]").forEach((el) =>
  el.addEventListener("click", () => {
    if (!closeDetail()) return;
    state.view = el.dataset.view;
    location.hash = state.view;
    document
      .querySelectorAll("[data-view]")
      .forEach((n) => n.classList.toggle("active", n === el));
    load();
  }),
);
$("refresh").addEventListener("click", () => {
  state.pendingChange = null;
  state.revision = null;
  reconnectAttempt = 0;
  watchChanges();
  load();
});
$("focus").addEventListener("change", () => {
  if (!closeDetail()) {
    $("focus").value = state.focus;
    return;
  }
  state.focus = $("focus").value;
  const focus = focuses[state.focus];
  $("focus-hint").textContent = focus.hint;
  state.view = focus.view;
  document
    .querySelectorAll("[data-view]")
    .forEach((item) =>
      item.classList.toggle("active", item.dataset.view === state.view),
    );
  load();
});
$("playbook").addEventListener("click", () => {
  if (!closeDetail()) return;
  const body = modal($("focus").selectedOptions[0].textContent);
  if (!body) return;
  body.append(node("p", "", focuses[state.focus].task));
  body.append(
    node(
      "p",
      "metadata",
      "Your focus affects this session’s view. Connect an agent to discuss or write from your saved evidence.",
    ),
  );
  body.append(
    button("Go to notes", () => {
      closeDetail(true);
      state.view = "knowledge";
      location.hash = "knowledge";
      load();
    }),
  );
  body.append(
    technical(
      "Advanced workflow guide",
      button("Read the guide", () => source("docs/workflows.md"), "citation"),
    ),
  );
});
function closeDetail(force = false) {
  if (!force && reviewDirty()) {
    askDiscard(() => closeDetail(true));
    return false;
  }
  openPicker?.close();
  detailGeneration += 1;
  $("detail").close();
  document
    .querySelector(".shell")
    .classList.remove("inspector-open", "document-open");
  reviewingNote = null;
  detailReturn = null;
  detailSnapshot = null;
  if (detailOpener?.isConnected) detailOpener.focus();
  else document.querySelector("[data-view].active")?.focus();
  return true;
}
$("close-detail").addEventListener("click", () => {
  if (detailReturn) {
    const restore = detailReturn;
    detailReturn = null;
    restore();
  } else closeDetail();
});
$("detail").addEventListener("close", () => {
  if (!$("detail").open) {
    document
      .querySelector(".shell")
      .classList.remove("inspector-open", "document-open");
    reviewingNote = null;
    detailReturn = null;
  }
});
document.addEventListener("keydown", (event) => {
  if (
    event.key === "Escape" &&
    !event.defaultPrevented &&
    !["INPUT", "TEXTAREA", "SELECT"].includes(event.target.tagName) &&
    $("detail").open
  )
    $("close-detail").click();
  if (
    event.key === "/" &&
    !["INPUT", "TEXTAREA", "SELECT"].includes(document.activeElement.tagName)
  ) {
    const input = $("view").querySelector('input[type="search"]');
    if (input) {
      event.preventDefault();
      input.focus();
    }
  }
});
let appearance = "system";
try {
  appearance = localStorage.getItem("job-harness-theme") || "system";
} catch {}
function applyTheme() {
  if (appearance === "system")
    document.documentElement.removeAttribute("data-theme");
  else document.documentElement.dataset.theme = appearance;
  document.querySelectorAll("[data-theme]").forEach((button) => {
    if (button.tagName === "BUTTON")
      button.setAttribute(
        "aria-pressed",
        String(button.dataset.theme === appearance),
      );
  });
}
document.querySelectorAll(".theme-controls [data-theme]").forEach((button) =>
  button.addEventListener("click", () => {
    appearance = button.dataset.theme;
    applyTheme();
    try {
      localStorage.setItem("job-harness-theme", appearance);
    } catch {}
  }),
);
applyTheme();
document
  .querySelectorAll("[data-panel]")
  .forEach((button) =>
    button.addEventListener("click", () => openPanel(button.dataset.panel)),
  );
$("attention-button").addEventListener("click", () => openPanel("attention"));
window.addEventListener("hashchange", () => {
  const view = location.hash.slice(1);
  if (["pipeline", "knowledge"].includes(view) && view !== state.view) {
    if (!closeDetail()) {
      history.replaceState(null, "", "/#" + state.view);
      return;
    }
    state.view = view;
    load();
  }
});
load().then(() => {
  if (["attention", "sources", "activity", "connections"].includes(initial))
    openPanel(initial);
  if (initial.startsWith("note="))
    noteReview(decodeURIComponent(initial.slice(5)));
});

$("reload-session").addEventListener("click", () => location.reload());
$("apply-updates").addEventListener("click", async () => {
  if (!closeDetail()) return;
  $("update-notice").hidden = true;
  await load();
});
function interactionBusy() {
  return (
    $("detail").open ||
    state.dragging ||
    openPicker ||
    state.saving ||
    state.viewDirty ||
    ["INPUT", "TEXTAREA", "SELECT"].includes(document.activeElement?.tagName) ||
    !!$("view").querySelector("details[open] input, details[open] textarea")
  );
}
$("view").addEventListener("input", () => {
  state.viewDirty = true;
});
async function applyChanges(data) {
  const allowed = data.mode === "local_writes",
    modeChanged = allowed !== writable;
  if (modeChanged) {
    writable = allowed;
    $("mode-label").textContent = "Read only";
    $("mode-label").hidden = allowed;
  }
  if (!state.revision) {
    state.revision = data.revision;
    if (!modeChanged) return;
  }
  if (data.revision !== state.revision || modeChanged) {
    state.pendingChange = data;
    if (interactionBusy() || state.loading) {
      $("update-notice").hidden = false;
      if ($("detail").open)
        message(
          "Workspace changed. Your current review is preserved; refresh when ready.",
        );
    } else await flushChanges();
  }
  if (data.notifications === "manual") $("health").hidden = false;
  $("health").textContent = "Refresh for latest changes";
}
async function flushChanges() {
  if (state.changeApplying || state.loading || interactionBusy()) return;
  state.changeApplying = true;
  try {
    while (state.pendingChange && !state.loading && !interactionBusy()) {
      const data = state.pendingChange;
      state.pendingChange = null;
      const scroll = document.querySelector(".view-body").scrollTop;
      await load();
      document.querySelector(".view-body").scrollTop = scroll;
      state.revision = data.revision;
    }
  } finally {
    state.changeApplying = false;
    if (state.pendingChange) $("update-notice").hidden = false;
  }
}
let changeAbort = null,
  reconnectTimer = null,
  reconnectAttempt = 0;
async function watchChanges() {
  clearTimeout(reconnectTimer);
  changeAbort?.abort();
  if (document.hidden) return;
  const controller = new AbortController();
  changeAbort = controller;
  try {
    const response = await fetch("/api/changes", {
      headers: { "x-job-harness-csrf": csrf },
      credentials: "same-origin",
      cache: "no-store",
      signal: controller.signal,
    });
    if (!response.ok || !response.body)
      throw new Error("Live changes unavailable");
    const reader = response.body.getReader(),
      decoder = new TextDecoder();
    let pending = "";
    while (!controller.signal.aborted) {
      const { done, value } = await reader.read();
      if (done) throw new Error("Change stream ended");
      pending += decoder.decode(value, { stream: true });
      if (pending.length > 65536)
        throw new Error("Change notification too large");
      let newline;
      while ((newline = pending.indexOf("\n")) >= 0) {
        const line = pending.slice(0, newline);
        pending = pending.slice(newline + 1);
        if (!line.trim()) continue;
        const event = JSON.parse(line);
        if (event.ok && event.data) await applyChanges(event.data);
      }
    }
  } catch (error) {
    if (!controller.signal.aborted) {
      controller.abort();
      $("health").textContent = "Use Refresh for latest changes";
      if (!document.hidden && reconnectAttempt < 3)
        reconnectTimer = setTimeout(
          watchChanges,
          [500, 2000, 8000][reconnectAttempt++],
        );
    }
  }
}
watchChanges();
document.addEventListener("visibilitychange", () => {
  clearTimeout(reconnectTimer);
  if (document.hidden) changeAbort?.abort();
  else {
    reconnectAttempt = 0;
    watchChanges();
  }
});
window.addEventListener("beforeunload", (event) => {
  changeAbort?.abort();
  clearTimeout(reconnectTimer);
  if (reviewDirty() || state.viewDirty) {
    event.preventDefault();
    event.returnValue = "";
  }
});
