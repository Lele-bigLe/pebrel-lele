# Codex turn change viewing

The terminal footer captures one local Codex native thread identity. Its dialog
owns turn pagination, selected-turn item pagination and frozen patch views. It
only calls `thread/turns/list` and `thread/items/list`; reading never resumes a
thread, takes its writer lock or starts a model turn. WSL/SSH are not mapped to
the host's session store.

`ai_hook::integrations::codex_rpc` owns executable discovery, the bounded JSONL
transport and the short-lived child process. Quota operations retain their
existing startup overrides and errors through their adapter. History uses the
configured provider and a bounded larger response allowance. One operation has
a 25-second deadline; pagination can continue explicitly. Closing a dialog
drops its UI task; any already-started background read remains deadline-bound.

Native completed `fileChange` items are the source of edits. Shell/external writes
without native patches are not attributed from the current Git working tree.
Files with repeated edits keep their chronological steps, rather than treating
concatenated intermediate patches as a net diff. Add/delete native content is
converted to hunks for the shared renderer. Missing context stays omitted.

Historical diffs reuse the existing comparison UI but have an explicit read-only
source flag and no live Git/review snapshot token. Live reload, staging and
review entry points reject historical views. Later working-tree changes cannot
replace their captured patch. No undo or extension hosting is introduced.
