## Web Search
- Always prefer the SearXNG MCP tool (`searxng_web_search`) over the built-in `WebSearch` tool
- Use `web_url_read` from SearXNG MCP for reading web page content

## Project Skills
- For this project, always apply skills from `~/.agents/skills` when doing code work:
  - `karpathy-guidelines`
  - `clean-code`
  - `rust/weldsorm.md`
  - `rust-best-practices`
  - `rust-async-patterns`
- For `rust-best-practices`, read relevant reference chapters in the same turn when the work needs deeper Rust design, review, testing, error-handling, performance, or documentation guidance.
- For `rust-async-patterns`, read `references/details.md` when async/Tokio/channel/task/cancellation details matter beyond the quick rules in `SKILL.md`.

## Plan Execution
- When executing an implementation plan, prefer using the `caveman` skill to keep coordination concise.
- If parts of the plan can be completed independently, execute them in parallel subagents.
- If parts of the plan depend on each other, execute them in sequential subagents, preserving dependency order.

## Database Access
- Use Welds for database models, repositories, queries, inserts, updates, deletes, and migrations by default.
- Do not add raw SQL for application data access when Welds can express the operation clearly.
- Raw SQL is allowed only when Welds cannot reasonably express the query or when it is needed for a backend-specific operation; document the reason at the call site.

## Project Knowledge
- `docs/solutions/` contains documented solutions to past problems and durable patterns, organized by category with YAML frontmatter (`module`, `problem_type`, `tags`). Relevant when implementing or debugging in documented areas.
- `CONCEPTS.md` contains shared project vocabulary for domain concepts and named processes.
