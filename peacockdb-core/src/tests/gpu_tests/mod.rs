//! The operator harness's device half: a session over one operator's recipe, an upload,
//! a fetch, and the same call sequence run on both backends.

#[macro_use]
mod coverage;
mod accumulate_cases;
mod accumulate_schema_cases;
mod aggregate_cases;
mod aggregate_dimension_cases;
mod aggregate_identity_cases;
mod aggregate_schema_cases;
mod device;
mod distinct_cases;
mod emit_cases;
mod emit_schema_cases;
mod exec_cases;
mod exec_schema_cases;
mod harness_cases;
mod harness_schema_cases;
mod join_cases;
mod join_dimension_cases;
mod join_key_type_cases;
mod join_schema_cases;
mod join_shape_cases;
mod nested_cases;
mod nested_schema_cases;
mod script;
mod source_cases;
mod source_schema_cases;
mod welford_cases;
mod whole_plan;
