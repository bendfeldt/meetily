# Third-party notices (fork-only agent tooling)

Everything under `.claude/` is agent tooling for this fork, not part of the
meetily application. It includes adapted copies of MIT-licensed work:

| Material | Source | License |
|---|---|---|
| `skills/*` except `ponytail*` (loopkit skills, adapted to meetily), `commands/`, `agents/verifier.md`, `hooks/pre-compact`, `run.sh` at the repo root | [Archive228/loopkit](https://github.com/Archive228/loopkit), via bendfeldt/fabric-connect | MIT, see [`skills/LOOPKIT-LICENSE`](skills/LOOPKIT-LICENSE) |
| `hooks/session-start` (pattern) | [obra/superpowers](https://github.com/obra/superpowers) | MIT, see [`hooks/SUPERPOWERS-LICENSE`](hooks/SUPERPOWERS-LICENSE) |
| `skills/ponytail*` | [DietrichGebert/ponytail](https://github.com/DietrichGebert/ponytail) | MIT, see [`skills/ponytail/LICENSE`](skills/ponytail/LICENSE) |

meetily itself is MIT licensed by Zackriya Solutions (`LICENSE.md`, unchanged).
