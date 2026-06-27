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

## Project Knowledge
- `docs/solutions/` contains documented solutions to past problems and durable patterns, organized by category with YAML frontmatter (`module`, `problem_type`, `tags`). Relevant when implementing or debugging in documented areas.
- `CONCEPTS.md` contains shared project vocabulary for domain concepts and named processes.
