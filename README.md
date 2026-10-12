<div align="center">
  <img src="docs/assets/icon.svg" width="72" height="72" alt="Job Harness">
  <h1>Job Harness</h1>
  <p><strong>Your career workspace. Any capable agent.</strong><br>Keep your history. Change your agent. Carry on.</p>
  <p>
    <img alt="Local first" src="https://img.shields.io/badge/local-first-4965ae">
    <img alt="Rust core" src="https://img.shields.io/badge/core-Rust-384655">
    <img alt="Private preview" src="https://img.shields.io/badge/status-private_preview-66717b">
    <a href="LICENSE"><img alt="MIT license" src="https://img.shields.io/badge/license-MIT-384655"></a>
  </p>
  <p><a href="#start-once">Get started</a> · <a href="examples/README.md">Try the demo</a> · <a href="docs/AGENTS.md">Connect an agent</a> · <a href="#documentation">Documentation</a></p>
</div>

![Your workspace, any capable agent. Shared tools and private career history stay separate.](docs/assets/hero.svg)

Job Harness keeps the evidence behind your job search together: answers, project stories, interviews, CVs and application history. Give your agent the tools, then ask for the work you want done.

## Ask your agent

**Tidy the inbox**

> Review my job emails. Archive resolved receipts and rejections, bring unanswered requests forward, and update my tracker.

**Apply to roles**

> Apply to 20 suitable saved roles within my stated preferences. Use my confirmed answers and primary CV, check duplicates and record verified outcomes. Ask only for missing facts or required confirmations.

**Prepare for an interview**

> Prepare a screening brief for this company and vacancy. Connect two supported project stories, likely questions and useful questions to ask them.

**Keep useful notes**

> Save these interview notes, link them to the company and show me proposed updates with their sources before accepting anything.

**Find the right story**

> Find my strongest example of improving a slow system. Show the original evidence and help me turn it into a clear interview answer.

Your agent brings its own browser, email and calendar connections. The workspace retains sourced context and checked outcomes across sessions. [Everyday workflows](resources/guides/workflows.md) · [Access and permissions](docs/AUTH.md)

## See where things stand

The optional desktop app uses the same workspace as your agent. Move a card, review a note or inspect its original source; your agent sees the change next time.

![Job Harness 0.2.0 on macOS, shown with fictional applications.](docs/assets/dashboard.png)

[Preview Notes & evidence](docs/assets/note-review.png) · [Explore the fictional demo](examples/README.md) · [View a sample CV](examples/cv/example.pdf)

## Start once

[Download the private preview](https://github.com/KingPsychopath/job-harness/releases/tag/v0.2.0) or [install from source](docs/INSTALL.md), then create your separate private workspace:

```sh
jh setup --workspace "$HOME/Career/workspace"
```

Connect your agent, add your existing CV and confirmed answers, and start with one of the prompts above. Desktop, models and CV generation are optional. [Setup guide](resources/guides/job-harness-onboarding.md) · [Agent connection guide](docs/AGENTS.md)

**Private preview.** macOS, Linux and experimental Windows builds are available. Windows packages were cross-built and have not been run on Windows; a known notification stall is tracked in [issue 1](https://github.com/KingPsychopath/job-harness/issues/1). Packages are unsigned. [Platform requirements and checksums](docs/INSTALL.md) · [Validation record](docs/VALIDATION.md)

## Documentation

| Use your workspace | Build with the toolkit |
|---|---|
| [Notes and evidence](docs/INTAKE.md) · [CVs](docs/CV.md) · [Documents](docs/DOCUMENTS.md) | [Architecture](docs/ARCHITECTURE.md) · [Tool contracts](resources/guides/job-harness-contract.md) |
| [Backup and restore](docs/BUNDLES.md) · [Optional models](docs/MODELS.md) | [Contributing](CONTRIBUTING.md) · [Development](docs/DEVELOPMENT.md) · [Roadmap](docs/ROADMAP.md) |

The reusable software contains no candidate profile or credentials. Your private data stays in a separate, portable workspace. MIT software; [third-party components](docs/THIRD_PARTY.md) keep their own licenses.
