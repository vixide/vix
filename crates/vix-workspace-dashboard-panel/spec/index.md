# Workspace Dashboard Panel

Live workspace metrics for the dashboard overlay (Tools → Workspace Dashboard).

Pure state. The host fills these fields from background computations — disk
usage via `du`, a recursive file count, and the git commit count — and each
metric stays `None` until its computation finishes, so the panel can show a
"computing…" placeholder. The host owns the threads and rendering.
