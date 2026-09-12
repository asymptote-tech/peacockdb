//! What a node declares about its output: how many lanes, how rows were routed into
//! them, what order they carry and whether a lane is one batch. Declarations only —
//! nothing here executes, and the vocabulary is fixed by `llm-wiki/architecture.md`.

#[cfg(test)]
mod tests;
