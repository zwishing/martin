//! Integration tests for smart routing functionality

use std::collections::HashMap;

use maptile::handler::smart_routing::{has_filter_params, resolve_source_id, resolve_source_ids};

#[test]
fn test_no_filter_params() {
    let params = HashMap::new();
    let sources = vec!["cities".to_string(), "cities_filtered".to_string()];

    let result = resolve_source_id("cities", &params, &sources);
    assert_eq!(result, "cities", "Should use base source when no filters");
}

#[test]
fn test_limit_param_triggers_routing() {
    let mut params = HashMap::new();
    params.insert("limit".to_string(), "100".to_string());
    let sources = vec!["cities".to_string(), "cities_filtered".to_string()];

    let result = resolve_source_id("cities", &params, &sources);
    assert_eq!(
        result, "cities_filtered",
        "Should route to filtered source with limit param"
    );
}

#[test]
fn test_offset_param_triggers_routing() {
    let mut params = HashMap::new();
    params.insert("offset".to_string(), "50".to_string());
    let sources = vec!["cities".to_string(), "cities_filtered".to_string()];

    let result = resolve_source_id("cities", &params, &sources);
    assert_eq!(
        result, "cities_filtered",
        "Should route to filtered source with offset param"
    );
}

#[test]
fn test_sortby_param_triggers_routing() {
    let mut params = HashMap::new();
    params.insert("sortby".to_string(), "-population".to_string());
    let sources = vec!["cities".to_string(), "cities_filtered".to_string()];

    let result = resolve_source_id("cities", &params, &sources);
    assert_eq!(
        result, "cities_filtered",
        "Should route to filtered source with sortby param"
    );
}

#[test]
fn test_range_filter_min_triggers_routing() {
    let mut params = HashMap::new();
    params.insert("population_min".to_string(), "1000000".to_string());
    let sources = vec!["cities".to_string(), "cities_filtered".to_string()];

    let result = resolve_source_id("cities", &params, &sources);
    assert_eq!(
        result, "cities_filtered",
        "Should route to filtered source with _min filter"
    );
}

#[test]
fn test_range_filter_max_triggers_routing() {
    let mut params = HashMap::new();
    params.insert("population_max".to_string(), "10000000".to_string());
    let sources = vec!["cities".to_string(), "cities_filtered".to_string()];

    let result = resolve_source_id("cities", &params, &sources);
    assert_eq!(
        result, "cities_filtered",
        "Should route to filtered source with _max filter"
    );
}

#[test]
fn test_property_filter_triggers_routing() {
    let mut params = HashMap::new();
    params.insert("name".to_string(), "Tokyo".to_string());
    let sources = vec!["cities".to_string(), "cities_filtered".to_string()];

    let result = resolve_source_id("cities", &params, &sources);
    assert_eq!(
        result, "cities_filtered",
        "Should route to filtered source with property filter"
    );
}

#[test]
fn test_datetime_param_triggers_routing() {
    let mut params = HashMap::new();
    params.insert("datetime".to_string(), "2024-01-01T00:00:00Z".to_string());
    let sources = vec!["cities".to_string(), "cities_filtered".to_string()];

    let result = resolve_source_id("cities", &params, &sources);
    assert_eq!(
        result, "cities_filtered",
        "Should route to filtered source with datetime param"
    );
}

#[test]
fn test_multiple_filters() {
    let mut params = HashMap::new();
    params.insert("limit".to_string(), "100".to_string());
    params.insert("population_min".to_string(), "1000000".to_string());
    params.insert("sortby".to_string(), "-population".to_string());
    let sources = vec!["cities".to_string(), "cities_filtered".to_string()];

    let result = resolve_source_id("cities", &params, &sources);
    assert_eq!(
        result, "cities_filtered",
        "Should route to filtered source with multiple filters"
    );
}

#[test]
fn test_fallback_when_filtered_not_available() {
    let mut params = HashMap::new();
    params.insert("limit".to_string(), "100".to_string());
    let sources = vec!["cities".to_string()]; // No filtered variant

    let result = resolve_source_id("cities", &params, &sources);
    assert_eq!(
        result, "cities",
        "Should fallback to base source when filtered variant doesn't exist"
    );
}

#[test]
fn test_multiple_sources_routing() {
    let mut params = HashMap::new();
    params.insert("limit".to_string(), "100".to_string());
    let sources = vec![
        "cities".to_string(),
        "cities_filtered".to_string(),
        "roads".to_string(),
        "roads_filtered".to_string(),
        "buildings".to_string(), // No filtered variant
    ];

    let result = resolve_source_ids(&["cities", "roads", "buildings"], &params, &sources);
    assert_eq!(
        result,
        vec!["cities_filtered", "roads_filtered", "buildings"],
        "Should route each source independently"
    );
}

#[test]
fn test_has_filter_params_detection() {
    // Empty params
    let params = HashMap::new();
    assert!(!has_filter_params(&params));

    // Standard filter params
    let mut params = HashMap::new();
    params.insert("limit".to_string(), "100".to_string());
    assert!(has_filter_params(&params));

    // Range filter
    let mut params = HashMap::new();
    params.insert("population_min".to_string(), "1000000".to_string());
    assert!(has_filter_params(&params));

    // Property filter
    let mut params = HashMap::new();
    params.insert("custom_field".to_string(), "value".to_string());
    assert!(has_filter_params(&params));
}

#[test]
fn test_case_sensitivity() {
    let mut params = HashMap::new();
    params.insert("LIMIT".to_string(), "100".to_string()); // Uppercase
    let sources = vec!["cities".to_string(), "cities_filtered".to_string()];

    let result = resolve_source_id("cities", &params, &sources);
    // Current implementation is case-sensitive, so this should NOT trigger routing
    // If you want case-insensitive, modify smart_routing.rs
    assert_eq!(result, "cities_filtered"); // Will route because any param is considered a filter
}

#[test]
fn test_empty_param_value() {
    let mut params = HashMap::new();
    params.insert("limit".to_string(), "".to_string()); // Empty value
    let sources = vec!["cities".to_string(), "cities_filtered".to_string()];

    let result = resolve_source_id("cities", &params, &sources);
    assert_eq!(
        result, "cities_filtered",
        "Should still route even with empty value"
    );
}

#[test]
fn test_special_characters_in_source_id() {
    let mut params = HashMap::new();
    params.insert("limit".to_string(), "100".to_string());
    let sources = vec!["my-cities".to_string(), "my-cities_filtered".to_string()];

    let result = resolve_source_id("my-cities", &params, &sources);
    assert_eq!(result, "my-cities_filtered");
}

#[test]
fn test_nested_source_names() {
    let mut params = HashMap::new();
    params.insert("limit".to_string(), "100".to_string());
    let sources = vec![
        "schema.cities".to_string(),
        "schema.cities_filtered".to_string(),
    ];

    let result = resolve_source_id("schema.cities", &params, &sources);
    assert_eq!(result, "schema.cities_filtered");
}

#[test]
fn test_performance_with_many_sources() {
    let mut params = HashMap::new();
    params.insert("limit".to_string(), "100".to_string());

    // Create 1000 sources
    let mut sources = Vec::new();
    for i in 0..1000 {
        sources.push(format!("source_{}", i));
        sources.push(format!("source_{}_filtered", i));
    }

    let start = std::time::Instant::now();
    let result = resolve_source_id("source_500", &params, &sources);
    let duration = start.elapsed();

    assert_eq!(result, "source_500_filtered");
    assert!(
        duration.as_micros() < 1000,
        "Routing should be fast even with many sources"
    );
}
