//! The operator harness's device half: a session over one operator's recipe, an upload,
//! a fetch, and the same call sequence run on both backends.

#[macro_use]
mod coverage;
mod accumulate_cases;
mod aggregate_cases;
mod device;
mod emit_cases;
mod exec_cases;
mod harness_cases;
mod script;
