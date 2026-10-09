//! The planner's tests at the component's own boundary: what it plans, what it refuses,
//! what it believes about NULLs, and the goldens every mode's plans are held to.
//!
//! `translator` and `memory_estimation` keep their own `tests` modules; these are the
//! cases that drive the planner through `plan`, `can_be_null` and the knobs, rather
//! than a subcomponent directly.

mod join_capability;
mod join_projection_names;
mod join_refusals;
mod logical_nullability;
mod not_in;
mod null_analysis;
mod null_key_filters;
mod plan_goldens;
