//! `merge_m2`: state into state, which none of DataFusion's aggregate modes does.
//!
//! Partial takes values and emits state, Final takes state and emits a value, Single does
//! both at once. The engine stacks two merges — per lane, then across lanes — so the lower
//! one must emit the state the upper one reads: the gap `AggregateMode::Merge` fills.
//!
//! The accumulator is DataFusion's own with one method rewired: what arrives is state, so
//! `update_batch` merges it. No part of Welford is written out here, because the device
//! runs cuDF's MERGE_M2 and two spellings differ in the last digits. The count is `Int64`
//! outside, as every count, and `u64` inside, so it is cast on the way in and out.

use std::any::Any;
use std::sync::Arc;

use datafusion::arrow::array::ArrayRef;
use datafusion::arrow::compute::cast;
use datafusion::arrow::datatypes::{DataType, Field};
use datafusion::common::{Result as DfResult, ScalarValue};
use datafusion::functions_aggregate::stddev::stddev_udaf;
use datafusion::logical_expr::function::{AccumulatorArgs, StateFieldsArgs};
use datafusion::logical_expr::{
    Accumulator, AggregateUDF, AggregateUDFImpl, Signature, Volatility,
};

/// What this aggregate is called. Ours rather than DataFusion's: nothing in a session
/// registers it, and the CPU backend hands the definition to the aggregate it builds.
pub(crate) const NAME: &str = "merge_m2";

pub(crate) fn udaf() -> Arc<AggregateUDF> {
    Arc::new(AggregateUDF::from(MergeM2::new()))
}

#[derive(Debug)]
struct MergeM2 {
    signature: Signature,
}

impl MergeM2 {
    fn new() -> Self {
        Self {
            // The Welford state as the plan declares it, in DataFusion's order: a count and
            // two doubles. Exact rather than coercible — a state column arriving as another
            // type is a decomposition that has gone wrong upstream, and coercing it would
            // merge the wrong numbers rather than say so.
            signature: Signature::exact(
                vec![DataType::Int64, DataType::Float64, DataType::Float64],
                Volatility::Immutable,
            ),
        }
    }
}

impl AggregateUDFImpl for MergeM2 {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn name(&self) -> &str {
        NAME
    }

    fn signature(&self) -> &Signature {
        &self.signature
    }

    /// What a finalize would produce. Nothing reaches it: the engine finalizes in a
    /// project, so the aggregate is only ever asked for its state.
    fn return_type(&self, _args: &[DataType]) -> DfResult<DataType> {
        Ok(DataType::Float64)
    }

    fn state_fields(&self, args: StateFieldsArgs) -> DfResult<Vec<Field>> {
        Ok(vec![
            Field::new(format!("{}[count]", args.name), DataType::Int64, true),
            Field::new(format!("{}[mean]", args.name), DataType::Float64, true),
            Field::new(format!("{}[m2]", args.name), DataType::Float64, true),
        ])
    }

    fn accumulator(&self, args: AccumulatorArgs) -> DfResult<Box<dyn Accumulator>> {
        Ok(Box::new(Merging {
            inner: stddev_udaf().accumulator(args)?,
        }))
    }
}

/// DataFusion's own stddev accumulator, reading its input as state rather than as values.
#[derive(Debug)]
struct Merging {
    inner: Box<dyn Accumulator>,
}

/// The state with its count in the type DataFusion's accumulator reads, `u64`.
fn counted_unsigned(states: &[ArrayRef]) -> DfResult<Vec<ArrayRef>> {
    let count = cast(&states[0], &DataType::UInt64)?;
    Ok(vec![count, Arc::clone(&states[1]), Arc::clone(&states[2])])
}

impl Accumulator for Merging {
    /// The rewired method, and the whole of what this type is for.
    fn update_batch(&mut self, values: &[ArrayRef]) -> DfResult<()> {
        self.inner.merge_batch(&counted_unsigned(values)?)
    }

    fn merge_batch(&mut self, states: &[ArrayRef]) -> DfResult<()> {
        self.inner.merge_batch(&counted_unsigned(states)?)
    }

    fn state(&mut self) -> DfResult<Vec<ScalarValue>> {
        let mut state = self.inner.state()?;
        state[0] = state[0].cast_to(&DataType::Int64)?;
        Ok(state)
    }

    fn evaluate(&mut self) -> DfResult<ScalarValue> {
        self.inner.evaluate()
    }

    fn size(&self) -> usize {
        self.inner.size()
    }
}
