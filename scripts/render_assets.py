"""Original repo-native SVGs. No screenshots, private data or external assets."""

from pathlib import Path
from xml.sax.saxutils import escape

ROOT = Path(__file__).resolve().parents[1]
ASSETS = ROOT / "docs/assets"
STYLE = """<style>text{font-family:Inter,Arial,sans-serif;fill:#1e3041}.muted{fill:#586b79}.label{font-size:16px;font-weight:700;letter-spacing:2px}.body{font-size:21px}.title{font-size:32px;font-weight:700}.mono{font-family:Menlo,monospace}</style>"""


def text(x, y, value, size=22, cls="", fill=None):
    color = f' fill="{fill}"' if fill else ""
    return f'<text x="{x}" y="{y}" font-size="{size}" class="{cls}"{color}>{escape(value)}</text>'


def frame(width, height, title, body):
    return f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" role="img" aria-labelledby="title"><title id="title">{escape(title)}</title>{STYLE}<rect width="{width}" height="{height}" rx="24" fill="#f6f3ed"/>{body}</svg>\n'


def card(x, y, w, h, color):
    return (
        f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="20" fill="{color}" stroke="#d3dbdc"/>'
    )


def render():
    ASSETS.mkdir(parents=True, exist_ok=True)
    icon = '<rect width="96" height="96" rx="22" fill="#253a46"/><path d="M22 30h52v42H22zM38 30v-9h20v9" fill="none" stroke="#8be0cd" stroke-width="5" stroke-linejoin="round"/><path d="m39 44-7 7 7 7m18-14 7 7-7 7" fill="none" stroke="#fffaf0" stroke-width="4" stroke-linecap="round"/>'
    (ASSETS / "icon.svg").write_text(
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 96" role="img"><title>Job Harness briefcase and code mark</title>{icon}</svg>\n'
    )
    body = (
        text(64, 83, "JOB HARNESS", 17, "label")
        + text(64, 164, "Your workspace.", 64)
        + text(64, 241, "Any capable agent.", 64)
    )
    body += text(66, 303, "A durable job-search history.", 25, "muted") + text(
        66, 343, "Dependable tools. Your own connections.", 25, "muted"
    )
    body += card(66, 403, 385, 62, "#e1ebe5") + text(92, 442, "Context · Commands · Workspace", 20)
    body += (
        card(748, 78, 545, 348, "#fffdfa")
        + text(786, 122, "SHARED TOOLING", 15, "label")
        + text(786, 173, "Job Harness", 35, "title")
    )
    body += text(786, 220, "Typed tools · Batches · Recovery", 22, "muted")
    body += '<path d="M782 246h478" stroke="#d9e1df"/>'
    body += (
        text(786, 285, "YOUR PRIVATE WORKSPACE", 15, "label")
        + text(786, 327, "Facts  /  CV  /  Applications", 25)
        + text(786, 372, "Answers and receipts stay with you.", 21, "muted")
    )
    body += text(750, 472, "No candidate data in the code package.", 20, "muted")
    (ASSETS / "hero.svg").write_text(
        frame(1360, 530, "Job Harness: reusable tools and a separate private workspace", body)
    )
    body = text(60, 60, "A workspace is durable. Execution is replaceable.", 34, "title")
    body += (
        card(464, 100, 560, 133, "#ece6f6")
        + text(490, 139, "AGENT OR FUTURE GUI", 16, "label")
        + text(490, 181, "Decisions, context and presentation", 26)
        + text(490, 216, "CLI · MCP · Python API", 18, "muted")
    )
    body += (
        card(60, 327, 320, 283, "#e1ebe5")
        + text(86, 369, "PRIVATE WORKSPACE", 16, "label")
        + text(86, 415, "Your facts and history", 24)
        + text(86, 466, "context/   evidence", 20, "mono")
        + text(86, 505, "content/   approved CV", 20, "mono")
        + text(86, 544, "applications/   outcomes", 18, "mono")
        + text(86, 582, "config/   personal settings", 18, "mono")
    )
    body += (
        card(464, 327, 560, 283, "#fffdfa")
        + text(490, 369, "REUSABLE ENGINE", 16, "label")
        + text(490, 415, "Job Harness", 31, "title")
        + text(490, 464, "Schemas and focused retrieval", 23)
        + text(490, 504, "Batches and duplicate checks", 23)
        + text(490, 544, "Packets, one-submit guards, recovery", 23)
        + text(490, 584, "Generic skills are installed resources.", 18, "muted")
    )
    body += (
        card(1108, 327, 432, 283, "#f1e5da")
        + text(1134, 369, "HOST CONNECTORS &amp; ACCESS".replace("&amp;", "&"), 15, "label")
        + text(1134, 415, "Browser · Mail · Calendar", 25)
        + text(1134, 466, "Existing authenticated sessions", 21)
        + text(1134, 507, "Current user scope and policy", 21)
        + text(1134, 556, "Credentials stay in the host.", 21)
        + text(1134, 589, "Separate from facts and bundles.", 18, "muted")
    )
    body += '<defs><marker id="arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto-start-reverse"><path d="M0 0 8 4 0 8" fill="none" stroke="#47796f" stroke-width="1.5"/></marker></defs>'
    body += '<path d="M744 233v94M380 444h84M1108 520h-84M1024 165h300v162" fill="none" stroke="#47796f" stroke-width="2.5" marker-end="url(#arrow)"/>'
    body += (
        text(766, 281, "typed calls", 17, "muted")
        + text(388, 423, "files", 16, "muted")
        + text(1035, 498, "evidence", 14, "muted")
        + text(1160, 144, "authorized actions", 17, "muted")
    )
    body += text(60, 669, "One canonical workspace per person.", 25) + text(
        60,
        707,
        "Local functions today; choose extra isolation or hosted compute only when the workload needs it.",
        20,
        "muted",
    )
    (ASSETS / "architecture.svg").write_text(
        frame(
            1600,
            755,
            "Four layers: private workspace, reusable engine, agent or GUI, and host connectors",
            body,
        )
    )


if __name__ == "__main__":
    render()
