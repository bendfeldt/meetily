# Vendored: Ponytail

Source: https://github.com/DietrichGebert/ponytail at commit `9b58c1f`
(skills/ only), MIT licensed (see LICENSE). Covers the six `ponytail*`
skill directories in `.claude/skills/`.

Only the SKILL.md files are vendored. The upstream plugin's JavaScript hooks
(auto-activation each session, status line, `PONYTAIL_DEFAULT_MODE` /
`~/.config/ponytail/config.json`) are not installed, so Ponytail is not
auto-active: AGENTS.md makes it the default coding mode instead, and
`/ponytail lite|full|ultra` switches level. Update by re-copying
`skills/*/SKILL.md` from a newer upstream commit after reading the diff.
