# bf_search

Brainfuck program search via lazy partial-program expansion, with
structural sharing of memory and AST. Given a target byte sequence, the
tool searches for a short, efficient Brainfuck program that produces the
sequence (and often extrapolates beyond it).

- Input defaults to decimal bytes (e.g., `0 1 2 3`). Hex is optional
  via `--hex`.
- Best-first search using the score:
  score = correct − β · min_len − γ · log2(steps + 1)
- Structural sharing:
  - AST nodes shared with `Rc` and stable node IDs
  - Tape is a sparse persistent map (`im::HashMap<i64, u8>`)
- Pruning:
  - Any output mismatch is pruned
  - Premature halts (before producing full target) are pruned
  - `,` (input) is not supported and is pruned

See `spec.md` for the theory and search spec behind this program. You’ll
provide that file separately.

## Installation

- Build locally:

```bash
cargo build --release
```

- Run in place:

```bash
cargo run --release -- 0 1 2 3
```

- Or install from your fork/clone:

```bash
cargo install --path .
```

## Usage

```text
bf_search [OPTIONS] [BYTE]...

Positional arguments:
  BYTE...     Target byte sequence in decimal (0..=255).
              Space-separated or comma-delimited.
              Examples: 0 1 2 3   or   "0,1,2,3"

Options:
  -x, --hex <HEX>        Provide the target as hex (e.g., "00010203" or
                         "00 01 02 03"). If given, overrides decimal bytes.
  -e, --extra <N>        Extra bytes to display beyond the input length for
                         extrapolation (default: 64)
  -b, --beta <BETA>      β in score (#correct − β·len − γ·log2(steps+1))
                         (default: 1.0)
  -g, --gamma <GAMMA>    γ in score (#correct − β·len − γ·log2(steps+1))
                         (default: 1.0)
      --max-steps <N>    Safety cap on interpreter steps per search node
                         (default: 1_000_000)
      --demo-steps <N>   Safety cap on interpreter steps during solution
                         demo (default: 1_000_000)
  -h, --help             Print help
  -V, --version          Print version
```

Examples:

```bash
# Decimal bytes (default)
bf_search 0 1 2 3

# Decimal bytes from a single quoted argument (comma-delimited)
bf_search "0,1,2,3"

# Hex target
bf_search --hex "00 01 02 03"
bf_search --hex 00010203

# Adjust scoring weights and shown extrapolation length
bf_search -b 1.0 -g 1.0 -e 64 0 1 2 3
```

## Sample run

This is a real example run that finds a short program for
`0 1 2 3 2 1 0 1 2 3 2` and shows extrapolated output.

```text
$ cargo run --release -- 0 1 2 3 2 1 0 1 2 3 2
   Compiling bf_search v0.1.0 (/Users/lucina/Brainfuck spec)
    Finished `release` profile [optimized] target(s) in 1.43s
     Running `target/release/bf_search 0 1 2 3 2 1 0 1 2 3 2`
Target length: 11 bytes
Scoring: score = correct - 1.000 * min_len - 1.000 * log2(steps + 1)
Press Ctrl+C to stop at any time.

Solution #1 found:
Program length (inst): 15
Program (Brainfuck):
.+.[+.+[.-].+.]

Output (first 75 bytes shown):
DEC  : 0 1 2 3 2 1 0 1 2 3 2 1 0 1 2 3 2 1 0 1 2 3 2 1 0 1 2 3 2 1 0 1 2 3 2 1 0 1 2 3 2 1 0 1 2 3 2 1 0 1 2 3 2 1 0 1 2 3 2 1 0 1 2 3 2 1 0 1 2 3 2 1 0 1 2
Interpreter steps during demo: 210 (halted: false)

Press Enter to search for the next different solution (or 'q' + Enter to quit):
```

Tip: After each solution is printed, press Enter to continue searching
for another different solution, or type `q` then Enter to quit.

## How it works (short)

- Grammar:
  - I := > | < | + | - | . | ,
  - P := Empty | I;P | [P];P
- Start with a single hole P. When the interpreter needs the next
  instruction, lazily expand that hole into one of:
  - `Empty`, `I;P` (for each I), or `[P];P`
- Best-first search (priority queue) by score:
  - `score = correct − β·min_len − γ·log2(steps + 1)`
- Pruning:
  - Output mismatch or premature halt => drop the branch
  - `,` (input) unsupported => drop the branch
- Sharing:
  - AST nodes `Rc`-shared. Each node has a stable ID; loops store these
    IDs to jump consistently even after expansions.
  - Tape is sparse and persistent (`im::HashMap`), so children inherit
    their parent’s tape structurally without copying.

## Limitations

- The Brainfuck input instruction `,` is pruned (unsupported).
- Search can be expensive on long targets; the scoring function and caps
  help, but expect exponential behavior in the worst case.
- Beware of non-terminating programs; demo execution has a step cap.

## Contributing

Issues and PRs are welcome. If you add features or change interfaces,
please update this README and the `--help` output accordingly.

## License

MIT. See `LICENSE`.
