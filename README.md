<p align="center">
  <img src="https://diegopn.github.io/potoo-site/src/assets/images/language-interpreter-icon.svg?v=1c651ca7" alt="Potoo IDE language interpreter icon" width="120" />
</p>

<h1 align="center">LSE Interpreter</h1>

<p align="center">An evidence-backed Rust interpreter for the documented historical LSE language.</p>

<p align="center">
  <a href="https://github.com/diegopn/potoo-lse-interpreter/releases">Download</a> ·
  <a href="https://diegopn.github.io/potoo-site/manual/lse/">Website</a> ·
  <a href="https://diegopn.github.io/potoo-site/manual/lse/">User manual</a>
</p>

<p align="center">
  <a href="https://www.rust-lang.org/"><img alt="Rust" src="https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white"></a>
  <a href="https://doc.rust-lang.org/cargo/"><img alt="Cargo" src="https://img.shields.io/badge/Cargo-DEA584?style=for-the-badge&logo=rust&logoColor=white"></a>
  <img alt="Rust library" src="https://img.shields.io/badge/Rust_library-4A86CF?style=for-the-badge">
  <img alt="CLI" src="https://img.shields.io/badge/CLI-6A4C93?style=for-the-badge">
</p>

This Rust package is part of the Potoo IDE ecosystem and is distributed independently from the desktop interface. Integration with the IDE has not been implemented or verified. The implementation targets selected historical references and keeps areas with incomplete semantic documentation explicit.

Program lines use the documented numeric labels from 1 through 32767, and identifiers follow the documented one-letter plus four-character limit.

## Implemented subset

| Source evidence | Implemented behavior |
| --- | --- |
| `CHAINE name`, `BOOLEEN name`, real-valued variables, and `←` assignment | Runtime declarations for string and Boolean values; numeric values by default; assignment (`<-` and `_` are accepted for keyboard/legacy text representations) |
| `ALLER EN`, `SI ... ALORS ... SINON ...`, `DEBUT` / `FIN`, `FAIRE ... POUR ...` with optional `PAS`, `JUSQUA` / `TANT QUE`, `PAUSE`, `LIBERER` | Numeric line labels, nested same-line compound statements, conditional statements, labeled loops with step and limit expressions recalculated at each loop header, host pause, release of one or more variable names |
| `LIRE` and `AFFICHER` | Host-provided input/output, multiple input variables, input prompts, and the 1981 output formats `/`, `X`, `C`, `L`, literals, repetition factors, and numeric `F`, `E`, and `U` formats |
| `GARER`, `CHARGER`, `SUPPRIMER`, `EXECUTER` | Keyed scalar or table records and external program invocation through the host; the CLI stores records in local files and has no external-program service |
| `TABLEAU` and indexed expressions/assignments | Dynamically allocated numeric, string, Boolean, and graphic tables with one through seven dimensions; 1-based bounds; `LIRE` reads every supported element in row-major storage order |
| `FORME`, `PAGE`, `MOTIF`, graphics expressions and commands | Opaque graphic values, motif registration, `:` juxtaposition, `%` superposition, the indexed vector/transform functions, and every graphics command listed in the 1982 aide-mémoire; all graphical state and rendering are delegated to `Host` |
| `PROCEDURE &name(...)`, `LOCAL`, `RESULTAT`, `RETOUR` | In-source and host-loaded procedures; recursive functions, `LOCAL` formals by value, other formals by reference, scoped locals/tables, and procedure-function parameters |
| `↑`, `*`, `/`, `+`, `-`; `RAC`, `ABS`, `ENT`, `SIN`, `COS`, `TAN`, `EXP`, `LGN`, `ATG`; `TEM`, `TZL`, `NIV`, `TYP`, `IND` | Numeric operators and mathematical functions listed in the 1982 aide-mémoire; system-information functions route through `Host::system_function` |
| `BOOLEEN`, `.VRAI.`, `.FAUX.`, `ET`, `OU`, `NON` | Boolean declarations, values, and operations listed in the 1981 comparison and 1982 aide-mémoire |
| `ICH`, `MCH`, `REP`, `TMA`, `TMI` | String reversal, replacement, repetition, and case conversion listed in the 1981 comparison and 1982 aide-mémoire |
| `!`, `SCH`, `GRL`, `CCA`, `EQC`, `DAT`, `LGR`, `POS`, `CNB`, `EQN`, `PTR`, `SKP`, `RCC`, `ALE(1)` | String, conversion, date/time, position, comparison, and random functions documented by the selected references; `RCC` also follows the later LSE 2000 collation-order description |

External procedure source is supplied by the host; the CLI looks for `<procedure-name>.lse` in the current directory. `EXECUTER` routes a program name and numeric argument through `Host::execute_program`; a host that provides this service may return an optional numeric status. Graphics hosts create opaque `GraphicValue` handles that retain the `FORME` or `PAGE` type and implement the documented operations; the CLI has no drawing surface. The selected indexed aide-mémoire lists the graphics forms and signatures but does not specify the transformation mathematics or renderer behavior, so those remain the graphics host's responsibility. The original system's external library search order is not documented in the selected sources. The 1982 index lists `DIJ`, but the available original reference does not include its truth table. The 1981 comparison documents dynamic repetition factors for `AFFICHER`, but does not specify which expression supplies such a factor in `LIRE`'s format-only form, so prompt factors are fixed. The command-line host uses `0` for successful file operations and `1` when a record or file to load/delete is absent; other hosts provide their own status values. Table allocation is bounded at one million elements, recursion at 128 calls, and `REP` at 16 MiB to keep interpreter allocations bounded. Interactive monitor commands remain outside the interpreter's program-language API. The implemented `FAIRE` loop repeats the instruction at its numeric target label; `PAS` defaults to one when omitted, and the loop step and limit expressions are reevaluated at each header as documented for LSE. A change to the loop variable in the body is applied at the next header. The `GRL` spelling is consistent in both language sources.

See [reference coverage](REFERENCE-COVERAGE.md) for the documented feature inventory and remaining source gaps.

## Library use

Call `parse(source)`, then `Interpreter::new(host).run(&program)`. The `Host` trait supplies input, output, pause, date/time, random values, keyed file records, optional external-program execution, system-information functions, and optional graphics object, function, operator, motif, and command services. `Execution` returns the final variables and number of visited source lines.

## Command line

```text
cargo run --locked -- program.lse
```

The CLI reads a UTF-8 source file, takes `LIRE` data from standard input, writes `AFFICHER` output to standard output, pauses for Enter at `PAUSE`, and stores `GARER` records in files relative to the current working directory.

## References

- [Hebenstreit and Noyelle, “Un langage symbolique destiné à l’enseignement : LSE” (1973)](https://www.epi.asso.fr/revue/06/b06p010.htm)
- [Berche, “Comparaison LSE/BASIC” (1981)](https://ife.ens-lyon.fr/publications/edition-electronique/revue-francaise-de-pedagogie/INRP_RF056_2.pdf)
- [EPI/CNDP, “Aide mémoire pour LSE / L.S.E. pour tous” (1982)](https://www.epi.asso.fr/blocnote/plaquetteLSE.pdf)
- [Arsac, “LSE 83” (1985), including a comparison of original LSE loop evaluation](https://www.epi.asso.fr/revue/38/b38p116.htm)
- [Baudé, “Le système LSE” (2015)](https://www.epi.asso.fr/revue/histo/h70-lse_jb-2015.htm)
- [LSE 2000 reference, `RCC` comparison and collation order](https://fr.wikibooks.org/wiki/Programmation_LSE)
- [Potoo LSE historical notes](https://diegopn.github.io/potoo-site/manual/lse/)

## Support

<p align="left">
  <a href="https://github.com/sponsors/diegopn"><img alt="Become a sponsor" src="https://img.shields.io/badge/Become_a_sponsor-EA4AAA?style=for-the-badge&logo=githubsponsors&logoColor=white"></a>
</p>

## Project policies

- [Code of conduct](CODE_OF_CONDUCT.md)
- [Contributing](CONTRIBUTING.md)
- [Security policy](SECURITY.md)
- [GPL-3.0-or-later license](LICENSE)
