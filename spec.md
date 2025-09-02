| Character | Instruction Performed                                                                        |
| --------- | -------------------------------------------------------------------------------------------- |
| `>`       | Increment the data pointer by one (to point to the next cell to the right).                  |
| `<`       | Decrement the data pointer by one (to point to the next cell to the left).                   |
| `+`       | Increment the byte at the data pointer by one.                                               |
| `-`       | Decrement the byte at the data pointer by one.                                               |
| `.`       | Output the byte at the data pointer.                                                         |
| `,`       | Accept one byte of input, storing its value in the byte at the data pointer.                 |
| `[`       | If the byte at the data pointer is zero, jump forward to the command after the matching `]`. |
| `]`       | If the byte at the data pointer is nonzero, jump back to the command after the matching `[`. |

I := > | < | + | - | . | ,
P := Empty | I;P | [P];P

A program is a fully concrete program, such as Empty;+;-
A partial program is a program that may or may not be fully concrete, such as <;I;Empty or [P];<;Empty or ,;Empty
A partial program represents a set of possible concrete instantiations.

The length of a partial program is the minimum possible length that an instantiation can be, so [P];<;Empty would have a length of 3, because the instantiation [Empty];<;Empty has a length of 3 since Empty has a length of 0.

For partial programs A and B, A ⊆ B means that A is a sub program of B, that all possible instantiations of A are a possible instantiations of B.

A = B means that A ⊆ B and B ⊆ A.

Other concepts from set theory follow in an analagous manner, such as unions and intersections.

For efficient Brainfuck program enumeration and execution, start with the partial program P.
Run one step of the interpreter. It needs the first instruction, which has not been determined.
So expand the tree to Empty | {I};P | [P];P
{I} meaning expansions for each possible instruction.
Now the first instruction is determined and the interpreter branches into multiple as each executes a different instruction.
Now each interpreter is run another step, but the second instruction has yet to be determined.
Branch for each interpreter again and execute all possible second instructions.
Child nodes of the tree share significant state with their parents, so memory and execution state can be shared to a large extent, minimizing repeated computations.
Expansions are only made as needed, for example, the P in [P];Empty can be skipped and be left unexpanded if the byte at the data pointer is zero.

This can relatively efficiently find a program that compresses a sequence of bytes.
Let every node of the execution tree have a score, #correct - β * partial program length - γ * log2(#interpreter steps + 1),
where #correct is the number of correct output bytes generated so far and #interpreter steps is the number of interpreter steps taken so far.
At every step of program enumeration, the node with the highest score is taken and advanced, expanding the tree as needed.
If even a single output is incorrect or an interpreter halts prematurely, then the score is effectively -inf and the node/partial program is never further considered.
This excludes programs that generate invalid sequences and privileges efficient programs.
Once a valid program is found, the original sequence can be extrapolated.
