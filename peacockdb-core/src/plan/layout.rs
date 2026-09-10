//! What a node declares about its output: how many lanes, how rows were routed into
//! them, what order they carry and whether a lane is one batch. Declarations only —
//! nothing here executes, and the vocabulary is fixed by `llm-wiki/architecture.md`.

#[cfg(test)]
mod tests {
    use crate::plan::*;

    fn key(column: u32) -> ColumnOrder {
        ColumnOrder {
            column,
            ascending: true,
            nulls_first: false,
        }
    }

    #[test]
    fn empty_sort_columns_canonicalize_to_not_specified() {
        assert_eq!(SortOrder::batch_sorted(vec![]), SortOrder::NotSpecified);
        assert!(!SortOrder::batch_sorted(vec![]).is_batch_sorted());
        assert!(SortOrder::batch_sorted(vec![key(0)]).is_batch_sorted());
    }

    #[test]
    fn stream_sorted_is_batch_sorted_meeting_single_batch() {
        let mut layout = PartitionLayout::new(1);
        assert!(!layout.is_stream_sorted());

        layout.sort_order = SortOrder::batch_sorted(vec![key(0)]);
        assert!(
            !layout.is_stream_sorted(),
            "many batches, each sorted, is not a sorted stream"
        );

        layout.batch_layout = BatchLayout::SingleBatch;
        assert!(layout.is_stream_sorted());

        layout.sort_order = SortOrder::NotSpecified;
        assert!(
            !layout.is_stream_sorted(),
            "one batch is not an ordered one"
        );
    }

    #[test]
    fn layouts_are_equal_exactly_when_every_field_agrees() {
        let base = PartitionLayout {
            n: 4,
            key_distribution: KeyDistribution::ByHash {
                hash_keys: vec![0, 2],
            },
            sort_order: SortOrder::batch_sorted(vec![key(1)]),
            batch_layout: BatchLayout::SingleBatch,
        };
        assert_eq!(base, base.clone());

        let mut lanes = base.clone();
        lanes.n = 8;
        assert_ne!(base, lanes);

        let mut keys = base.clone();
        keys.key_distribution = KeyDistribution::ByHash {
            hash_keys: vec![2, 0],
        };
        assert_ne!(base, keys, "hash key order is the routing, not a set");

        let mut sorted = base.clone();
        sorted.sort_order = SortOrder::batch_sorted(vec![ColumnOrder {
            nulls_first: true,
            ..key(1)
        }]);
        assert_ne!(base, sorted);
    }

    #[test]
    fn hash_keys_must_be_a_subset_of_the_group_columns() {
        let hashed = KeyDistribution::ByHash {
            hash_keys: vec![1, 3],
        };
        assert!(hashed.is_subset_of(&[0, 1, 3]));
        assert!(!hashed.is_subset_of(&[1, 2]));
        assert!(!KeyDistribution::NotSpecified.is_subset_of(&[0, 1]));
    }
}
