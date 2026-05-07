# General
- Always answer in English.
- Before running any actual runtime commands which might require env variable (ex - unit tests, npm, cargo, tasks, etc), read in and apply .env or .envrc file if exists.
- Always keep code compilable (e.g. - cargo, tsc).
- After each run, if formatter exists (ex - prettier, cargo fmt), run it before you end your task.
- Write tests in a separate test file/path.
- For compiled languages, always check if the result compiles.
- After each job done, make sure to run tests to ensure it is done correctly.
- Always use strict typing.  Do not use any/unknown sort of type bypassing hack.
- Even if you are using languages like python/typescript, make everything strictly typed.  Do not use types like object, any, unknown, etc vague types if possible.  Write structs and types as you would do in rust.
- Put implementation of a single struct in one place unless really required to put in other places.
- Keep struct fields private, and use constructor/getter/setter to access them.
- Follow the 5 concepts of OOP - encapsulation, abstraction, inheritance, polymorphism, and composition.

# Project specific
- Make sure all codes and tests run in windows/mac/linux.
- Run tests in release mode.
- ALWAYS retain parallelization when doing full folder analysis and parsing.
- Do not use #[path="..."] when importing other file or module in rust.
- Put all tests under tests/ folder.
- DO NOT use static/global variables to share states among the process internally, like OnceLock/Mutex/etc.
- Limit cargo build cpu to half of maximum cores available.
- Frontend styling should consider both dark mode and light mode.
- After editing rust code, run 'cargo clippy' and fix simple warnings.