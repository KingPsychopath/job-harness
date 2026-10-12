"use strict";
window.JobHarnessDailyReview = (() => {
  const dailyRequest =
    "Check my job emails since the last completed review. Show unanswered requests and upcoming interviews, update exact application records from observed evidence, and propose application confirmations and rejections to archive. Wait for my selected messages before changing mail labels. Do not apply to jobs, send replies or book meetings. Keep the reviewed mail snapshot and its coverage in Daily review.";
  const actionLabels = {
    queued: "Ready for your agent",
    started: "Archive started — outcome unverified",
    archived: "Archived — observed result recorded",
    failed: "Archive failed",
    withdrawn: "Archive approval withdrawn",
    outcome_unknown: "Check the mail outcome before retrying",
  };
  function attentionItems(data) {
    return Array.isArray(data.attention?.items) ? data.attention.items : [];
  }
  async function render(target, helpers, options = {}) {
    const {
      node, button, read, post, safeLink, source, noteReview, packetPreview,
      companyDetail, empty, message, label, payloadKey,
    } = helpers;
    const query = new URLSearchParams({ offset: String(options.offset || 0) });
    const data = await read("/api/daily?" + query);
    if (!target.isConnected || (options.isCurrent && !options.isCurrent())) return;
    options.onUpdate?.(data);
    const overview = node("div", "daily-overview");
    const observed = data.observations || {};
    const sources = Array.isArray(observed.sources) ? observed.sources :
      ["saved", "supplied"].includes(observed.origin) ? [observed] : [];
    const supplied = sources.length > 0;
    overview.append(node("h3", "daily-mail-state", supplied ? "Saved mail review" : "Mail not checked"));
    if (supplied) {
      for (const saved of sources) {
        const sourceRow = node("div", "daily-source");
        if (sources.length > 1) sourceRow.append(node("div", "daily-item-title",
          [saved.provider ? label(saved.provider) : "Mail", saved.mailbox_id].filter(Boolean).join(" · ")));
        if (saved.observed_at) sourceRow.append(node("p", "metadata", "Observed " + saved.observed_at));
        const coverage = saved.coverage?.coverage || saved.coverage;
        if (coverage?.requested_period) sourceRow.append(node("p", "metadata", coverage.requested_period));
        if (coverage) {
          const complete = coverage.pagination_exhausted === true &&
            Array.isArray(coverage.failed_reads) && coverage.failed_reads.length === 0;
          sourceRow.append(node("p", "metadata", complete ? "Requested search completed." :
            "Partial review. Unchecked pages or failed reads may still contain requests."));
        } else sourceRow.append(node("p", "metadata", "Search coverage was not supplied."));
        if (saved.source_state && !["current", "valid", "unchanged"].includes(saved.source_state))
          sourceRow.append(node("p", "notice", "Its source has changed or is unavailable. Ask your agent to refresh this review."));
        overview.append(sourceRow);
      }
      if (sources.length > 1 && observed.all_sources_complete === false)
        overview.append(node("p", "metadata", "Some accounts have an incomplete recorded search. This is not a complete mailbox review."));
      if (observed.sources_truncated)
        overview.append(node("p", "metadata", "Additional account summaries are not shown here. Ask your agent for the remaining recorded coverage."));
      overview.append(node("p", "metadata", "Ask your agent to check for newer mail through its mail connection."));
    } else {
      overview.append(node("p", "metadata", "Ask your connected agent to check your mailbox. This app shows the results it records; it has no mail connection of its own."));
    }
    const copyRequest = (text, success) => button("Copy request for your agent", async () => {
      try {
        await navigator.clipboard.writeText(text);
        message(success || "Request copied. Paste it into your connected agent.");
      } catch {
        const textarea = node("textarea", "daily-request");
        textarea.readOnly = true;
        textarea.value = text;
        textarea.setAttribute("aria-label", "Request for your connected agent");
        overview.append(textarea);
        textarea.focus();
        textarea.select();
        message("Select and copy this request into your connected agent.");
      }
    }, "citation");
    overview.append(copyRequest(dailyRequest));
    target.append(overview);

    const communication = Array.isArray(data.communications) ? data.communications : [];
    const plan = data.archive_plan;
    const archiveCommunication = communication.filter((item) =>
      item.provider === plan?.provider && item.mailbox_id === plan?.mailbox_id);
    const proposedMessageIds = new Set((plan?.actions || []).flatMap((action) => action.message_ids || []));
    const local = attentionItems(data).filter((item) => item.kind !== "supplied_mail");
    const replies = communication.filter((item) => item.needs_reply === true);
    const uncertain = communication.filter((item) => item.needs_reply !== true && (item.needs_reply == null || item.category === "unknown"));
    const information = communication.filter((item) => item.needs_reply === false && item.category !== "unknown" &&
      !(archiveCommunication.includes(item) && proposedMessageIds.has(item.message_id)));
    const linkedActions = (item) => {
      const actions = node("div", "related");
      if (item.thread_url) actions.append(safeLink(item.thread_url, "Open email"));
      const path = item.record_path || item.record;
      if (path) actions.append(button("Application", () => companyDetail({
        company: item.company, title: item.title || item.company,
        record_path: path, stage: item.stage,
      }), "citation"));
      else if (item.company) actions.append(node("span", "metadata", "Application not assigned"));
      if (item.note_id) actions.append(button("Review note", () => noteReview(item.note_id, item.proposal_id), "citation"));
      if (item.packet_id) actions.append(button("Materials", () => packetPreview(item.packet_id), "citation"));
      if (item.source_path) actions.append(button("Recorded source", () => source(item.source_path), "citation"));
      return actions;
    };
    const group = (title, items, renderItem) => {
      if (!items.length) return;
      const section = node("section", "daily-group");
      const heading = node("h3", "", title);
      heading.append(node("span", "daily-group-count", items.length + " shown"));
      section.append(heading);
      for (const item of items) section.append(renderItem(item));
      target.append(section);
    };
    const row = (item) => {
      const article = node("article", "daily-item");
      article.append(node("div", "daily-item-title", item.subject || item.title || item.company ||
        (item.note_id ? "Note awaiting review" : label(item.kind || item.category))));
      const identity = [typeof item.sender === "string" ? item.sender : "", item.company].filter(Boolean);
      if (identity.length && item.subject) article.append(node("p", "metadata", identity.join(" · ")));
      const summary = item.summary || item.next_action || item.message || item.status_detail;
      if (summary) article.append(node("p", "", summary));
      if (item.scheduled_start) article.append(node("p", "metadata", item.scheduled_start));
      else if (item.received_at) article.append(node("p", "metadata", "Received " + item.received_at));
      else if (item.observed_at) article.append(node("p", "metadata", "Observed " + item.observed_at));
      article.append(linkedActions(item));
      return article;
    };
    group("Needs a reply", replies, row);
    group("Upcoming conversations", local.filter((item) => item.kind === "interview" && item.temporal_state === "upcoming"), row);
    group("Dates to confirm", local.filter((item) => item.kind === "interview" && !["upcoming", "past"].includes(item.temporal_state)), row);
    group("Check the outcome", local.filter((item) => item.kind === "unknown_submission"), row);
    group("Mail to review", uncertain, row);

    const archiveActions = Array.isArray(data.archive_actions) ? data.archive_actions : [];
    const statuses = new Map(archiveActions.map((action) => [action.action_id, action]));
    for (const action of plan?.actions || []) {
      if (action.archive_state?.status) statuses.set(action.action_id, action.archive_state);
    }
    const visibleIds = new Set(archiveCommunication.map((item) => item.message_id));
    const visibleActions = (plan?.actions || []).filter((action) =>
      Array.isArray(action.message_ids) && action.message_ids.length > 0 && !action.requires_paged_review &&
      action.message_ids.every((id) => visibleIds.has(id)));
    let currentArchiveDraft = null;
    const keepButton = (action) => {
      const hash = action.plan_sha256 || (action.plan_id === plan?.plan_id ? plan.plan_sha256 : null);
      if (!options.writable || action.status !== "queued" || !hash) return null;
      const keyFor = payloadKey();
      return button(action.target_scope === "thread" ? "Keep thread in inbox" : "Keep in inbox", async () => {
        const payload = {
          plan_id: action.plan_id, expected_plan_sha256: hash, selected_action_ids: [action.action_id],
          approved_by: "local-user", approval_evidence: "The local user clicked Keep in inbox for this queued archive approval.",
          reason: "Local user withdrew this queued selection",
        };
        const draft = currentArchiveDraft?.();
        await post("/api/daily/withdraw-archive", { ...payload, idempotency_key: keyFor(payload) });
        const content = target.closest(".detail-content");
        if (content) delete content.dataset.dirty;
        target.replaceChildren();
        await render(target, helpers, { ...options, archiveDraft: draft });
        message("Archive approval withdrawn. This cancels the queued instruction; it performs no mailbox action.");
      }, "citation");
    };
    if (visibleActions.length) {
      const section = node("section", "daily-group");
      section.append(node("h3", "", "Archive suggestions"));
      section.append(node("p", "metadata", "Select the messages you have reviewed. Confirmation queues them for your connected agent; it does not change your mailbox."));
      const draft = options.archiveDraft;
      const restored = draft?.plan_id === plan.plan_id && draft?.plan_sha256 === plan.plan_sha256 && observed.source_state === "unchanged";
      const selectable = (action) => !statuses.has(action.action_id) || ["failed", "withdrawn"].includes(statuses.get(action.action_id).status);
      const selected = new Set(restored ? draft.selected_action_ids.filter((id) =>
        visibleActions.some((action) => action.action_id === id && selectable(action))) : []);
      const confirmation = node("label", "choice");
      const reviewed = node("input");
      reviewed.type = "checkbox";
      reviewed.checked = !!(restored && draft.reviewed);
      currentArchiveDraft = () => ({ plan_id: plan.plan_id, plan_sha256: plan.plan_sha256,
        selected_action_ids: [...selected], reviewed: reviewed.checked });
      confirmation.append(reviewed, node("span", "", "I reviewed the selected messages and want them archived."));
      const keyFor = payloadKey();
      const approve = button("Confirm selected for archiving", async () => {
        if (!selected.size || !reviewed.checked)
          return message("Select messages and confirm your review first.");
        const selection = [...selected].sort();
        const payload = {
          plan_id: plan.plan_id,
          expected_plan_sha256: plan.plan_sha256,
          selected_action_ids: selection,
          approved_by: "local-user",
          approval_evidence: "The local user selected these archive suggestions and explicitly confirmed having reviewed them in Daily review.",
        };
        await post("/api/daily/approve-archive", { ...payload, idempotency_key: keyFor(payload) });
        const content = target.closest(".detail-content");
        if (content) delete content.dataset.dirty;
        target.replaceChildren();
        await render(target, helpers, options);
        message("Selected messages queued. Your connected agent must recheck the latest threads and verify the archive result.");
      }, "primary");
      const updateSelection = () => {
        const messages = new Set(visibleActions.filter((action) => selected.has(action.action_id)).flatMap((action) => action.message_ids));
        approve.textContent = "Confirm " + messages.size + " messages for archiving";
        approve.disabled = !options.writable || observed.source_state !== "unchanged" || !selected.size || !reviewed.checked;
      };
      for (const action of visibleActions) {
        const matching = archiveCommunication.filter((item) => action.message_ids.includes(item.message_id));
        const article = node("article", "daily-item");
        const choice = node("label", "daily-archive-choice");
        const checkbox = node("input");
        checkbox.type = "checkbox";
        const existing = statuses.get(action.action_id);
        checkbox.disabled = !options.writable || observed.source_state !== "unchanged" || !selectable(action);
        checkbox.checked = selected.has(action.action_id);
        const copy = node("div");
        copy.append(node("div", "daily-item-title", matching.map((item) => item.subject).filter(Boolean).join("; ") ||
          label(action.category) + (action.message_ids.length > 1 ? " · " + action.message_ids.length + " messages" : "")));
        copy.append(node("p", "metadata", action.reason || "Suggested archive after review"));
        for (const item of matching) {
          const details = [matching.length > 1 ? item.subject : "", typeof item.sender === "string" ? item.sender : "",
            item.received_at ? "Received " + item.received_at : item.observed_at ? "Observed " + item.observed_at : ""].filter(Boolean);
          if (details.length) copy.append(node("p", "metadata", details.join(" · ")));
        }
        if (action.target_scope === "thread")
          copy.append(node("p", "metadata", "This mail service archives the complete thread: " + action.message_ids.length + " messages."));
        if (existing) copy.append(node("p", "daily-mail-state", actionLabels[existing.status] || "Archive result needs review"));
        if (existing?.label_changes) copy.append(node("p", "notice", "Other mail labels also changed. Review the recorded result before restoring anything."));
        choice.append(checkbox, copy);
        article.append(choice);
        if (matching[0]) article.append(linkedActions(matching[0]));
        const keep = existing && keepButton(existing);
        if (keep) article.append(keep, node("p", "metadata", "Cancels this queued approval without changing mail labels."));
        checkbox.addEventListener("change", () => {
          if (checkbox.checked) selected.add(action.action_id);
          else selected.delete(action.action_id);
          updateSelection();
        });
        section.append(article);
      }
      if (options.writable && observed.source_state === "unchanged" && visibleActions.some(selectable)) {
        const form = node("div", "daily-approval");
        form.append(confirmation, approve);
        reviewed.addEventListener("change", updateSelection);
        updateSelection();
        section.append(form);
      } else if (!options.writable) {
        section.append(node("p", "metadata", "Read only. In the desktop app, turn off Workspace → Read only to confirm a selection."));
      } else if (observed.source_state !== "unchanged") {
        section.append(node("p", "metadata", "Ask your agent to refresh the changed mail snapshot before confirming an archive selection."));
      }
      if (archiveActions.some((action) => ["queued", "started", "outcome_unknown"].includes(action.status)))
        section.append(copyRequest("Complete only the human-approved archive actions for Daily review plan " + plan.plan_id +
          ". Re-read each latest thread first, retain new requests, use the one-action guard and record verified resulting labels. Resolve any unknown outcome before retrying. Do not apply to jobs, send replies or book meetings."));
      target.append(section);
    }
    if ((plan?.actions_page?.total ?? plan?.actions?.length ?? 0) > visibleActions.length)
      target.append(node("p", "metadata", "Archive suggestions are shown only when all selected messages appear on this page. Review more messages below, or ask your agent to present a complete thread."));
    group("Archive work", archiveActions.filter((action) => !visibleActions.some((visible) => visible.action_id === action.action_id)), (action) => {
      const article = node("article", "daily-item");
      const original = communication.find((item) => item.provider === action.provider &&
        item.mailbox_id === action.mailbox_id && action.message_ids?.includes(item.message_id));
      article.append(node("div", "daily-item-title", original?.subject || label(action.category || "Archive work")));
      article.append(node("p", "daily-mail-state", actionLabels[action.status] || "Archive result needs review"));
      if (action.label_changes) article.append(node("p", "notice", "Other mail labels also changed. Review the recorded result before restoring anything."));
      if (original) article.append(linkedActions(original));
      const keep = keepButton(action);
      if (keep) article.append(keep, node("p", "metadata", "Cancels this queued approval without changing mail labels."));
      if (action.result_sha256) article.append(node("p", "metadata", "A recorded result is available for your agent to review."));
      for (const reference of (Array.isArray(action.evidence) ? action.evidence : []).slice(0, 3)) {
        if (typeof reference !== "string") continue;
        if (/^https?:\/\//.test(reference)) article.append(safeLink(reference, "Archive evidence"));
        else article.append(button("Archive evidence", () => source(reference), "citation"));
      }
      article.append(copyRequest("Review Daily review archive action " + action.action_id +
        ". Inspect its preserved plan and current mailbox labels. Do not blindly retry a started or unknown action. Do not apply to jobs, reply or book meetings."));
      return article;
    });
    group("Notes to review", local.filter((item) => item.kind === "pending_note_review"), row);
    group("Recorded conversations", local.filter((item) => item.kind === "interview" && item.temporal_state === "past"), row);
    group("Application preparation", local.filter((item) => !["interview", "unknown_submission", "pending_note_review", "mail_archive_action"].includes(item.kind)), row);
    group("Recorded updates", information, row);
    if (!communication.length && !local.length && !(plan?.actions?.length))
      target.append(empty("No recorded items need review. A connected agent can check your mail for anything new."));
    const paging = node("div", "paging");
    if (options.offset) paging.append(button("Previous", () => options.onPage(Math.max(0, options.offset - 50))));
    const next = [data.page?.next_offset, plan?.actions_page?.next_offset,
      data.archive_actions_page?.next_offset, data.attention?.page?.next_offset].find((value) => value != null);
    if (next != null) paging.append(button("More review items", () => options.onPage(next)));
    target.append(paging);
    message(supplied ? "Showing saved mail observations and local next steps." : "Local next steps shown. Mail has not been checked here.");
    return data;
  }
  async function renderCompany(target, helpers, record) {
    const { read, node, button, safeLink, empty } = helpers;
    const data = await read("/api/daily?" + new URLSearchParams({ record }));
    if (!target.isConnected) return;
    const section = node("section", "daily-group");
    section.append(node("h3", "", "Communications"));
    const rows = Array.isArray(data.communications) ? data.communications : [];
    for (const item of rows) {
      const article = node("article", "daily-item");
      article.append(node("div", "daily-item-title", item.subject || "Recorded message"));
      if (item.summary) article.append(node("p", "", item.summary));
      if (typeof item.sender === "string") article.append(node("p", "metadata", item.sender));
      if (item.received_at) article.append(node("p", "metadata", "Received " + item.received_at));
      else if (item.observed_at) article.append(node("p", "metadata", "Observed " + item.observed_at));
      if (item.needs_reply === true) article.append(node("p", "daily-mail-state", "Needs a reply"));
      if (item.thread_url) article.append(safeLink(item.thread_url, "Open email"));
      section.append(article);
    }
    if (!rows.length) section.append(empty("No mail observations are linked to this application."));
    if (data.page?.has_more || data.page?.next_offset != null) section.append(node("p", "metadata", "More recorded messages are available in Daily review."));
    if (rows.length) section.append(button("Open Daily review", helpers.openDaily, "citation"));
    target.append(section);
  }
  return { render, renderCompany, actionLabels, attentionItems };
})();
