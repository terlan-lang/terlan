use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;

use serde::Deserialize;

use crate::terlan_quality::terlan_polars_execution::run_terlan_consumer_projects;
use crate::terlan_quality::terlan_polars_source::resolve_terlan_polars_source;
use crate::terlan_quality::QualityResult;

const REQUIRED_FUNCTIONS: &[(&str, &str)] = &[
    ("col", "polars.expr.col"),
    ("cols", "polars.expr.cols"),
    ("all_columns", "polars.expr.all"),
    ("lit", "polars.expr.lit"),
    ("date", "polars.expr.date"),
    ("len_expr", "polars.expr.len"),
    ("year", "polars.expr.year"),
    ("mean", "polars.expr.mean"),
    ("max", "polars.expr.max"),
    ("multiply", "polars.expr.multiply"),
    ("divide", "polars.expr.divide"),
    ("floor_divide", "polars.expr.floor_divide"),
    ("pow", "polars.expr.pow"),
    ("lt", "polars.expr.lt"),
    ("gt", "polars.expr.gt"),
    ("and_predicate", "polars.expr.and_predicate"),
    ("alias", "polars.expr.alias"),
    ("suffix", "polars.expr.suffix"),
    ("prefix", "polars.expr.prefix"),
    ("exclude", "polars.expr.exclude"),
    ("round_to", "polars.expr.round"),
    ("between", "polars.expr.between"),
    ("split_first", "polars.expr.split_first"),
    ("read_csv", "polars.dataframe.read_csv"),
    ("read_csv_dates", "polars.dataframe.read_csv_dates"),
    ("write_csv", "polars.dataframe.write_csv"),
    ("from_rows", "polars.dataframe.from_rows"),
    ("height", "polars.dataframe.height"),
    ("width", "polars.dataframe.width"),
    ("columns", "polars.dataframe.columns"),
    ("rows", "polars.dataframe.rows"),
    ("schema", "polars.dataframe.schema"),
    ("filter_eq", "polars.dataframe.filter_eq"),
    ("sort_by", "polars.dataframe.sort_by"),
    ("sort_rows_by", "polars.dataframe.sort_rows_by"),
    ("top_rows_by", "polars.dataframe.top_rows_by"),
    ("bottom_rows_by", "polars.dataframe.bottom_rows_by"),
    ("group_count", "polars.dataframe.group_count"),
    ("lazy", "polars.dataframe.lazy"),
    ("where_eq", "polars.lazy_frame.filter_eq"),
    ("project", "polars.lazy_frame.select"),
    ("collect", "polars.lazy_frame.collect"),
    ("release", "polars.lazy_frame.dispose"),
    ("select", "polars.dataframe.select"),
    ("head", "polars.dataframe.head"),
    ("tail", "polars.dataframe.tail"),
    ("gather_rows", "polars.dataframe.gather_rows"),
    ("clear_rows", "polars.dataframe.clear_rows"),
    ("rechunk_frame", "polars.dataframe.rechunk_frame"),
    ("column_sums", "polars.dataframe.column_sums"),
    ("column_means", "polars.dataframe.column_means"),
    ("column_medians", "polars.dataframe.column_medians"),
    ("column_minima", "polars.dataframe.column_minima"),
    ("column_maxima", "polars.dataframe.column_maxima"),
    ("column_products", "polars.dataframe.column_products"),
    ("column_variances", "polars.dataframe.column_variances"),
    ("column_stddevs", "polars.dataframe.column_stddevs"),
    ("column_quantiles", "polars.dataframe.column_quantiles"),
    (
        "column_non_null_counts",
        "polars.dataframe.column_non_null_counts",
    ),
    ("column_lengths", "polars.dataframe.column_lengths"),
    (
        "column_unique_counts",
        "polars.dataframe.column_unique_counts",
    ),
    (
        "column_approx_unique_counts",
        "polars.dataframe.column_approx_unique_counts",
    ),
    ("select_exprs", "polars.dataframe.select_exprs"),
    (
        "select_exprs_sequential",
        "polars.dataframe.select_exprs_sequential",
    ),
    ("with_columns", "polars.dataframe.with_columns_exprs"),
    (
        "with_columns_sequential",
        "polars.dataframe.with_columns_sequential",
    ),
    ("filter", "polars.dataframe.filter_expr"),
    ("remove_where", "polars.dataframe.remove_where"),
    ("group_agg", "polars.dataframe.group_agg"),
    ("left_join", "polars.dataframe.left_join"),
    ("concat_vertical", "polars.dataframe.concat_vertical"),
    ("dispose", "polars.dataframe.dispose"),
    ("dtype_cols", "polars.expr.dtype_cols"),
    ("dtype_cols_typed", "polars.expr.dtype_cols_typed"),
    ("null_lit", "polars.expr.null"),
    ("not_expr", "polars.expr.not_expr"),
    ("is_null", "polars.expr.is_null"),
    ("n_unique", "polars.expr.n_unique"),
    ("approx_n_unique", "polars.expr.approx_n_unique"),
    ("value_counts", "polars.expr.value_counts"),
    ("unique_stable", "polars.expr.unique_stable"),
    ("unique_counts", "polars.expr.unique_counts"),
    ("add", "polars.expr.add"),
    ("subtract", "polars.expr.subtract"),
    ("modulo", "polars.expr.modulo"),
    ("lte", "polars.expr.lte"),
    ("gte", "polars.expr.gte"),
    ("equal", "polars.expr.equal"),
    ("not_equal", "polars.expr.not_equal"),
    ("or_predicate", "polars.expr.or_predicate"),
    ("xor", "polars.expr.xor"),
    ("when_then_otherwise", "polars.expr.when_then_otherwise"),
    ("date_format", "polars.expr.date_format"),
    ("uppercase_names", "polars.expr.uppercase_names"),
    ("cast", "polars.expr.cast"),
    ("cast_to", "polars.expr.cast_typed"),
    ("strict_cast", "polars.expr.strict_cast"),
    ("strict_cast_to", "polars.expr.strict_cast_typed"),
    ("parse_datetime", "polars.expr.parse_datetime"),
    ("string_len_bytes", "polars.expr.string_len_bytes"),
    ("string_len_chars", "polars.expr.string_len_chars"),
    ("string_starts_with", "polars.expr.string_starts_with"),
    ("string_contains", "polars.expr.string_contains"),
    ("string_ends_with", "polars.expr.string_ends_with"),
    ("string_extract", "polars.expr.string_extract"),
    ("string_extract_all", "polars.expr.string_extract_all"),
    ("string_replace", "polars.expr.string_replace"),
    ("string_replace_all", "polars.expr.string_replace_all"),
    ("string_titlecase", "polars.expr.string_titlecase"),
    ("string_lowercase", "polars.expr.string_lowercase"),
    ("string_uppercase", "polars.expr.string_uppercase"),
    ("string_strip_chars", "polars.expr.string_strip_chars"),
    (
        "string_strip_chars_start",
        "polars.expr.string_strip_chars_start",
    ),
    (
        "string_strip_chars_end",
        "polars.expr.string_strip_chars_end",
    ),
    ("string_strip_prefix", "polars.expr.string_strip_prefix"),
    ("string_strip_suffix", "polars.expr.string_strip_suffix"),
    ("string_slice", "polars.expr.string_slice"),
    ("string_head", "polars.expr.string_head"),
    ("string_tail", "polars.expr.string_tail"),
    ("null_count", "polars.expr.null_count"),
    ("fill_null", "polars.expr.fill_null"),
    ("fill_null_forward", "polars.expr.fill_null_forward"),
    ("fill_null_backward", "polars.expr.fill_null_backward"),
    ("interpolate", "polars.expr.interpolate"),
    ("fill_nan", "polars.expr.fill_nan"),
    ("sum", "polars.expr.sum"),
    ("fold_sum", "polars.expr.fold_sum"),
    ("fold_product", "polars.expr.fold_product"),
    ("sum_horizontal", "polars.expr.sum_horizontal"),
    ("all_horizontal", "polars.expr.all_horizontal"),
    ("concat_string", "polars.expr.concat_string"),
    ("first", "polars.expr.first"),
    ("last", "polars.expr.last"),
    ("sort_ascending", "polars.expr.sort_ascending"),
    ("rank_dense_descending", "polars.expr.rank_dense_descending"),
    ("explode", "polars.expr.explode"),
    ("filter_expression", "polars.expr.filter_expression"),
    ("sort_by_ascending", "polars.expr.sort_by_ascending"),
    ("sort_by_descending", "polars.expr.sort_by_descending"),
    ("over", "polars.expr.over"),
    ("over_explode", "polars.expr.over_explode"),
    ("head_expression", "polars.expr.head_expression"),
    ("as_struct", "polars.expr.as_struct"),
    ("struct_field", "polars.expr.struct_field"),
    ("estimated_size", "polars.dataframe.estimated_size"),
    ("frames_equal", "polars.dataframe.frames_equal"),
    (
        "frames_equal_missing",
        "polars.dataframe.frames_equal_missing",
    ),
    ("sample_rows_n", "polars.dataframe.sample_rows_n"),
    (
        "sample_rows_n_seeded",
        "polars.dataframe.sample_rows_n_seeded",
    ),
    (
        "sample_rows_fraction",
        "polars.dataframe.sample_rows_fraction",
    ),
    (
        "sample_rows_fraction_seeded",
        "polars.dataframe.sample_rows_fraction_seeded",
    ),
    ("row_is_unique", "polars.dataframe.row_is_unique"),
    ("row_is_duplicated", "polars.dataframe.row_is_duplicated"),
    ("row_hashes", "polars.dataframe.row_hashes"),
    ("row_hashes_seeded", "polars.dataframe.row_hashes_seeded"),
    ("read_parquet", "polars.dataframe.read_parquet"),
    ("write_parquet", "polars.dataframe.write_parquet"),
    ("read_json", "polars.dataframe.read_json"),
    ("write_json", "polars.dataframe.write_json"),
    ("read_ndjson", "polars.dataframe.read_ndjson"),
    ("write_ndjson", "polars.dataframe.write_ndjson"),
    ("read_ipc", "polars.dataframe.read_ipc"),
    ("write_ipc", "polars.dataframe.write_ipc"),
    ("column_series", "polars.dataframe.column_series"),
    ("series_from_strings", "polars.series.from_strings"),
    ("series_from_ints", "polars.series.from_ints"),
    ("series_from_floats", "polars.series.from_floats"),
    ("series_from_bools", "polars.series.from_bools"),
    (
        "series_from_nullable_strings",
        "polars.series.from_nullable_strings",
    ),
    (
        "series_from_nullable_ints",
        "polars.series.from_nullable_ints",
    ),
    (
        "series_from_nullable_floats",
        "polars.series.from_nullable_floats",
    ),
    (
        "series_from_nullable_bools",
        "polars.series.from_nullable_bools",
    ),
    ("series_from_dates", "polars.series.from_dates"),
    (
        "series_from_nullable_dates",
        "polars.series.from_nullable_dates",
    ),
    ("series_from_datetimes", "polars.series.from_datetimes"),
    (
        "series_from_nullable_datetimes",
        "polars.series.from_nullable_datetimes",
    ),
    ("series_empty", "polars.series.empty"),
    ("series_empty_typed", "polars.series.empty_typed"),
    ("series_name", "polars.series.name"),
    ("series_len", "polars.series.len"),
    ("series_null_count", "polars.series.null_count"),
    ("series_equal", "polars.series.equal"),
    ("series_equal_missing", "polars.series.equal_missing"),
    ("series_data_type", "polars.series.data_type"),
    ("series_values", "polars.series.values"),
    ("series_cast", "polars.series.cast"),
    ("series_cast_to", "polars.series.cast_typed"),
    ("series_to_frame", "polars.series.to_frame"),
    ("series_to_expr", "polars.series.to_expr"),
    ("dispose_series", "polars.series.dispose"),
    ("scan_csv", "polars.lazy_frame.scan_csv"),
    ("scan_parquet", "polars.lazy_frame.scan_parquet"),
    ("scan_ndjson", "polars.lazy_frame.scan_ndjson"),
    ("scan_ipc", "polars.lazy_frame.scan_ipc"),
    ("lazy_write_csv", "polars.lazy_frame.write_csv"),
    ("lazy_write_parquet", "polars.lazy_frame.write_parquet"),
    ("lazy_write_ndjson", "polars.lazy_frame.write_ndjson"),
    ("lazy_write_ipc", "polars.lazy_frame.write_ipc"),
    ("lazy_select_exprs", "polars.lazy_frame.select_exprs"),
    (
        "lazy_select_exprs_sequential",
        "polars.lazy_frame.select_exprs_sequential",
    ),
    ("lazy_with_columns", "polars.lazy_frame.with_columns"),
    (
        "lazy_with_columns_sequential",
        "polars.lazy_frame.with_columns_sequential",
    ),
    ("lazy_filter", "polars.lazy_frame.filter_expr"),
    ("lazy_remove_where", "polars.lazy_frame.remove_where"),
    ("lazy_group_agg", "polars.lazy_frame.group_agg"),
    ("lazy_sort", "polars.lazy_frame.sort"),
    ("lazy_sort_rows_by", "polars.lazy_frame.sort_rows_by"),
    ("lazy_top_rows_by", "polars.lazy_frame.top_rows_by"),
    ("lazy_bottom_rows_by", "polars.lazy_frame.bottom_rows_by"),
    ("lazy_limit", "polars.lazy_frame.limit"),
    ("lazy_tail", "polars.lazy_frame.tail"),
    ("lazy_unique_rows", "polars.lazy_frame.unique_rows"),
    ("lazy_drop_null_rows", "polars.lazy_frame.drop_null_rows"),
    ("lazy_slice_rows", "polars.lazy_frame.slice_rows"),
    ("lazy_gather_rows", "polars.lazy_frame.gather_rows"),
    ("lazy_clear_rows", "polars.lazy_frame.clear_rows"),
    ("lazy_first_row", "polars.lazy_frame.first_row"),
    ("lazy_last_row", "polars.lazy_frame.last_row"),
    ("lazy_column_sums", "polars.lazy_frame.column_sums"),
    ("lazy_column_means", "polars.lazy_frame.column_means"),
    ("lazy_column_medians", "polars.lazy_frame.column_medians"),
    ("lazy_column_minima", "polars.lazy_frame.column_minima"),
    ("lazy_column_maxima", "polars.lazy_frame.column_maxima"),
    ("lazy_column_products", "polars.lazy_frame.column_products"),
    (
        "lazy_column_variances",
        "polars.lazy_frame.column_variances",
    ),
    ("lazy_column_stddevs", "polars.lazy_frame.column_stddevs"),
    (
        "lazy_column_quantiles",
        "polars.lazy_frame.column_quantiles",
    ),
    (
        "lazy_column_non_null_counts",
        "polars.lazy_frame.column_non_null_counts",
    ),
    ("lazy_column_lengths", "polars.lazy_frame.column_lengths"),
    (
        "lazy_column_unique_counts",
        "polars.lazy_frame.column_unique_counts",
    ),
    (
        "lazy_column_approx_unique_counts",
        "polars.lazy_frame.column_approx_unique_counts",
    ),
    ("lazy_rename_columns", "polars.lazy_frame.rename_columns"),
    ("lazy_drop_columns", "polars.lazy_frame.drop_columns"),
    ("lazy_reverse_rows", "polars.lazy_frame.reverse_rows"),
    ("lazy_with_row_index", "polars.lazy_frame.with_row_index"),
    (
        "lazy_fill_null_values",
        "polars.lazy_frame.fill_null_values",
    ),
    ("lazy_fill_nan_values", "polars.lazy_frame.fill_nan_values"),
    ("lazy_drop_nan_rows", "polars.lazy_frame.drop_nan_rows"),
    ("lazy_null_counts", "polars.lazy_frame.null_counts"),
    ("lazy_schema", "polars.lazy_frame.schema"),
    ("lazy_cache", "polars.lazy_frame.cache"),
    (
        "lazy_without_optimizations",
        "polars.lazy_frame.without_optimizations",
    ),
    (
        "lazy_set_optimization",
        "polars.lazy_frame.set_optimization",
    ),
    ("lazy_shift_rows", "polars.lazy_frame.shift_rows"),
    (
        "lazy_shift_and_fill_rows",
        "polars.lazy_frame.shift_and_fill_rows",
    ),
    ("lazy_left_join", "polars.lazy_frame.left_join"),
    ("lazy_join", "polars.lazy_frame.join"),
    (
        "lazy_join_with_options",
        "polars.lazy_frame.join_with_options",
    ),
    ("lazy_join_where", "polars.lazy_frame.join_where"),
    ("lazy_explode", "polars.lazy_frame.explode"),
    ("lazy_unpivot", "polars.lazy_frame.unpivot"),
    ("lazy_unnest", "polars.lazy_frame.unnest"),
    ("describe_plan", "polars.lazy_frame.describe_plan"),
    ("describe_plan_tree", "polars.lazy_frame.describe_plan_tree"),
    ("profile_plan", "polars.lazy_frame.profile_plan"),
    ("collect_streaming", "polars.lazy_frame.collect_streaming"),
    ("unique_rows", "polars.dataframe.unique_rows"),
    ("drop_null_rows", "polars.dataframe.drop_null_rows"),
    ("slice_rows", "polars.dataframe.slice_rows"),
    ("rename_columns", "polars.dataframe.rename_columns"),
    ("drop_columns", "polars.dataframe.drop_columns"),
    ("reverse_rows", "polars.dataframe.reverse_rows"),
    ("with_row_index", "polars.dataframe.with_row_index"),
    ("fill_null_values", "polars.dataframe.fill_null_values"),
    ("fill_nan_values", "polars.dataframe.fill_nan_values"),
    ("drop_nan_rows", "polars.dataframe.drop_nan_rows"),
    ("null_counts", "polars.dataframe.null_counts"),
    ("shift_rows", "polars.dataframe.shift_rows"),
    (
        "shift_and_fill_rows",
        "polars.dataframe.shift_and_fill_rows",
    ),
    ("join", "polars.dataframe.join"),
    ("join_with_options", "polars.dataframe.join_with_options"),
    ("join_where", "polars.dataframe.join_where"),
    ("concat_horizontal", "polars.dataframe.concat_horizontal"),
    ("concat_diagonal", "polars.dataframe.concat_diagonal"),
    ("explode_columns", "polars.dataframe.explode"),
    ("unpivot", "polars.dataframe.unpivot"),
    ("unnest_columns", "polars.dataframe.unnest"),
    ("transpose", "polars.dataframe.transpose"),
    ("lazy_asof_join", "polars.lazy_frame.asof_join"),
    ("dynamic_group_agg", "polars.lazy_frame.dynamic_group_agg"),
    ("rolling_group_agg", "polars.lazy_frame.rolling_group_agg"),
    ("asof_join", "polars.dataframe.asof_join"),
    ("pivot", "polars.dataframe.pivot"),
    ("month", "polars.expr.month"),
    ("day", "polars.expr.day"),
    ("weekday", "polars.expr.weekday"),
    ("hour", "polars.expr.hour"),
    ("minute", "polars.expr.minute"),
    ("second", "polars.expr.second"),
    ("millennium", "polars.expr.millennium"),
    ("century", "polars.expr.century"),
    ("is_leap_year", "polars.expr.is_leap_year"),
    ("iso_year", "polars.expr.iso_year"),
    ("days_in_month", "polars.expr.days_in_month"),
    ("quarter", "polars.expr.quarter"),
    ("week", "polars.expr.week"),
    ("ordinal_day", "polars.expr.ordinal_day"),
    ("time_of_day", "polars.expr.time_of_day"),
    ("calendar_date", "polars.expr.calendar_date"),
    ("local_datetime", "polars.expr.local_datetime"),
    ("millisecond", "polars.expr.millisecond"),
    ("microsecond", "polars.expr.microsecond"),
    ("nanosecond", "polars.expr.nanosecond"),
    ("month_start", "polars.expr.month_start"),
    ("month_end", "polars.expr.month_end"),
    (
        "timestamp_milliseconds",
        "polars.expr.timestamp_milliseconds",
    ),
    (
        "timestamp_microseconds",
        "polars.expr.timestamp_microseconds",
    ),
    ("timestamp_nanoseconds", "polars.expr.timestamp_nanoseconds"),
    ("temporal_truncate", "polars.expr.temporal_truncate"),
    ("temporal_round", "polars.expr.temporal_round"),
    ("temporal_offset_by", "polars.expr.temporal_offset_by"),
    ("total_days", "polars.expr.total_days"),
    ("total_hours", "polars.expr.total_hours"),
    ("total_minutes", "polars.expr.total_minutes"),
    ("total_seconds", "polars.expr.total_seconds"),
    ("total_milliseconds", "polars.expr.total_milliseconds"),
    ("total_microseconds", "polars.expr.total_microseconds"),
    ("total_nanoseconds", "polars.expr.total_nanoseconds"),
    ("string_split", "polars.expr.string_split"),
    ("list_len", "polars.expr.list_len"),
    ("list_sum", "polars.expr.list_sum"),
    ("list_mean", "polars.expr.list_mean"),
    ("list_min", "polars.expr.list_min"),
    ("list_max", "polars.expr.list_max"),
    ("list_sort", "polars.expr.list_sort"),
    ("list_get", "polars.expr.list_get"),
    ("list_contains", "polars.expr.list_contains"),
    ("list_type", "polars.dtype.list"),
    ("list_type", "polars.dtype.nested_list"),
    ("array_type", "polars.dtype.array"),
    ("array_type", "polars.dtype.nested_array"),
    ("decimal_type", "polars.dtype.decimal"),
    ("data_type_field", "polars.dtype.field"),
    ("data_type_field", "polars.dtype.nested_field"),
    ("struct_type", "polars.dtype.struct_type"),
    ("dtype_cols_typed", "polars.expr.dtype_cols_nested"),
    ("cast_nested", "polars.expr.cast_nested"),
    ("strict_cast_nested", "polars.expr.strict_cast_nested"),
    ("series_empty_typed", "polars.series.empty_nested"),
    ("series_cast_nested", "polars.series.cast_nested"),
    ("categorical_type", "polars.dtype.categorical"),
    ("enum_type", "polars.dtype.enum_type"),
    ("datetime_type", "polars.dtype.datetime_tz"),
    ("array_len", "polars.expr.array_len"),
    ("array_sum", "polars.expr.array_sum"),
    ("array_mean", "polars.expr.array_mean"),
    ("array_min", "polars.expr.array_min"),
    ("array_max", "polars.expr.array_max"),
    ("array_sort", "polars.expr.array_sort"),
    ("array_to_list", "polars.expr.array_to_list"),
    ("array_get", "polars.expr.array_get"),
    ("array_contains", "polars.expr.array_contains"),
    (
        "categorical_categories",
        "polars.expr.categorical_categories",
    ),
    ("categorical_len_bytes", "polars.expr.categorical_len_bytes"),
    ("categorical_len_chars", "polars.expr.categorical_len_chars"),
    (
        "categorical_starts_with",
        "polars.expr.categorical_starts_with",
    ),
    ("categorical_ends_with", "polars.expr.categorical_ends_with"),
    ("binary_contains", "polars.expr.binary_contains"),
    ("binary_starts_with", "polars.expr.binary_starts_with"),
    ("binary_ends_with", "polars.expr.binary_ends_with"),
    ("binary_size_bytes", "polars.expr.binary_size_bytes"),
    ("binary_get", "polars.expr.binary_get"),
    ("binary_head", "polars.expr.binary_head"),
    ("binary_tail", "polars.expr.binary_tail"),
    ("categorical_slice", "polars.expr.categorical_slice"),
    (
        "categorical_slice_to_end",
        "polars.expr.categorical_slice_to_end",
    ),
    ("binary_slice", "polars.expr.binary_slice"),
    ("binary_hex_decode", "polars.expr.binary_hex_decode"),
    ("binary_hex_encode", "polars.expr.binary_hex_encode"),
    ("binary_base64_decode", "polars.expr.binary_base64_decode"),
    ("binary_base64_encode", "polars.expr.binary_base64_encode"),
    ("binary_reinterpret", "polars.expr.binary_reinterpret_typed"),
    (
        "binary_reinterpret_nested",
        "polars.expr.binary_reinterpret_nested",
    ),
    ("struct_field_at", "polars.expr.struct_field_at"),
    ("struct_fields", "polars.expr.struct_fields"),
    ("struct_rename_fields", "polars.expr.struct_rename_fields"),
    ("struct_json_encode", "polars.expr.struct_json_encode"),
    ("struct_with_fields", "polars.expr.struct_with_fields"),
    ("meta_root_names", "polars.expr.meta_root_names"),
    ("meta_output_name", "polars.expr.meta_output_name"),
    (
        "meta_has_multiple_outputs",
        "polars.expr.meta_has_multiple_outputs",
    ),
    ("meta_is_column", "polars.expr.meta_is_column"),
    (
        "meta_is_simple_projection",
        "polars.expr.meta_is_simple_projection",
    ),
    (
        "meta_is_column_selection",
        "polars.expr.meta_is_column_selection",
    ),
    ("meta_is_literal", "polars.expr.meta_is_literal"),
    (
        "meta_is_regex_projection",
        "polars.expr.meta_is_regex_projection",
    ),
    ("meta_format_tree", "polars.expr.meta_format_tree"),
    ("array_std", "polars.expr.array_std"),
    ("array_var", "polars.expr.array_var"),
    ("array_median", "polars.expr.array_median"),
    ("array_arg_min", "polars.expr.array_arg_min"),
    ("array_arg_max", "polars.expr.array_arg_max"),
    ("array_join", "polars.expr.array_join"),
    ("array_count_matches", "polars.expr.array_count_matches"),
    ("array_to_struct", "polars.expr.array_to_struct"),
    ("array_slice", "polars.expr.array_slice"),
    ("array_head", "polars.expr.array_head"),
    ("array_tail", "polars.expr.array_tail"),
    ("array_shift", "polars.expr.array_shift"),
    ("array_explode", "polars.expr.array_explode"),
    ("element", "polars.expr.element"),
    ("array_eval", "polars.expr.array_eval"),
    ("array_agg", "polars.expr.array_agg"),
    ("array_sort_with", "polars.expr.array_sort_with"),
    ("array_get_with", "polars.expr.array_get_with"),
    ("array_contains_with", "polars.expr.array_contains_with"),
    ("list_std", "polars.expr.list_std"),
    ("list_var", "polars.expr.list_var"),
    ("list_median", "polars.expr.list_median"),
    ("list_first", "polars.expr.list_first"),
    ("list_last", "polars.expr.list_last"),
    ("list_arg_min", "polars.expr.list_arg_min"),
    ("list_arg_max", "polars.expr.list_arg_max"),
    ("list_join", "polars.expr.list_join"),
    ("list_shift", "polars.expr.list_shift"),
    ("list_slice", "polars.expr.list_slice"),
    ("list_head", "polars.expr.list_head"),
    ("list_tail", "polars.expr.list_tail"),
    ("list_to_array", "polars.expr.list_to_array"),
    ("list_eval", "polars.expr.list_eval"),
    ("list_agg", "polars.expr.list_agg"),
    ("list_sort_with", "polars.expr.list_sort_with"),
    ("list_get_with", "polars.expr.list_get_with"),
    ("list_contains_with", "polars.expr.list_contains_with"),
    ("list_drop_nulls", "polars.expr.list_drop_nulls"),
    ("list_sample_n", "polars.expr.list_sample_n"),
    ("list_sample_n_seeded", "polars.expr.list_sample_n_seeded"),
    ("list_sample_fraction", "polars.expr.list_sample_fraction"),
    (
        "list_sample_fraction_seeded",
        "polars.expr.list_sample_fraction_seeded",
    ),
    ("list_gather", "polars.expr.list_gather"),
    ("list_gather_every", "polars.expr.list_gather_every"),
    ("list_diff_drop", "polars.expr.list_diff_drop"),
    ("list_diff_ignore", "polars.expr.list_diff_ignore"),
    ("list_to_struct", "polars.expr.list_to_struct"),
    ("list_count_matches", "polars.expr.list_count_matches"),
    ("list_set_union", "polars.expr.list_set_union"),
    ("list_set_difference", "polars.expr.list_set_difference"),
    ("list_set_intersection", "polars.expr.list_set_intersection"),
    (
        "list_set_symmetric_difference",
        "polars.expr.list_set_symmetric_difference",
    ),
    ("shuffle", "polars.expr.shuffle"),
    ("shuffle_seeded", "polars.expr.shuffle_seeded"),
    ("sample_n", "polars.expr.sample_n"),
    ("sample_n_seeded", "polars.expr.sample_n_seeded"),
    ("sample_fraction", "polars.expr.sample_fraction"),
    (
        "sample_fraction_seeded",
        "polars.expr.sample_fraction_seeded",
    ),
    ("bitwise_count_ones", "polars.expr.bitwise_count_ones"),
    ("bitwise_count_zeros", "polars.expr.bitwise_count_zeros"),
    ("bitwise_leading_ones", "polars.expr.bitwise_leading_ones"),
    ("bitwise_leading_zeros", "polars.expr.bitwise_leading_zeros"),
    ("bitwise_trailing_ones", "polars.expr.bitwise_trailing_ones"),
    (
        "bitwise_trailing_zeros",
        "polars.expr.bitwise_trailing_zeros",
    ),
    ("bitwise_and", "polars.expr.bitwise_and"),
    ("bitwise_or", "polars.expr.bitwise_or"),
    ("bitwise_xor", "polars.expr.bitwise_xor"),
    ("std", "polars.expr.std"),
    ("var", "polars.expr.var"),
    ("min", "polars.expr.min"),
    ("median", "polars.expr.median"),
    ("min_by", "polars.expr.min_by"),
    ("max_by", "polars.expr.max_by"),
    ("nan_min", "polars.expr.nan_min"),
    ("nan_max", "polars.expr.nan_max"),
    ("histogram_auto", "polars.expr.histogram_auto"),
    ("histogram_count", "polars.expr.histogram_count"),
    ("histogram_bins", "polars.expr.histogram_bins"),
];

pub(super) const REQUIRED_TESTS: &[&str] = &[
    "DataFrameReadCsvTest.terl",
    "DataFrameSchemaTest.terl",
    "DataFrameFilterTest.terl",
    "DataFrameSortTest.terl",
    "DataFrameGroupCountTest.terl",
    "LazyFrameTest.terl",
    "DataFrameConstructionTest.terl",
    "DataFrameCsvErrorTest.terl",
    "DataFrameRowsTest.terl",
    "DataFrameHeadTest.terl",
    "DataFrameSelectTest.terl",
    "DataFrameDisposeTest.terl",
];

/// Summary produced by the `terlan-polars` external package gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerlanPolarsPackageSummary {
    pub checked_operation_count: usize,
    pub checked_test_count: usize,
}

/// Runs the `terlan-polars` external package boundary gate.
///
/// Inputs:
/// - `root`: Terlan golden repository root.
/// - `TERLAN_POLARS_DIR`: optional path override for the external package.
///
/// Output:
/// - Success summary when the package boundary is coherent.
/// - Stable diagnostics when metadata, native adapter wiring, generated
///   boundary files, or public namespace rules drift.
///
/// Transformation:
/// - Resolves the external package, validates manifests through `basic-toml`,
///   checks the public Terlan API and generated NativeBoundary metadata, and
///   executes the package-owned Rust adapter tests.
pub fn run_terlan_polars_package(root: &Path) -> QualityResult<TerlanPolarsPackageSummary> {
    let package_root = resolve_terlan_polars_source(root)?;
    let mut diagnostics = validate_package_boundary(&package_root);
    if diagnostics.is_empty() {
        if let Err(message) = run_native_adapter_tests(&package_root) {
            diagnostics.push(message);
        }
    }
    if diagnostics.is_empty() {
        if let Err(message) = run_terlan_consumer_projects(&package_root) {
            diagnostics.push(message);
        }
    }
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    Ok(TerlanPolarsPackageSummary {
        checked_operation_count: REQUIRED_FUNCTIONS.len(),
        checked_test_count: REQUIRED_TESTS.len(),
    })
}

fn validate_package_boundary(package_root: &Path) -> Vec<String> {
    let mut diagnostics = Vec::new();
    if !package_root.exists() {
        diagnostics.push(format!(
            "terlan-polars package not found at {}",
            package_root.display()
        ));
        return diagnostics;
    }

    diagnostics.extend(validate_package_metadata(package_root));
    diagnostics.extend(validate_native_metadata(package_root));
    diagnostics.extend(validate_terlan_surface(package_root));
    diagnostics.extend(validate_generated_boundary(package_root));
    diagnostics.extend(validate_no_std_native_polars_namespace(package_root));
    diagnostics
}

fn validate_package_metadata(package_root: &Path) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let terlan_manifest = match read_toml::<TerlanManifest>(&package_root.join("terlan.toml")) {
        Ok(manifest) => manifest,
        Err(message) => {
            diagnostics.push(message);
            return diagnostics;
        }
    };
    let contract_manifest =
        match read_toml::<ContractManifest>(&package_root.join("package.contract.toml")) {
            Ok(manifest) => manifest,
            Err(message) => {
                diagnostics.push(message);
                return diagnostics;
            }
        };

    validate_common_manifest(
        "terlan.toml",
        &terlan_manifest.package,
        &terlan_manifest.native.rust,
        &mut diagnostics,
    );
    validate_common_manifest(
        "package.contract.toml",
        &contract_manifest.package.common,
        &contract_manifest.native.rust.common,
        &mut diagnostics,
    );
    validate_publication_metadata("terlan.toml", &terlan_manifest.package, &mut diagnostics);
    expect_eq(
        "package.contract.toml",
        "package.hex",
        contract_manifest.package.hex.as_deref(),
        Some("terlan_polars"),
        &mut diagnostics,
    );
    let polars = contract_manifest.native.rust.dependencies.polars;
    expect_eq(
        "package.contract.toml",
        "native.rust.dependencies.polars.cargo",
        Some(polars.cargo.as_str()),
        Some("polars"),
        &mut diagnostics,
    );
    expect_eq(
        "package.contract.toml",
        "native.rust.dependencies.polars.status",
        Some(polars.status.as_str()),
        Some("feature-gated"),
        &mut diagnostics,
    );
    diagnostics
}

fn validate_publication_metadata(
    label: &str,
    package: &PackageIdentity,
    diagnostics: &mut Vec<String>,
) {
    for (field, actual, expected) in [
        (
            "package.description",
            package.description.as_deref(),
            "Polars DataFrame integration for Terlan",
        ),
        ("package.license", package.license.as_deref(), "MIT"),
        (
            "package.repository",
            package.repository.as_deref(),
            "https://github.com/terlan-lang/terlan-polars",
        ),
        ("package.compiler", package.compiler.as_deref(), ">= 0.0.7"),
    ] {
        expect_eq(label, field, actual, Some(expected), diagnostics);
    }
    for link in ["https://terlan.org", "https://pola.rs"] {
        if !package.links.iter().any(|candidate| candidate == link) {
            diagnostics.push(format!("{label}: package.links is missing `{link}`"));
        }
    }
}

fn validate_common_manifest(
    label: &str,
    package: &PackageIdentity,
    native: &NativeRustManifest,
    diagnostics: &mut Vec<String>,
) {
    expect_eq(
        label,
        "package.name",
        Some(package.name.as_str()),
        Some("terlan-polars"),
        diagnostics,
    );
    expect_eq(
        label,
        "package.namespace",
        Some(package.namespace.as_str()),
        Some("polars"),
        diagnostics,
    );
    expect_eq(
        label,
        "native.rust.crate",
        Some(native.crate_name.as_str()),
        Some("terlan_polars_native"),
        diagnostics,
    );
    expect_eq(
        label,
        "native.rust.path",
        Some(native.path.as_str()),
        Some("native"),
        diagnostics,
    );
    expect_eq(
        label,
        "native.rust.helper",
        Some(native.helper.as_str()),
        Some("terlan-polars-native-boundary"),
        diagnostics,
    );
    expect_eq(
        label,
        "native.rust.helper_env",
        Some(native.helper_env.as_str()),
        Some("TERLAN_NATIVE_BOUNDARY_HELPER_PATH"),
        diagnostics,
    );
    if !native
        .features
        .iter()
        .any(|feature| feature == "real-polars")
    {
        diagnostics.push(format!(
            "{label}: native.rust.features must include real-polars"
        ));
    }
}
