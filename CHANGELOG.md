# Changelog - blender-mcp-rs

## [1.9.0] - 2026-08-08

### Added
- docs/verification.md: test layers, quality gates, release-build
  verification, the live acceptance criteria, and badge verification.
- CHANGELOG.md for this repository.

### Changed
- Skill moved to the standard skills/blender-mcp/SKILL.md location and its
  frontmatter extended with argument-hint, when-to-use, and
  user-invocable. The registration path in the skill was corrected to the
  installed binary (~/.local/bin/blender-mcp-rs) and the test count
  updated to 50.
- README: Documentation section expanded (setup + verification guides),
  skill path references updated to skills/.
- docs/setup.md: version reference updated to 1.9.0, a skill install step
  added, and a pointer to docs/verification.md added.

## [1.8.0] - 2026-08-05

- Standard repo layout applied (badge README, embedded build/, controlled
  setup guide, shipped skill).
- 50 tests: bbox unit tests, mock-server wire-protocol tests, per-tool
  output tests, and the headless Blender integration test.

## [1.0.0] - 2026-07-XX

- Initial release: 1:1 Rust port of the BlenderMCP standalone MCP server
  (rmcp), telemetry removed.
