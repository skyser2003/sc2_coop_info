# General
- Always answer in English.
- Before running any actual runtime commands which might require env variable (ex - unit tests, npm, cargo, tasks, etc), read in and apply .env or .envrc file if exists.
- After each run, if formatter exists (ex - prettier, cargo fmt), run it before you end your task.
- Write tests in a separate test file/path.
- For compiled languages, always check if the result compiles.
- After each job done, make sure to run tests to ensure it is done correctly.
- Always use strict typing.  Do not use any/unknown sort of type bypassing hack.
- Even if you are using languages like python/typescript, make everything strictly typed.  Do not use types like object, any, unknown, etc vague types if possible.  Write structs and types as you would do in rust.

# Project specific
- Make sure all codes and tests run in windows/mac/linux.
- Run tests in release mode.
- ALWAYS retain parallelization when doing full folder analysis and parsing.
- Do not use #[path="..."] when importing other file or module in rust.
- Put all tests under tests/ folder.