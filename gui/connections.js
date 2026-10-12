"use strict";
(() => {
  let snapshot = null, snapshotKey = null, activeAction = null;
  const releases = "https://github.com/KingPsychopath/job-harness/releases";
  const labels = {
    available: ["Available", "good"], running: ["Running", "good"],
    not_found: ["Not installed", "neutral"], probe_failed: ["Check failed", "bad"],
    incompatible_version: ["Update required", "warn"],
    absent: ["Not installed", "neutral"], manifest_present: ["Files recorded · not verified", "warn"],
    manifest_mismatch: ["Cache needs review", "bad"], unreadable: ["Cache unreadable", "bad"],
    location_unknown: ["Cache not located", "neutral"], responding: ["Default provider responding", "good"],
    unreachable: ["Default provider not reachable", "neutral"],
    worker_unavailable: ["Provider worker unavailable", "bad"], unrecognized: ["Unrecognized local service", "warn"]
  };
  window.renderConnections = async function(helpers) {
    const {target, read, post, node, button, safeLink, formatBytes, message} = helpers;
    const shell = node("div", "connections-panel");
    target.append(shell);
    const heading = node("div", "connections-heading");
    heading.append(node("h2", "", "This device"), node("p", "metadata", "Your workspace works without an agent or local model."));
    shell.append(heading);
    const pending = node("p", "metadata", "Checking fixed local tools and cache records…");
    shell.append(pending);
    let data, device;
    try {
      data = await read("/api/connections");
      const key = data.workspace + ":" + data.version;
      if (!snapshot || snapshotKey !== key) {
        snapshot = await read("/api/device");
        snapshotKey = key;
      }
      device = snapshot;
    } catch (error) {
      if (shell.isConnected) pending.textContent = error.message;
      return;
    }
    if (!shell.isConnected) return;
    pending.remove();
    const status = (text, tone) => {
      const el = node("span", "connection-status status-" + tone);
      const dot = node("span", "connection-dot");
      dot.setAttribute("aria-hidden", "true");
      el.append(dot, node("span", "", text));
      return el;
    };
    const state = (value) => status(...(labels[value] || ["Not checked", "neutral"]));
    const section = (title) => {
      const group = node("section", "connections-section");
      group.append(node("h3", "connections-group-title", title));
      shell.append(group);
      return group;
    };
    const row = (group, title, description, indicator) => {
      const el = node("div", "connection-row");
      const copy = node("div", "connection-copy");
      copy.append(node("strong", "connection-name", title), node("p", "metadata", description));
      const controls = node("div", "connection-controls");
      if (indicator) controls.append(indicator);
      const actions = node("div", "connection-actions");
      controls.append(actions);
      el.append(copy, controls);
      group.append(el);
      return {el, copy, controls, actions};
    };
    const refresh = async () => {
      if (activeAction) return message("A local artifact action is still running.");
      try {
        snapshot = await read("/api/device");
        if (!shell.isConnected) return;
        target.replaceChildren();
        await window.renderConnections(helpers);
        message("Device check refreshed. Recorded files are separate from verified or running models.");
      } catch (error) { message(error.message); }
    };
    const headControls = node("div", "connection-actions");
    const refreshButton = button("Refresh checks", refresh);
    refreshButton.disabled = !!activeAction;
    headControls.append(refreshButton);
    heading.append(headControls);
    const workspace = section("Workspace");
    const core = row(workspace, "Job Harness", "Version " + data.version + " · " + device.platform, state("running"));
    core.copy.append(node("p", "connection-location", data.workspace));
    row(workspace, "Review controls", data.mode === "local_writes" ? "You can review and edit notes and application records in this session." : "Your records are read only. For local changes in the native window, turn off Workspace → Read only.", status(data.mode === "local_writes" ? "Review enabled" : "Read only", data.mode === "local_writes" ? "good" : "neutral"));
    const hosts = section("Your agent");
    const host = row(hosts, "Agent workspace tools (MCP)", (data.mcp?.focused_tools || 11) + " focused tools are available. Connect them to your existing agent.", status("Agent connection not checked", "neutral"));
    const byId = Object.fromEntries((device.tools || []).map((tool) => [tool.id, tool]));
    const cli = byId["job-harness"] || {state: "not_found"};
    if (cli.state === "available" && typeof cli.command_path === "string") {
      const configuration = JSON.stringify({
        mcpServers: {
          "job-harness": {command: cli.command_path, args: ["--root", data.workspace, "mcp", "--tool-profile", "focused"]}
        }
      }, null, 2);
      host.copy.append(node("p", "metadata", "Add this configuration in your agent’s settings. This app does not connect the agent automatically."));
      let field = null;
      const showConfiguration = (select) => {
        if (!field) {
          field = node("textarea", "connection-command");
          field.readOnly = true;
          field.rows = 9;
          field.value = configuration;
          field.setAttribute("aria-label", "MCP configuration JSON");
          host.copy.append(field);
          viewButton.textContent = "Hide configuration";
        }
        if (select) { field.focus(); field.select(); }
      };
      const viewButton = button("View configuration", () => {
        if (field) { field.remove(); field = null; viewButton.textContent = "View configuration"; }
        else showConfiguration(false);
      });
      const copyButton = button("Copy MCP configuration", async () => {
        try {
          if (!navigator.clipboard?.writeText) throw new Error("Native selection required");
          await navigator.clipboard.writeText(configuration);
          message("MCP configuration copied. Add it in your chosen agent host.");
        } catch {
          showConfiguration(true);
          message("Configuration selected. Use Copy in the native Edit menu, then add it in your agent host.");
        }
      });
      host.actions.append(copyButton, viewButton);
    } else {
      host.copy.append(node("p", "metadata", "A matching separate CLI is needed for an agent host to run these tools. Install it, then refresh checks."));
      host.actions.append(safeLink(releases + "/tag/v" + data.version, cli.state === "incompatible_version" ? "Get matching CLI" : "Get CLI"));
    }
    row(hosts, "Browser, mail and calendar", "Your agent manages sign-in and action permissions. This app has not checked those connections.", status("Agent managed", "neutral"));

    const tools = section("Local tools");
    const provider = byId["job-harness-local"] || {state: "not_found"};
    const providerRow = row(tools, "Optional model provider", provider.version ? "Native provider " + provider.version + ". Models and vendor runtimes are installed separately." : "Install the small native provider first, then review any model download.", state(provider.state));
    if (provider.state !== "available") providerRow.actions.append(safeLink(releases + "/tag/v" + data.version, "Get provider"));
    const typst = byId.typst || {state: "not_found"};
    const typstRow = row(tools, "Typst for CVs", typst.version ? "Typst " + typst.version + ". Compiles selected, approved CV content." : "Optional. An existing approved PDF works without Typst.", state(typst.state));
    if (typst.state !== "available") typstRow.actions.append(safeLink("https://github.com/typst/typst/releases", "Get Typst"));
    row(tools, "Local provider service", "Only 127.0.0.1:18081 was checked. Custom endpoints and loaded model identities are not checked here.", state(device.provider?.state || "unreachable"));

    const models = section("Optional local models");
    models.append(node("p", "metadata", "Recorded files still need verification. Verify checks the expected file checksums; downloading never starts a model."));
    const allowed = data.native_model_actions === true;
    const names = {"embeddinggemma2-text": ["Gemma", "Find related evidence even when the wording differs."], "laya-s256": ["Laya", "Suggest answers to specific questions for your review."], "kev-0.8b": ["Kev", "Optional decision model for Apple Silicon Macs."]};
    for (const model of device.models || []) {
      const [title, purpose] = names[model.id] || [model.name || model.id, ""];
      const advanced = model.id === "kev-0.8b";
      const cacheState = model.cache?.state || "location_unknown";
      const item = row(models, title, purpose + " " + formatBytes(model.download_bytes) + " of model files.", advanced ? status(cacheState === "manifest_present" ? "Advanced · files recorded" : "Advanced setup", cacheState === "manifest_present" ? "warn" : "neutral") : state(cacheState));
      if (model.runtime) item.copy.append(node("p", "metadata", "Model runtime: " + (labels[model.runtime.cache?.state]?.[0] || "Not checked") + ". Model running state is unknown."));
      if (advanced) item.copy.append(node("p", "metadata", model.hardware || "Separate native worker and reviewed runtime required."));
      if (!model.platform_runtime_listed && !advanced) item.copy.append(node("p", "metadata", "No supported local runtime is listed for this platform. You can still use a compatible remote service."));
      if (model.model_card) item.actions.append(safeLink(model.model_card, "Requirements"));
      const actionNote = node("p", "connection-action-note metadata");
      item.copy.append(actionNote);
      if (activeAction?.id === model.id) actionNote.textContent = activeAction.note;
      if (advanced || !model.gui_install_supported) continue;

      const verify = button("Verify files", async () => {
        if (activeAction) return message(activeAction.note);
        activeAction = {id: model.id, note: "Checking model and runtime files…"};
        verify.disabled = true; review.disabled = true;
        actionNote.textContent = activeAction.note;
        try {
          const result = await post("/api/models/verify", {id: model.id});
          actionNote.textContent = result.artifacts_verified === true ? "Model and runtime files verified at " + new Date(result.verified_at).toLocaleTimeString() + ". Model running state remains unknown." : "Verification did not confirm complete artifacts.";
          message(actionNote.textContent);
          if (result.artifacts_verified === true) item.controls.firstChild.replaceWith(status("Files verified just now", "good"));
          snapshot = null;
        } catch (error) { actionNote.textContent = error.message; message(error.message); }
        finally { activeAction = null; verify.disabled = !allowed || provider.state !== "available"; review.disabled = !allowed || provider.state !== "available"; }
      });
      const review = button(cacheState === "manifest_present" ? "Review repair" : "Review download", async () => {
        if (activeAction) return message(activeAction.note);
        review.disabled = true;
        actionNote.textContent = "Loading the download details…";
        try {
          const plan = await read("/api/models/plan?" + new URLSearchParams({id: model.id}));
          if (!item.el.isConnected) return;
          item.el.querySelector(".connection-plan")?.remove();
          const pane = node("div", "connection-plan");
          pane.append(node("h4", "", "Review " + title + " download"), node("p", "", formatBytes(plan.download_bytes) + " total, including its native runtime."), node("p", "metadata", plan.hardware || ""));
          for (const artifact of plan.items || []) {
            const detail = node("div", "connection-plan-item");
            detail.append(node("strong", "", artifact.kind === "runtime" ? "Native runtime · " + artifact.id : title + " model"), node("p", "metadata", formatBytes(artifact.download_bytes) + " · License: " + artifact.license), safeLink(artifact.source, "Publisher source (fixed version)"), node("p", "connection-digest", "SHA256 " + artifact.sha256));
            pane.append(detail);
          }
          pane.append(node("p", "metadata", "These files go into your local model cache. No credentials are read and no model or service starts. Interrupted downloads may leave partial cache files."));
          const consent = node("label", "download-confirm");
          const checkbox = node("input"); checkbox.type = "checkbox";
          consent.append(checkbox, node("span", "", "I reviewed these sources, licenses, requirements and download size."));
          const install = button("Download reviewed files", async () => {
            if (!checkbox.checked || activeAction) return;
            activeAction = {id: model.id, note: "Downloading and checking the reviewed runtime and model. This may take several minutes…"};
            install.disabled = true; checkbox.disabled = true; verify.disabled = true; review.disabled = true;
            actionNote.textContent = activeAction.note;
            try {
              const result = await post("/api/models/install", {id: model.id, expected_plan_sha256: plan.plan_sha256});
              if (result.status !== "installed_verified") {
                actionNote.textContent = "Installation is incomplete. " + (result.next_action || "Refresh checks and verify files before explicitly retrying.");
              } else {
                actionNote.textContent = "Downloaded and verified exact pinned files. The model has not been started.";
                item.controls.firstChild.replaceWith(status("Files verified just now", "good"));
                pane.remove();
              }
              snapshot = null;
              message(actionNote.textContent);
            } catch (error) { actionNote.textContent = error.message; snapshot = null; message(error.message); }
            finally { activeAction = null; checkbox.disabled = false; verify.disabled = !allowed || provider.state !== "available"; review.disabled = !allowed || provider.state !== "available"; install.disabled = !checkbox.checked || !allowed; }
          }, "primary");
          install.disabled = true;
          checkbox.addEventListener("change", () => { install.disabled = !checkbox.checked || !allowed || !!activeAction; });
          const close = button("Cancel", () => { if (!activeAction) pane.remove(); });
          pane.append(consent, node("div", "connection-actions"));
          pane.lastChild.append(install, close);
          item.el.append(pane);
          actionNote.textContent = "";
        } catch (error) { actionNote.textContent = error.message; message(error.message); }
        finally { review.disabled = !allowed || !!activeAction; }
      });
      verify.disabled = !allowed || !!activeAction || provider.state !== "available" || cacheState !== "manifest_present";
      review.disabled = !allowed || !!activeAction || provider.state !== "available";
      if (!allowed) { verify.title = review.title = "Use the native window with Workspace → Read only turned off to run artifact actions."; }
      item.actions.append(verify, review);
    }
    const checked = new Date(device.checked_at);
    shell.append(node("p", "connection-footer metadata", "Checked " + (Number.isNaN(checked.getTime()) ? "on request" : checked.toLocaleTimeString()) + ". Default cache and address only. No periodic device scans."));
    message(activeAction ? activeAction.note : "Device readiness checked. Agent sessions and individual model execution remain separate.");
  };
})();
