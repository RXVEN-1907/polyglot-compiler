# Graph Report - polyglot-compiler  (2026-08-24)

## Corpus Check
- Corpus is ~28,160 words - fits in a single context window. You may not need a graph.

## Summary
- 507 nodes · 1415 edges · 36 communities (31 shown, 5 thin omitted)
- Extraction: 99% EXTRACTED · 1% INFERRED · 0% AMBIGUOUS · INFERRED: 14 edges (avg confidence: 0.85)
- Token cost: 0 input · 0 output

## Community Hubs (Navigation)
- IR Core Types & Optimizations
- IR Lowering (AST to IR)
- Codegen (LLVM Backend)
- Parsing & AST Construction
- Diagnostics & Core Infrastructure
- Driver / CLI Commands
- C Preprocessor
- Lexer / Tokenizer
- Type System (Core)
- AST Node Definitions
- Semantic Analysis Helpers
- Type Layout & Integer Kinds
- Semantic Analysis (Main)
- Span / Source Location Utilities
- Example: Multi-file Math
- Test Fixtures: Math Utils
- Parser Tests
- Crate Package Metadata
- Example: Functions
- Test Fixtures: Functions
- Example: Control Flow
- Example: Structs
- Example: Arithmetic
- Example: IR Assembly
- Test Fixtures: Pointers
- Test Fixtures: Structs

## God Nodes (most connected - your core abstractions)
1. `DiagnosticEngine` - 42 edges
2. `Parser` - 38 edges
3. `Expr` - 31 edges
4. `Span` - 30 edges
5. `FnLowerer<'a>` - 29 edges
6. `Function` - 25 edges
7. `Instr` - 20 edges
8. `TranslationUnit` - 18 edges
9. `BinOp` - 18 edges
10. `Lexer` - 18 edges

## Surprising Connections (you probably didn't know these)
- `frontends()` --calls--> `lower_program()`  [INFERRED]
  crates/polyglot-driver/src/main.rs → crates/polyglot-ir/src/lower.rs
- `cmd_build()` --calls--> `optimize_module()`  [INFERRED]
  crates/polyglot-driver/src/main.rs → crates/polyglot-ir/src/opt.rs
- `cmd_emit_ir()` --calls--> `optimize_module()`  [INFERRED]
  crates/polyglot-driver/src/main.rs → crates/polyglot-ir/src/opt.rs
- `cmd_emit_ir()` --calls--> `print_module()`  [INFERRED]
  crates/polyglot-driver/src/main.rs → crates/polyglot-ir/src/print.rs
- `parse_source()` --calls--> `preprocess()`  [INFERRED]
  crates/polyglot-syntax/src/lib.rs → crates/polyglot-syntax/src/preprocess.rs

## Import Cycles
- None detected.

## Communities (36 total, 5 thin omitted)

### Community 0 - "IR Core Types & Optimizations"
Cohesion: 0.07
Nodes (43): ArgKind, Block, Function, GlobalInit, GlobalVar, Instr, Module, BlockId (+35 more)

### Community 1 - "IR Lowering (AST to IR)"
Cohesion: 0.17
Nodes (18): Binding, common_ptr_or_arith(), FnLowerer, FnLowerer<'a>, lower_global_init(), lower_program(), LvalueAddr, BlockId (+10 more)

### Community 2 - "Codegen (LLVM Backend)"
Cohesion: 0.09
Nodes (32): Builder, ClType, cl_type(), CodegenOptions, compile_modules(), convert_value(), find_cc(), FuncCodegen (+24 more)

### Community 3 - "Parsing & AST Construction"
Cohesion: 0.23
Nodes (13): DiagnosticEngine, Expr, TranslationUnit, unexpected(), const_array_len(), Parser, punct_to_binop(), HashMap (+5 more)

### Community 4 - "Diagnostics & Core Infrastructure"
Cohesion: 0.08
Nodes (25): color(), ColorChoice, Diagnostic, draw_source_line(), Label, Display, Formatter, Into (+17 more)

### Community 5 - "Driver / CLI Commands"
Cohesion: 0.18
Nodes (31): Cli, Command, OptLevelArg, Option, PathBuf, String, Vec, cache_key() (+23 more)

### Community 6 - "C Preprocessor"
Cohesion: 0.16
Nodes (19): Frame, is_conditional_directive(), Macro, Pp, Pp<'d>, preprocess(), Preprocessed, HashMap (+11 more)

### Community 7 - "Lexer / Tokenizer"
Cohesion: 0.16
Nodes (11): Lexer, Display, Formatter, Option, Result, String, Vec, Suffix (+3 more)

### Community 8 - "Type System (Core)"
Cohesion: 0.12
Nodes (6): Box, Display, Formatter, Option, Result, Type

### Community 9 - "AST Node Definitions"
Cohesion: 0.24
Nodes (16): is_lvalue(), op_str_incdec(), AssignOp, ExprKind, FunctionDef, GlobalDef, IncDec, Param (+8 more)

### Community 10 - "Semantic Analysis Helpers"
Cohesion: 0.24
Nodes (8): compatible(), const_eval(), decay_array(), FnCtx<'a>, int_lit_type(), require_scalar(), Option, usual_arith()

### Community 11 - "Type Layout & Integer Kinds"
Cohesion: 0.19
Nodes (7): IntKind, round_up(), String, Vec, StructDef, StructField, default_int_kinds()

### Community 12 - "Semantic Analysis (Main)"
Cohesion: 0.30
Nodes (13): check(), CheckedProgram, FnCtx, FuncSig, GlobalInfo, LocalSym, op_str(), resolve_global_type() (+5 more)

### Community 13 - "Span / Source Location Utilities"
Cohesion: 0.17
Nodes (6): Display, Formatter, Result, Self, Span, Self

### Community 14 - "Example: Multi-file Math"
Cohesion: 0.39
Nodes (5): main(), add(), factorial(), multiply(), power()

### Community 16 - "Parser Tests"
Cohesion: 0.60
Nodes (5): compile_unit(), parses_hello(), parses_macros_and_conditionals(), parses_structs_typedefs(), parses_switch_and_loops()

### Community 17 - "Crate Package Metadata"
Cohesion: 0.93
Nodes (6): polyglot-codegen, polyglot-core, polyglot-driver, polyglot-ir, polyglot-sema, polyglot-syntax

### Community 18 - "Example: Functions"
Cohesion: 0.70
Nodes (4): factorial(), gcd(), main(), sum_range()

### Community 19 - "Test Fixtures: Functions"
Cohesion: 0.70
Nodes (4): add(), factorial(), main(), multiply()

### Community 20 - "Example: Control Flow"
Cohesion: 0.83
Nodes (3): classify(), fibonacci(), main()

### Community 21 - "Example: Structs"
Cohesion: 0.83
Nodes (3): main(), rectangle_area(), swap()

## Knowledge Gaps
- **1 isolated node(s):** `UnOp`
  These have ≤1 connection - possible missing edges or undocumented components.
- **5 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `Type` connect `Type System (Core)` to `IR Core Types & Optimizations`, `IR Lowering (AST to IR)`, `Codegen (LLVM Backend)`, `Parsing & AST Construction`, `AST Node Definitions`, `Semantic Analysis Helpers`, `Type Layout & Integer Kinds`, `Semantic Analysis (Main)`, `Span / Source Location Utilities`?**
  _High betweenness centrality (0.304) - this node is a cross-community bridge._
- **Why does `DiagnosticEngine` connect `Parsing & AST Construction` to `Diagnostics & Core Infrastructure`, `Semantic Analysis (Main)`, `C Preprocessor`?**
  _High betweenness centrality (0.111) - this node is a cross-community bridge._
- **Why does `Span` connect `Span / Source Location Utilities` to `Parsing & AST Construction`, `Diagnostics & Core Infrastructure`, `Lexer / Tokenizer`, `AST Node Definitions`, `Semantic Analysis Helpers`, `Semantic Analysis (Main)`?**
  _High betweenness centrality (0.087) - this node is a cross-community bridge._
- **What connects `UnOp` to the rest of the system?**
  _1 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `IR Core Types & Optimizations` be split into smaller, more focused modules?**
  _Cohesion score 0.074034902168165 - nodes in this community are weakly interconnected._
- **Should `Codegen (LLVM Backend)` be split into smaller, more focused modules?**
  _Cohesion score 0.09090909090909091 - nodes in this community are weakly interconnected._
- **Should `Diagnostics & Core Infrastructure` be split into smaller, more focused modules?**
  _Cohesion score 0.08084163898117387 - nodes in this community are weakly interconnected._