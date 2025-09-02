use clap::Parser;
use im::HashMap as ImHashMap;
use ordered_float::NotNan;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashSet};
use std::io::{self, Write};
use std::rc::Rc;

#[derive(Parser, Debug, Clone)]
struct Args {
    /// Provide the target as a hex string (e.g., "00010203" or "00 01 02 03")
    #[arg(short = 'x', long = "hex", value_name = "HEX")]
    hex: Option<String>,

    /// Target byte sequence in decimal (0..=255). Space-separated or comma-delimited.
    /// Examples: 0 1 2 3    or: "0,1,2,3"
    #[arg(
        value_name = "BYTE",
        value_parser = clap::value_parser!(u8),
        num_args = 1..,
        value_delimiter = ',',
        required_unless_present = "hex"
    )]
    bytes: Vec<u8>,

    /// Extra bytes to display beyond the input length for extrapolation
    #[arg(short = 'e', long = "extra", default_value_t = 64)]
    extra: usize,

    /// Beta coefficient in score (#correct − β·len − γ·log2(steps+1))
    #[arg(short = 'b', long = "beta", default_value_t = 1.0)]
    beta: f64,

    /// Gamma coefficient in score (#correct − β·len − γ·log2(steps+1))
    #[arg(short = 'g', long = "gamma", default_value_t = 1.0)]
    gamma: f64,

    /// Safety cap on interpreter steps for any node
    #[arg(long = "max-steps", default_value_t = 1_000_000)]
    max_steps: u64,

    /// Safety cap on steps when running the concrete solution for display
    #[arg(long = "demo-steps", default_value_t = 1_000_000)]
    demo_steps: u64,
}

#[derive(Clone, Copy, Debug)]
enum Instr {
    IncPtr,
    DecPtr,
    Inc,
    Dec,
    Output,
    Input,
}

impl Instr {
    fn all() -> &'static [Instr] {
        &[
            Instr::IncPtr,
            Instr::DecPtr,
            Instr::Inc,
            Instr::Dec,
            Instr::Output,
            Instr::Input,
        ]
    }

    fn to_char(self) -> char {
        match self {
            Instr::IncPtr => '>',
            Instr::DecPtr => '<',
            Instr::Inc => '+',
            Instr::Dec => '-',
            Instr::Output => '.',
            Instr::Input => ',',
        }
    }
}

#[derive(Clone)]
struct ProgramNode {
    nid: u32, // stable node id
    kind: PKind,
    min_len: u32, // minimal possible length of any instantiation of this P
}

#[derive(Clone)]
enum PKind {
    Hole,
    Empty,
    Instr(Instr, Rc<ProgramNode>), // I;P
    Loop {
        body: Rc<ProgramNode>, // [P];P
        next: Rc<ProgramNode>,
    },
}

impl ProgramNode {
    fn hole_with_id(id: u32) -> Rc<ProgramNode> {
        Rc::new(ProgramNode {
            nid: id,
            kind: PKind::Hole,
            min_len: 0,
        })
    }
    fn empty_with_id(id: u32) -> Rc<ProgramNode> {
        Rc::new(ProgramNode {
            nid: id,
            kind: PKind::Empty,
            min_len: 0,
        })
    }
    fn instr_with_id(id: u32, i: Instr, next: Rc<ProgramNode>) -> Rc<ProgramNode> {
        Rc::new(ProgramNode {
            nid: id,
            kind: PKind::Instr(i, next.clone()),
            min_len: 1 + next.min_len,
        })
    }
    fn loop_with_id(id: u32, body: Rc<ProgramNode>, next: Rc<ProgramNode>) -> Rc<ProgramNode> {
        Rc::new(ProgramNode {
            nid: id,
            kind: PKind::Loop {
                body: body.clone(),
                next: next.clone(),
            },
            min_len: 2 + body.min_len + next.min_len,
        })
    }

    fn concretize_min(&self) -> Rc<ProgramNode> {
        match &self.kind {
            PKind::Hole => ProgramNode::empty_with_id(self.nid),
            PKind::Empty => ProgramNode::empty_with_id(self.nid),
            PKind::Instr(i, next) => {
                ProgramNode::instr_with_id(self.nid, *i, next.concretize_min())
            }
            PKind::Loop { body, next } => {
                ProgramNode::loop_with_id(
                    self.nid,
                    body.concretize_min(),
                    next.concretize_min(),
                )
            }
        }
    }

    fn to_bf_string(root: &Rc<ProgramNode>) -> String {
        let mut s = String::new();
        fn rec(node: &Rc<ProgramNode>, out: &mut String) {
            match &node.kind {
                PKind::Hole => {
                    // In a concrete program we shouldn't have holes. If any, treat as end.
                }
                PKind::Empty => {}
                PKind::Instr(i, next) => {
                    out.push(i.to_char());
                    rec(next, out);
                }
                PKind::Loop { body, next } => {
                    out.push('[');
                    rec(body, out);
                    out.push(']');
                    rec(next, out);
                }
            }
        }
        rec(root, &mut s);
        s
    }
}

fn replace_hole(root: &Rc<ProgramNode>, target_id: u32, replacement: Rc<ProgramNode>) -> Rc<ProgramNode> {
    fn rec(cur: &Rc<ProgramNode>, tid: u32, rep: &Rc<ProgramNode>) -> (Rc<ProgramNode>, bool) {
        match &cur.kind {
            PKind::Hole => {
                if cur.nid == tid {
                    (rep.clone(), true)
                } else {
                    (cur.clone(), false)
                }
            }
            PKind::Empty => (cur.clone(), false),
            PKind::Instr(i, next) => {
                let (new_next, chg) = rec(next, tid, rep);
                if chg {
                    // preserve this node's id
                    (
                        ProgramNode::instr_with_id(cur.nid, *i, new_next),
                        true,
                    )
                } else {
                    (cur.clone(), false)
                }
            }
            PKind::Loop { body, next } => {
                let (new_body, chg_b) = rec(body, tid, rep);
                let (new_next, chg_n) = rec(next, tid, rep);
                if chg_b || chg_n {
                    (
                        ProgramNode::loop_with_id(cur.nid, new_body, new_next),
                        true,
                    )
                } else {
                    (cur.clone(), false)
                }
            }
        }
    }
    let (new_root, changed) = rec(root, target_id, &replacement);
    if !changed {
        panic!("Hole id {} not found in AST", target_id);
    }
    new_root
}

fn find_by_id(root: &Rc<ProgramNode>, target_id: u32) -> Option<Rc<ProgramNode>> {
    fn dfs(n: &Rc<ProgramNode>, tid: u32) -> Option<Rc<ProgramNode>> {
        if n.nid == tid {
            return Some(n.clone());
        }
        match &n.kind {
            PKind::Hole | PKind::Empty => None,
            PKind::Instr(_, next) => dfs(next, tid),
            PKind::Loop { body, next } => dfs(body, tid).or_else(|| dfs(next, tid)),
        }
    }
    dfs(root, target_id)
}

#[derive(Clone)]
struct LoopFrame {
    body_id: u32,
    next_id: u32,
}

#[derive(Clone)]
struct SearchNode {
    root: Rc<ProgramNode>,      // partial program AST
    pc: Rc<ProgramNode>,        // P-subtree to execute next
    loop_stack: Vec<LoopFrame>, // for matching ']' semantics
    dp: i64,
    tape: ImHashMap<i64, u8>,
    steps: u64,
    outputs: Vec<u8>,
    correct: usize, // number of correct output bytes (matching prefix)
    next_id: u32, // generator for fresh node ids (holes and new nodes)
}

impl SearchNode {
    fn initial() -> SearchNode {
        let root = ProgramNode::hole_with_id(0);
        SearchNode {
            root: root.clone(),
            pc: root,
            loop_stack: Vec::new(),
            dp: 0,
            tape: ImHashMap::new(),
            steps: 0,
            outputs: Vec::new(),
            correct: 0,
            next_id: 1,
        }
    }

    fn get_cell(&self, idx: i64) -> u8 {
        *self.tape.get(&idx).unwrap_or(&0)
    }

    fn set_cell(mut tape: ImHashMap<i64, u8>, idx: i64, val: u8) -> ImHashMap<i64, u8> {
        if val == 0 {
            tape.remove(&idx);
        } else {
            tape.insert(idx, val);
        }
        tape
    }

    fn score(&self, beta: f64, gamma: f64) -> f64 {
        let len = self.root.min_len as f64;
        let steps_term = (self.steps + 1) as f64;
        (self.correct as f64) - beta * len - gamma * steps_term.log2()
    }
}

// For the priority queue
struct HeapItem {
    score: NotNan<f64>,
    seq: u64, // tie-breaker for deterministic ordering
    node: SearchNode,
}

impl PartialEq for HeapItem {
    fn eq(&self, other: &Self) -> bool {
        self.score == other.score && self.seq == other.seq
    }
}
impl Eq for HeapItem {}

impl PartialOrd for HeapItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for HeapItem {
    fn cmp(&self, other: &Self) -> Ordering {
        // Max-heap by score, then by smaller seq first
        match self.score.cmp(&other.score) {
            Ordering::Equal => self.seq.cmp(&other.seq).reverse(),
            ord => ord,
        }
    }
}

#[derive(Clone, Copy)]
enum AdvancePolicy {
    Search,     // expand holes and step
    #[allow(dead_code)]
    NoExpand,   // for demo/extrapolation: do not expand; treat holes as halt
}

fn step_once(
    node: &SearchNode,
    target: &[u8],
    policy: AdvancePolicy,
) -> Vec<SearchNode> {
    // Returns 0..N next states (children) after advancing one interpreter step
    // under the requested policy. Pruned branches return empty.
    // Note: when policy == NoExpand, encountering a hole halts (no child).
    let mut results = Vec::new();

    match &node.pc.kind {
        PKind::Hole => {
            let cur_id = node.pc.nid;
            if let AdvancePolicy::NoExpand = policy {
                // Do not expand holes in demo mode; treat as halt.
                // If hasn't produced full target, it's premature halt (prune by caller).
                return results;
            }
            // Expand: Empty, I;P, [P];P
            // 1) Empty
            {
                let replacement = ProgramNode::empty_with_id(cur_id);
                let new_root = replace_hole(&node.root, cur_id, replacement.clone());
                let mut child = node.clone();
                child.root = new_root.clone();
                child.pc = replacement;
                // No step executed (halt). Parent loop_stack unchanged.
                // Will be interpreted by caller as a halt/no-progress node.
                // If premature halt: pruned later; otherwise a solution.
                results.push(child);
            }

            // 2) For each instruction: I;P
            for &i in Instr::all() {
                let new_hole_id = node.next_id;
                let next_p = ProgramNode::hole_with_id(new_hole_id);
                let replacement = ProgramNode::instr_with_id(cur_id, i, next_p.clone());
                let new_root = replace_hole(&node.root, cur_id, replacement.clone());
                // pc should point to the replaced P-subtree (replacement)
                let mut child = node.clone();
                child.root = new_root;
                child.pc = replacement; // start at I;P
                child.next_id = new_hole_id + 1;

                // Now execute one step on this child
                let mut stepped = exec_known_step(child, target);
                results.append(&mut stepped);
            }

            // 3) Loop: [P];P
            {
                let hid1 = node.next_id;
                let hid2 = node.next_id + 1;
                let body = ProgramNode::hole_with_id(hid1);
                let next = ProgramNode::hole_with_id(hid2);
                let replacement = ProgramNode::loop_with_id(cur_id, body.clone(), next.clone());
                let new_root = replace_hole(&node.root, cur_id, replacement.clone());
                let mut child = node.clone();
                child.root = new_root;
                child.pc = replacement;
                child.next_id = hid2 + 1;

                // Execute one step for '['
                let mut stepped = exec_known_step(child, target);
                results.append(&mut stepped);
            }
        }
        _ => {
            // Known node: execute one instruction step or loop movement
            let mut stepped = exec_known_step(node.clone(), target);
            if !stepped.is_empty() {
                results.append(&mut stepped);
            } else {
                // Could be halt at Empty outside loops; nothing to add.
            }
        }
    }

    results
}

fn exec_known_step(mut node: SearchNode, target: &[u8]) -> Vec<SearchNode> {
    // Execute one interpreter step for nodes where pc is not a Hole,
    // or already expanded in caller. Return either:
    // - empty vec: halted or pruned
    // - vec with one child: advanced
    //
    // Prune if:
    // - Outputs mismatch target prefix
    // - ',' encountered (no input supported): prune branch
    //
    // Halt cases:
    // - pc is Empty and loop_stack empty => halts (no child)
    // - NoExpand policy isn't handled here; this function is called from Search mode.
    //
    // Steps count includes '[' and ']' virtual steps.
    let mut out = Vec::new();

    match &node.pc.kind {
        PKind::Empty => {
            // Either end-of-program or end-of-loop-body (']' action)
            if node.loop_stack.is_empty() {
                // Program halts
                // No child produced; caller will check if it's premature.
                return out;
            } else {
                // Execute ']' step
                node.steps = node.steps.saturating_add(1);
                let top = node.loop_stack.last().cloned().unwrap();
                let cur = node.get_cell(node.dp);
                if cur != 0 {
                    // Jump back into body start; stay in same loop
                    if let Some(p) = find_by_id(&node.root, top.body_id) {
                        node.pc = p;
                    } else {
                        return out; // body not found, halt
                    }
                } else {
                    // Exit loop
                    node.loop_stack.pop();
                    if let Some(p) = find_by_id(&node.root, top.next_id) {
                        node.pc = p;
                    } else {
                        return out; // next not found, halt
                    }
                }
                out.push(node);
                return out;
            }
        }
        PKind::Instr(i, next) => {
            node.steps = node.steps.saturating_add(1);
            match i {
                Instr::IncPtr => {
                    node.dp = node.dp.saturating_add(1);
                }
                Instr::DecPtr => {
                    node.dp = node.dp.saturating_sub(1);
                }
                Instr::Inc => {
                    let v = node.get_cell(node.dp).wrapping_add(1);
                    node.tape = SearchNode::set_cell(node.tape.clone(), node.dp, v);
                }
                Instr::Dec => {
                    let v = node.get_cell(node.dp).wrapping_sub(1);
                    node.tape = SearchNode::set_cell(node.tape.clone(), node.dp, v);
                }
                Instr::Output => {
                    let v = node.get_cell(node.dp);
                    node.outputs.push(v);
                    let idx = node.outputs.len() - 1;
                    if idx < target.len() && v != target[idx] {
                        // Mismatch => prune
                        return out;
                    }
                    if idx < target.len() {
                        node.correct = idx + 1;
                    }
                }
                Instr::Input => {
                    // No input supported; prune this branch
                    return out;
                }
            }
            node.pc = next.clone();
            out.push(node);
            return out;
        }
        PKind::Loop { body, next } => {
            // Execute '[' step
            node.steps = node.steps.saturating_add(1);
            let cur = node.get_cell(node.dp);
            if cur == 0 {
                // Skip loop
                node.pc = next.clone();
            } else {
                // Enter loop: push frame and set pc to body
                node.loop_stack.push(LoopFrame {
                    body_id: body.nid,
                    next_id: next.nid,
                });
                node.pc = body.clone();
            }
            out.push(node);
            return out;
        }
        PKind::Hole => {
            // Should be expanded by caller
            return out;
        }
    }
}

fn parse_hex_bytes(s: &str) -> Result<Vec<u8>, String> {
    let filtered: String = s
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .collect::<String>();
    if filtered.len() % 2 != 0 {
        return Err("Hex string must have an even number of hex digits".into());
    }
    let mut out = Vec::with_capacity(filtered.len() / 2);
    let bytes = filtered.as_bytes();
    for i in (0..bytes.len()).step_by(2) {
        let hi = (bytes[i] as char).to_digit(16).ok_or("Invalid hex digit")?;
        let lo = (bytes[i + 1] as char).to_digit(16).ok_or("Invalid hex digit")?;
        out.push(((hi << 4) | lo) as u8);
    }
    Ok(out)
}

fn to_dec(bytes: &[u8]) -> String {
    let mut s = String::new();
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        // Fast enough; avoids pulling an extra crate for join/map
        s.push_str(&b.to_string());
    }
    s
}

fn run_concrete_to_limit(
    root: Rc<ProgramNode>,
    limit: usize,
    step_cap: u64,
) -> (Vec<u8>, u64, bool) {
    // Run concrete (no holes) program until:
    // - output length == limit, or
    // - halt, or
    // - step_cap reached
    //
    // Returns (outputs, steps, halted_flag)
    let mut node = SearchNode {
        root: root.clone(),
        pc: root.clone(),
        loop_stack: Vec::new(),
        dp: 0,
        tape: ImHashMap::new(),
        steps: 0,
        outputs: Vec::new(),
        correct: 0,
        next_id: 0,
    };

    loop {
        if node.outputs.len() >= limit {
            return (node.outputs, node.steps, false);
        }
        if node.steps >= step_cap {
            return (node.outputs, node.steps, false);
        }
        let children = exec_known_step(node.clone(), &[]);
        if children.is_empty() {
            // Halted
            return (node.outputs, node.steps, true);
        }
        node = children.into_iter().next().unwrap();
    }
}

fn main() {
    let args = Args::parse();
    // Input preference: decimal bytes (positional). If --hex is provided, use it.
    let target: Vec<u8> = if let Some(hexstr) = args.hex.as_deref() {
        match parse_hex_bytes(hexstr) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Invalid hex input: {}", e);
                std::process::exit(2);
            }
        }
    } else {
        args.bytes.clone()
    };

    if target.is_empty() {
        eprintln!("Target sequence must not be empty. Provide decimal bytes (0..=255), e.g.:");
        eprintln!("  bf_search 0 1 2 3");
        std::process::exit(2);
    }

    println!("Target length: {} bytes", target.len());
    println!(
        "Scoring: score = correct - {:.3} * min_len - {:.3} * log2(steps + 1)",
        args.beta, args.gamma
    );
    println!("Press Ctrl+C to stop at any time.");

    let mut heap = BinaryHeap::new();
    let mut seq_counter: u64 = 0;

    let start_node = SearchNode::initial();
    let start_score = NotNan::new(start_node.score(args.beta, args.gamma)).unwrap();
    heap.push(HeapItem {
        score: start_score,
        seq: seq_counter,
        node: start_node,
    });
    seq_counter += 1;

    let mut solutions_seen: HashSet<String> = HashSet::new();
    let mut solution_index: usize = 0;

    'search: loop {
        let Some(HeapItem { node, .. }) = heap.pop() else {
            println!("Search space exhausted without finding a solution.");
            break;
        };

        // If this node already matches the full target prefix, it's a solution.
        if node.correct >= target.len() {
            // Build a concrete minimal program by setting all holes to Empty
            let concrete = node.root.concretize_min();
            let code = ProgramNode::to_bf_string(&concrete);

            if solutions_seen.contains(&code) {
                // Already reported; continue search
            } else {
                solutions_seen.insert(code.clone());
                solution_index += 1;
                println!();
                println!("Solution #{} found:", solution_index);
                println!("Program length (inst): {}", concrete.min_len);
                println!("Program (Brainfuck):");
                println!("{}", code);

                // Run the concrete program to show extrapolation
                let show_limit = target.len() + args.extra;
                let (outputs, steps, halted) =
                    run_concrete_to_limit(concrete.clone(), show_limit, args.demo_steps);

                println!();
                println!(
                    "Output (first {} bytes shown):",
                    outputs.len().min(show_limit)
                );
                println!("DEC  : {}", to_dec(&outputs));
                println!(
                    "Interpreter steps during demo: {} (halted: {})",
                    steps, halted
                );

                println!();
                print!("Press Enter to search for the next different solution (or 'q' + Enter to quit): ");
                io::stdout().flush().ok();
                let mut line = String::new();
                io::stdin().read_line(&mut line).ok();
                if line.trim().eq_ignore_ascii_case("q") {
                    break 'search;
                }
            }
        }

        // Otherwise, advance this node by one step
        // Guard against runaway nodes
        if node.steps > args.max_steps {
            continue;
        }

        let children = step_once(&node, &target, AdvancePolicy::Search);

        for child in children {
            // Prune premature halt:
            // If child halted (i.e., step did nothing) we'd have an empty vec from exec_known_step.
            // Here we only get children that advanced or are non-advancing branches
            // from expansion with Empty; detect halting outside loops:
            let halted = matches!(child.pc.kind, PKind::Empty) && child.loop_stack.is_empty();

            if halted && child.correct < target.len() {
                // premature halt: prune
                continue;
            }

            // If output mismatch already pruned in exec_known_step.

            if child.steps > args.max_steps {
                continue;
            }

            let score_val = child.score(args.beta, args.gamma);
            // Guard against NaN
            let score = match NotNan::new(score_val) {
                Ok(s) => s,
                Err(_) => continue,
            };

            heap.push(HeapItem {
                score,
                seq: seq_counter,
                node: child,
            });
            seq_counter = seq_counter.wrapping_add(1);
        }
    }
}
